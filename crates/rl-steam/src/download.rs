//! Downloading depots from Steam into one folder.
//!
//! The steps for each depot are the ones DepotDownloader performs: get the depot key, find a CDN
//! server, get a manifest request code, download and read the manifest, then fetch and check every
//! chunk. All depots are prepared first so the total size is known before the first byte moves,
//! and so a problem with the second depot is reported before downloading the first.

use crate::error::{Failure, FailureKind};
use rl_core::secret::Secret;
use std::path::PathBuf;
use std::sync::Arc;
use steamroom::cdn::{CdnClient, CdnServerPool};
use steamroom::client::{LoggedIn, SteamClient};
use steamroom::depot::manifest::DepotManifest;
use steamroom::depot::{AppId, CellId, DepotId, DepotKey, ManifestId};
use steamroom_client::depot_config::DepotConfig;
use steamroom_client::download::{CdnChunkFetcher, DepotJob, FileFilter};
use steamroom_client::event::DownloadEvent as LibEvent;
use steamroom_client::login::LoginBuilder;
use steamroom_client::manifest::ManifestCache;
use tokio_util::sync::CancellationToken;

const DEVICE_NAME: &str = "RustyLauncher";

#[derive(Clone)]
pub enum Auth {
    /// No account: only the current public server build can be downloaded.
    Anonymous,
    Token {
        account: String,
        refresh_token: Secret<String>,
    },
}

impl Auth {
    pub fn is_signed_in(&self) -> bool {
        matches!(self, Auth::Token { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepotSpec {
    pub depot_id: u32,
    pub manifest_id: u64,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub app_id: u32,
    /// Depots are downloaded in this order, all into `install_dir`.
    pub depots: Vec<DepotSpec>,
    pub install_dir: PathBuf,
    /// Where downloaded manifests are cached (so a resume does not fetch them again).
    pub manifest_cache_dir: PathBuf,
    /// Steam branch used to ask for manifest codes (`public` unless told otherwise).
    pub branch: String,
    /// Only fetch files whose path matches one of these regular expressions (used by tests).
    pub only_files: Option<Vec<String>>,
    /// Limit on parallel chunk downloads; `None` uses the library default.
    pub max_downloads: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    /// A step before the first byte (signing in, reading manifests).
    Preparing(String),
    /// Sizes are known. `bytes` is what the manifests add up to, not what is left to fetch.
    Total {
        bytes: u64,
        files: u64,
    },
    /// Bytes accounted for so far (downloaded or found already present) out of the total.
    Progress {
        done: u64,
        total: u64,
    },
    Finished(DownloadSummary),
    Failed(Failure),
    Cancelled,
}

#[derive(Debug, Clone, Default)]
pub struct DownloadSummary {
    pub bytes_downloaded: u64,
    pub files_completed: u64,
    pub files_skipped: u64,
}

pub(crate) async fn login(auth: &Auth) -> Result<SteamClient<LoggedIn>, Failure> {
    let builder = LoginBuilder::new().device_name(DEVICE_NAME);
    let result = match auth {
        Auth::Anonymous => builder.anonymous().login().await,
        Auth::Token {
            account,
            refresh_token,
        } => {
            builder
                .with_refresh_token(account.clone(), refresh_token.expose().clone())
                .login()
                .await
        }
    };
    result.map_err(|e| {
        let mut f = Failure::from_error(&e, auth.is_signed_in());
        // A saved token Steam no longer accepts looks like a failed logon: ask for a new sign-in.
        if auth.is_signed_in() && matches!(f.kind, FailureKind::NotOwned | FailureKind::Other) {
            f = Failure::new(
                FailureKind::NeedsSignIn,
                "Your saved Steam login is no longer valid. Please sign in again in Settings.",
                f.detail,
            );
        }
        f
    })
}

struct Prepared {
    spec: DepotSpec,
    key: DepotKey,
    manifest: DepotManifest,
    manifest_bytes: Vec<u8>,
    total: u64,
}

fn decompress_manifest(raw: &[u8]) -> Result<Vec<u8>, Failure> {
    let bad = |e: String| {
        Failure::new(
            FailureKind::Other,
            "Steam sent a manifest that could not be read.",
            e,
        )
    };
    if raw.len() > 2 && raw[0] == b'P' && raw[1] == b'K' {
        let mut zip =
            zip::ZipArchive::new(std::io::Cursor::new(raw)).map_err(|e| bad(e.to_string()))?;
        if zip.is_empty() {
            return Err(bad("empty manifest archive".into()));
        }
        let mut file = zip.by_index(0).map_err(|e| bad(e.to_string()))?;
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut out).map_err(|e| bad(e.to_string()))?;
        Ok(out)
    } else {
        Ok(raw.to_vec())
    }
}

pub(crate) async fn run(
    req: DownloadRequest,
    auth: Auth,
    emit: Arc<dyn Fn(DownloadEvent) + Send + Sync>,
    cancel: CancellationToken,
) {
    let result = tokio::select! {
        r = run_inner(&req, &auth, &emit, &cancel) => r,
        _ = cancel.cancelled() => Err(Failure::cancelled()),
    };
    match result {
        Ok(summary) => emit(DownloadEvent::Finished(summary)),
        Err(f) if f.kind == FailureKind::Cancelled => emit(DownloadEvent::Cancelled),
        Err(f) => {
            tracing::warn!(kind = ?f.kind, detail = %f.detail, "download failed");
            emit(DownloadEvent::Failed(f));
        }
    }
}

async fn run_inner(
    req: &DownloadRequest,
    auth: &Auth,
    emit: &Arc<dyn Fn(DownloadEvent) + Send + Sync>,
    cancel: &CancellationToken,
) -> Result<DownloadSummary, Failure> {
    let signed_in = auth.is_signed_in();

    emit(DownloadEvent::Preparing("Signing in to Steam...".into()));
    let client = login(auth).await?;
    let app = AppId(req.app_id);

    emit(DownloadEvent::Preparing(
        "Finding download servers...".into(),
    ));
    let cdn_servers = client
        .get_cdn_servers(CellId(0), Some(20))
        .await
        .map_err(|e| Failure::from_error(&e, signed_in))?;
    if cdn_servers.is_empty() {
        return Err(Failure::new(
            FailureKind::Network,
            "Steam did not offer any download servers right now. Try again in a minute.",
            "no CDN servers",
        ));
    }
    let first_host = cdn_servers[0].host.clone();
    let pool = CdnServerPool::new(cdn_servers.clone());
    let cdn = CdnClient::new().map_err(|e| Failure::from_error(&e, signed_in))?;
    let cache = ManifestCache::new(req.manifest_cache_dir.clone());

    // Prepare every depot before downloading anything.
    let mut prepared: Vec<Prepared> = Vec::new();
    let mut auth_tokens: Vec<Option<String>> = Vec::new();
    for spec in &req.depots {
        let depot = DepotId(spec.depot_id);
        let manifest_id = ManifestId(spec.manifest_id);
        emit(DownloadEvent::Preparing(format!(
            "Reading build information (depot {})...",
            spec.depot_id
        )));
        let key = client
            .get_depot_decryption_key(depot, app)
            .await
            .map_err(|e| Failure::from_error(&e, signed_in))?;
        let request_code = match client
            .get_manifest_request_code(app, depot, manifest_id, Some(&req.branch), None)
            .await
        {
            Ok(Some(code)) => code,
            Ok(None) => 0,
            Err(e) => {
                tracing::debug!(error = %e, depot = spec.depot_id, "no manifest request code");
                0
            }
        };
        let cdn_auth_token = match client.get_cdn_auth_token(app, depot, &first_host).await {
            Ok(t) => t.token,
            Err(e) => {
                tracing::debug!(error = %e, "no CDN auth token");
                None
            }
        };

        let manifest_bytes = if let Some(cached) = cache.load(depot, manifest_id) {
            cached
        } else {
            let raw = cdn
                .download_manifest_pooled(
                    &pool,
                    depot,
                    manifest_id,
                    request_code,
                    cdn_auth_token.as_deref(),
                )
                .await
                .map_err(|e| Failure::from_error(&e, signed_in))?;
            let decompressed = decompress_manifest(&raw)?;
            let _ = cache.save(depot, manifest_id, &decompressed, &raw);
            decompressed
        };
        let mut manifest = DepotManifest::parse(&manifest_bytes)
            .map_err(|e| Failure::from_error(&e, signed_in))?;
        if manifest.filenames_encrypted {
            manifest
                .decrypt_filenames(&key)
                .map_err(|e| Failure::from_error(&e, signed_in))?;
        }
        let total = manifest
            .total_uncompressed_size
            .unwrap_or_else(|| manifest.files.iter().map(|f| f.size).sum());
        prepared.push(Prepared {
            spec: spec.clone(),
            key,
            manifest,
            manifest_bytes,
            total,
        });
        auth_tokens.push(cdn_auth_token);
    }

    let total_bytes: u64 = prepared.iter().map(|p| p.total).sum();
    let total_files: u64 = prepared.iter().map(|p| p.manifest.files.len() as u64).sum();
    emit(DownloadEvent::Total {
        bytes: total_bytes,
        files: total_files,
    });

    // Refuse early if it cannot fit. Files that are already there (a resume) do not count again.
    let already_there = rl_core::library::dir_size(&req.install_dir);
    let needed = total_bytes.saturating_sub(already_there);
    if let Some(free) = rl_core::install::free_space(&req.install_dir) {
        const MARGIN: u64 = 512 * 1024 * 1024;
        if free < needed + MARGIN {
            return Err(Failure::new(
                FailureKind::DiskFull,
                format!(
                    "This build needs about {} of free space, but that drive only has {} free. Free some space or choose another folder.",
                    rl_core::library::format_bytes(needed + MARGIN),
                    rl_core::library::format_bytes(free)
                ),
                format!("needed {needed} bytes, {free} free"),
            ));
        }
    }

    std::fs::create_dir_all(&req.install_dir).map_err(|e| Failure::from_error(&e, signed_in))?;
    let mut summary = DownloadSummary::default();
    let mut base_done = 0u64;

    for (p, cdn_auth_token) in prepared.iter().zip(auth_tokens) {
        if cancel.is_cancelled() {
            return Err(Failure::cancelled());
        }
        let depot = DepotId(p.spec.depot_id);
        let manifest_id = ManifestId(p.spec.manifest_id);
        let mut config = DepotConfig::load(&req.install_dir);
        config.set_installing(depot, manifest_id, &p.key);
        let _ = config.save(&req.install_dir);

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<LibEvent>();
        // A fresh client and server pool per depot (neither can be cloned).
        let job_cdn = CdnClient::new().map_err(|e| Failure::from_error(&e, signed_in))?;
        let fetcher = CdnChunkFetcher::new(
            job_cdn,
            CdnServerPool::new(cdn_servers.clone()),
            cdn_auth_token,
        );
        let mut builder = DepotJob::builder()
            .depot_id(depot)
            .depot_key(p.key.clone())
            .install_dir(req.install_dir.clone())
            // Skip anything already on disk and correct: this is what makes a resume cheap.
            .verify(true)
            .event_sender(tx);
        if let Some(n) = req.max_downloads {
            builder = builder.max_downloads(n);
        }
        if let Some(patterns) = &req.only_files {
            let lines: Vec<String> = patterns.iter().map(|p| format!("regex:{p}")).collect();
            let filter = FileFilter::from_filelist(&lines)
                .map_err(|e| Failure::new(FailureKind::Other, "Bad file filter.", e.to_string()))?;
            builder = builder.file_filter(filter);
        }
        let job = builder.build().map_err(|e| {
            Failure::new(
                FailureKind::Other,
                "Could not set up the download.",
                e.to_string(),
            )
        })?;

        let emit_progress = emit.clone();
        let total_all = total_bytes;
        let base = base_done;
        let forwarder = tokio::spawn(async move {
            // The library reports every chunk; the window only needs about ten updates a second.
            let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
            while let Some(ev) = rx.recv().await {
                match ev {
                    LibEvent::DepotProgress {
                        completed_bytes, ..
                    } => {
                        let done = (base + completed_bytes).min(total_all);
                        if done >= total_all
                            || last.elapsed() >= std::time::Duration::from_millis(100)
                        {
                            last = std::time::Instant::now();
                            emit_progress(DownloadEvent::Progress {
                                done,
                                total: total_all,
                            });
                        }
                    }
                    LibEvent::ChunkFailed { error } => {
                        tracing::debug!(error = %error, "a chunk failed and will be retried");
                    }
                    _ => {}
                }
            }
        });

        let stats = job
            .download(&p.manifest, Arc::new(fetcher))
            .await
            .map_err(|e| Failure::from_text(&format!("{e}\n{e:?}"), signed_in))?;
        drop(job); // closes the event channel so the forwarder ends
        let _ = forwarder.await;

        let _ = DepotConfig::save_manifest_decompressed(
            &req.install_dir,
            depot,
            manifest_id,
            &p.manifest_bytes,
        );
        config.set_installed(depot, manifest_id, &p.key);
        let _ = config.save(&req.install_dir);

        summary.bytes_downloaded += stats.bytes_downloaded;
        summary.files_completed += stats.files_completed;
        summary.files_skipped += stats.files_skipped;
        base_done += p.total;
        emit(DownloadEvent::Progress {
            done: base_done,
            total: total_bytes,
        });
    }
    Ok(summary)
}
