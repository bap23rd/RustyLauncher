//! Downloads: a queue with one running download at a time, progress in the window, and resuming
//! downloads that did not finish. Game builds and dedicated servers go through the same queue.
//!
//! A game build is two Steam depots (game files and content bundles) saved straight into
//! `<install folder>\RustyLauncherVersions\<client manifest>`. A server is two depots as well, saved into
//! `<install folder>\RustServers\<win64 manifest>`. While a download runs its folder carries an install
//! marker, so an unfinished one never shows up in a library and can be resumed.

use crate::ctx::{self, Ctx, now_unix};
use crate::{DownloadRow, UnfinishedRow, format, steam, tasks};
use rl_core::install::{self, InstallMarker};
use rl_core::library::format_bytes;
use rl_core::model::{
    CLIENT_APP, CLIENT_BUNDLES_DEPOT, CLIENT_DEPOT, SERVER_APP, SERVER_COMMON_DEPOT,
    SERVER_WIN64_DEPOT, ServerInstallType,
};
use rl_core::paths::{SERVERS_DIR_NAME, VERSIONS_DIR_NAME};
use rl_core::scan;
use rl_server::{InstallInfo, ServerFile, ServerLaunchConfig};
use rl_steam::{Auth, DepotSpec, DownloadEvent, DownloadHandle, DownloadRequest, FailureKind};
use slint::{ModelRc, VecModel};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

/// What is being downloaded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A game client build (needs a Steam account that owns Rust).
    Client,
    /// A server build at chosen manifests (older builds need a Steam account).
    ServerPinned,
    /// The newest server build (anonymous).
    ServerLatest,
    /// Bring an installed "latest" server up to the newest build, in place (anonymous).
    ServerUpdate,
}

impl Kind {
    fn marker(self) -> &'static str {
        match self {
            Kind::Client => "",
            Kind::ServerPinned => "server-pinned",
            Kind::ServerLatest => "server-latest",
            Kind::ServerUpdate => "server-update",
        }
    }
    fn from_marker(kind: &str) -> Kind {
        match kind {
            "server-pinned" => Kind::ServerPinned,
            "server-latest" => Kind::ServerLatest,
            "server-update" => Kind::ServerUpdate,
            _ => Kind::Client,
        }
    }
    fn is_server(self) -> bool {
        self != Kind::Client
    }
    /// Steam only gives old builds, and the game itself, to an account that owns the game.
    fn needs_account(self) -> bool {
        matches!(self, Kind::Client | Kind::ServerPinned)
    }
    fn label(self) -> &'static str {
        if self.is_server() {
            "Server"
        } else {
            "Game build"
        }
    }
    fn app_and_depots(self) -> (u32, u32, u32) {
        if self.is_server() {
            (SERVER_APP, SERVER_WIN64_DEPOT, SERVER_COMMON_DEPOT)
        } else {
            (CLIENT_APP, CLIENT_DEPOT, CLIENT_BUNDLES_DEPOT)
        }
    }
}

pub struct Job {
    pub id: u64,
    pub kind: Kind,
    pub name: String,
    /// The first depot's manifest (game files, or the win64 server depot).
    pub primary: String,
    /// The second depot's manifest (content bundles, or the common server depot).
    pub secondary: String,
    /// The install folder the user chose (the download goes in a subfolder of it).
    pub base: PathBuf,
    /// The folder this download goes into.
    pub dir: PathBuf,
}

struct Active {
    job: Job,
    handle: DownloadHandle,
    total: u64,
    done: u64,
    /// What is happening before the first byte moves ("Signing in...").
    phase: String,
    speed: f64,
    sample: (Instant, u64),
    started: Instant,
}

#[derive(Default)]
pub struct Manager {
    queue: VecDeque<Job>,
    active: Option<Active>,
    next_id: u64,
}

/// Is a download running or waiting?
pub fn is_busy() -> bool {
    let ctx = ctx::get();
    let m = ctx.downloads.borrow();
    m.active.is_some() || !m.queue.is_empty()
}

fn next_id(ctx: &Ctx) -> u64 {
    let mut m = ctx.downloads.borrow_mut();
    m.next_id += 1;
    m.next_id
}

/// Is a job for this folder already running or waiting?
fn already_queued(ctx: &Ctx, dir: &Path) -> bool {
    let m = ctx.downloads.borrow();
    m.active.as_ref().is_some_and(|a| a.job.dir == dir) || m.queue.iter().any(|j| j.dir == dir)
}

/// The signed-in account's token, or `None` (after prompting to sign in) if the job needs one.
fn auth_for(ctx: &Ctx, kind: Kind) -> Option<Auth> {
    if !kind.needs_account() {
        return Some(Auth::Anonymous);
    }
    let auth = steam::auth(ctx);
    if auth.is_none() {
        ctx.toast("Sign in to Steam first, then try again.", "info");
        ctx.with_app(|a| a.invoke_open_signin());
    }
    auth
}

// ---------------------------------------------------------------- starting

/// A confirmed "Download" dialog. `arg` is `client|bundles|name`.
pub fn start_from_dialog(arg: &str) {
    let mut parts = arg.splitn(3, '|');
    let client = parts.next().unwrap_or("").trim().to_string();
    let bundles = parts.next().unwrap_or("").trim().to_string();
    let name = parts.next().unwrap_or("").trim().to_string();
    start(&name, &client, &bundles);
}

/// Queue a game build.
pub fn start(name: &str, client: &str, bundles: &str) {
    let ctx = ctx::get();
    if !install::valid_manifest_id(client) || !install::valid_manifest_id(bundles) {
        ctx.toast(
            "Manifest IDs are long numbers, digits only. Copy them from SteamDB.",
            "error",
        );
        return;
    }
    let Some(base) = current_dir(&ctx) else {
        ctx.toast(
            "Choose a download folder first (the Change button).",
            "info",
        );
        return;
    };
    let dir = install::install_dir(&base, client);
    if dir.join("RustClient.exe").is_file() && !install::has_marker(&dir) {
        ctx.toast(
            "That build is already installed. It is in your Library.",
            "info",
        );
        return;
    }
    if already_queued(&ctx, &dir) {
        ctx.toast(
            "That build is already downloading or waiting in line.",
            "info",
        );
        return;
    }
    if auth_for(&ctx, Kind::Client).is_none() {
        return;
    }
    let name = if name.is_empty() {
        format!("Rust build {}", client.chars().take(8).collect::<String>())
    } else {
        name.to_string()
    };
    let id = next_id(&ctx);
    enqueue(
        &ctx,
        Job {
            id,
            kind: Kind::Client,
            name,
            primary: client.to_string(),
            secondary: bundles.to_string(),
            base,
            dir,
        },
    );
}

/// Queue a server at chosen manifests. Older server builds need a Steam account.
pub fn start_server_pinned(name: &str, win64: &str, common: &str) {
    let ctx = ctx::get();
    if !install::valid_manifest_id(win64) || !install::valid_manifest_id(common) {
        ctx.toast(
            "Manifest IDs are long numbers, digits only. Copy them from SteamDB (depots 258551 and 258554).",
            "error",
        );
        return;
    }
    let Some(base) = current_dir(&ctx) else {
        ctx.toast(
            "Choose a folder for servers first (the Change button).",
            "info",
        );
        return;
    };
    let dir = install::server_install_dir(&base, win64);
    if rl_server::looks_like_server(&dir) && !install::has_marker(&dir) {
        ctx.toast("That server build is already installed.", "info");
        return;
    }
    if already_queued(&ctx, &dir) {
        ctx.toast(
            "That server is already downloading or waiting in line.",
            "info",
        );
        return;
    }
    if auth_for(&ctx, Kind::ServerPinned).is_none() {
        return;
    }
    let name = if name.is_empty() {
        format!("Server {}", win64.chars().take(8).collect::<String>())
    } else {
        name.to_string()
    };
    let id = next_id(&ctx);
    enqueue(
        &ctx,
        Job {
            id,
            kind: Kind::ServerPinned,
            name,
            primary: win64.to_string(),
            secondary: common.to_string(),
            base,
            dir,
        },
    );
}

/// Install the newest server build. No Steam account is needed.
pub fn start_server_latest(name: &str) {
    let ctx = ctx::get();
    let Some(base) = current_dir(&ctx) else {
        ctx.toast(
            "Choose a folder for servers first (the Change button).",
            "info",
        );
        return;
    };
    resolve_latest(&ctx, name.to_string(), base, None);
}

/// Bring an installed "latest" server up to the newest build, keeping its settings and saves.
pub fn update_server(id: &str) {
    let ctx = ctx::get();
    tracing::info!(id, "server update requested");
    let found = ctx
        .state
        .borrow()
        .server_by_id(id)
        .map(|s| (s.name.clone(), s.install_path.clone(), s.install_type));
    let Some((name, dir, install_type)) = found else {
        ctx.toast("That server is no longer in the library.", "error");
        return;
    };
    if install_type != ServerInstallType::Latest {
        ctx.toast(
            "Only servers installed as \"latest\" can be updated.",
            "info",
        );
        return;
    }
    // dir = <base>\RustServers\<manifest>
    let base = dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    resolve_latest(&ctx, name, base, Some(dir));
}

/// Ask Steam for the newest server manifests, then queue the job.
fn resolve_latest(ctx: &Ctx, name: String, base: PathBuf, update_dir: Option<PathBuf>) {
    ctx.toast("Looking up the newest server build...", "info");
    ctx.engine.latest_manifests(
        Auth::Anonymous,
        SERVER_APP,
        vec![SERVER_WIN64_DEPOT, SERVER_COMMON_DEPOT],
        "public".to_string(),
        move |result| {
            let _ = slint::invoke_from_event_loop(move || {
                latest_resolved(name, base, update_dir, result);
            });
        },
    );
}

fn latest_resolved(
    name: String,
    base: PathBuf,
    update_dir: Option<PathBuf>,
    result: Result<Vec<DepotSpec>, rl_steam::Failure>,
) {
    let ctx = ctx::get();
    tracing::info!(
        updating = update_dir.is_some(),
        ok = result.is_ok(),
        "newest server build looked up"
    );
    let specs = match result {
        Ok(s) if s.len() == 2 => s,
        Ok(_) => {
            ctx.toast("Steam returned an unexpected answer. Try again.", "error");
            return;
        }
        Err(f) => {
            ctx.toast(&f.message, "error");
            return;
        }
    };
    let (win64, common) = (
        specs[0].manifest_id.to_string(),
        specs[1].manifest_id.to_string(),
    );
    let id = next_id(&ctx);
    match update_dir {
        Some(dir) => {
            let current = rl_server::read_server_file(&dir)
                .ok()
                .flatten()
                .and_then(|f| f.install)
                .map(|i| (i.manifest_id, i.common_manifest_id));
            if current == Some((Some(win64.clone()), Some(common.clone()))) {
                ctx.toast("This server is already on the newest build.", "success");
                return;
            }
            if already_queued(&ctx, &dir) {
                ctx.toast(
                    "That server is already updating or waiting in line.",
                    "info",
                );
                return;
            }
            enqueue(
                &ctx,
                Job {
                    id,
                    kind: Kind::ServerUpdate,
                    name,
                    primary: win64,
                    secondary: common,
                    base,
                    dir,
                },
            );
        }
        None => {
            let dir = install::server_install_dir(&base, &win64);
            if rl_server::looks_like_server(&dir) && !install::has_marker(&dir) {
                ctx.toast(
                    "You already have the newest server build installed.",
                    "info",
                );
                return;
            }
            if already_queued(&ctx, &dir) {
                ctx.toast(
                    "That server is already downloading or waiting in line.",
                    "info",
                );
                return;
            }
            let name = if name.is_empty() {
                format!("Latest server ({})", format::ymd(now_unix()))
            } else {
                name
            };
            enqueue(
                &ctx,
                Job {
                    id,
                    kind: Kind::ServerLatest,
                    name,
                    primary: win64,
                    secondary: common,
                    base,
                    dir,
                },
            );
        }
    }
}

pub fn resume(path: &str) {
    let ctx = ctx::get();
    let dir = PathBuf::from(path);
    let Some(marker) = install::read_marker(&dir) else {
        ctx.toast("That download can no longer be resumed.", "error");
        refresh_ui(&ctx);
        return;
    };
    let kind = Kind::from_marker(&marker.kind);
    if auth_for(&ctx, kind).is_none() {
        return;
    }
    // dir = <base>\RustyLauncherVersions\<manifest>  or  <base>\RustServers\<manifest>
    let parent = dir.parent().map(Path::to_path_buf).unwrap_or_default();
    let is_container = parent
        .file_name()
        .is_some_and(|n| n == VERSIONS_DIR_NAME || n == SERVERS_DIR_NAME);
    let base = if is_container {
        parent.parent().map(Path::to_path_buf).unwrap_or_default()
    } else {
        parent
    };
    let id = next_id(&ctx);
    enqueue(
        &ctx,
        Job {
            id,
            kind,
            name: marker.name,
            primary: marker.client_manifest,
            secondary: marker.bundles_manifest,
            base,
            dir,
        },
    );
}

/// Delete an unfinished download. Only folders that really carry the install marker qualify.
pub fn discard(path: &str) {
    let ctx = ctx::get();
    let dir = PathBuf::from(path);
    if install::read_marker(&dir).is_none() || dir.components().count() < 3 {
        ctx.toast(
            "That is not an unfinished download, so nothing was deleted.",
            "error",
        );
        return;
    }
    tasks::run(
        &ctx,
        "Deleting the unfinished download".to_string(),
        false,
        move |_rep, _cancel| std::fs::remove_dir_all(&dir).map_err(|e| e.to_string()),
        |ctx, result| {
            match result {
                Ok(()) => ctx.toast("Unfinished download deleted.", "success"),
                Err(e) => ctx.toast(&format!("Could not delete it: {e}"), "error"),
            }
            refresh_ui(ctx);
        },
    );
}

pub fn change_dir() {
    let ctx = ctx::get();
    let picked = rfd::FileDialog::new()
        .set_title("Choose where downloads are saved")
        .pick_folder();
    let Some(path) = picked else { return };
    ctx.state
        .borrow_mut()
        .touch_install_path(path.clone(), now_unix());
    ctx.save();
    *ctx.download_dir.borrow_mut() = Some(path);
    ctx.refresh();
    refresh_ui(&ctx);
    crate::actions::refresh_usage(&ctx);
}

/// The folder new downloads go to: the one chosen on the Download page, else the most recently used.
pub fn current_dir(ctx: &Ctx) -> Option<PathBuf> {
    ctx.download_dir.borrow().clone().or_else(|| {
        ctx.state
            .borrow()
            .install_paths
            .first()
            .map(|p| p.path.clone())
    })
}

// ---------------------------------------------------------------- running

fn enqueue(ctx: &Ctx, job: Job) {
    ctx.downloads.borrow_mut().queue.push_back(job);
    refresh_ui(ctx);
    pump(ctx);
}

fn pump(ctx: &Ctx) {
    if ctx.downloads.borrow().active.is_some() {
        return;
    }
    let next = ctx.downloads.borrow_mut().queue.pop_front();
    if let Some(job) = next {
        begin(ctx, job);
    }
}

fn begin(ctx: &Ctx, job: Job) {
    let Some(auth) = auth_for(ctx, job.kind) else {
        refresh_ui(ctx);
        return;
    };
    let marker = marker_for(&job);
    if let Err(err) = std::fs::create_dir_all(&job.dir)
        .map_err(|e| e.to_string())
        .and_then(|()| install::write_marker(&job.dir, &marker).map_err(|e| e.to_string()))
    {
        tracing::error!(%err, "could not prepare the download folder");
        ctx.toast(&format!("Could not use that folder: {err}"), "error");
        pump(ctx);
        return;
    }
    ctx.state
        .borrow_mut()
        .touch_install_path(job.base.clone(), now_unix());
    ctx.save();

    let (app_id, first, second) = job.kind.app_and_depots();
    let request = DownloadRequest {
        app_id,
        depots: vec![
            DepotSpec {
                depot_id: first,
                manifest_id: job.primary.parse().unwrap_or(0),
            },
            DepotSpec {
                depot_id: second,
                manifest_id: job.secondary.parse().unwrap_or(0),
            },
        ],
        install_dir: job.dir.clone(),
        manifest_cache_dir: ctx.paths.cache_dir().join("manifests"),
        branch: "public".to_string(),
        only_files: None,
        max_downloads: None,
    };
    let id = job.id;
    tracing::info!(name = %job.name, kind = ?job.kind, primary = %job.primary, secondary = %job.secondary, "starting a download");
    let handle = ctx.engine.download(request, auth, move |event| {
        let _ = slint::invoke_from_event_loop(move || on_event(id, event));
    });
    ctx.downloads.borrow_mut().active = Some(Active {
        job,
        handle,
        total: 0,
        done: 0,
        phase: "Starting...".to_string(),
        speed: 0.0,
        sample: (Instant::now(), 0),
        started: Instant::now(),
    });
    refresh_ui(ctx);
}

fn marker_for(job: &Job) -> InstallMarker {
    InstallMarker {
        name: job.name.clone(),
        client_manifest: job.primary.clone(),
        bundles_manifest: job.secondary.clone(),
        kind: job.kind.marker().to_string(),
    }
}

fn on_event(id: u64, event: DownloadEvent) {
    let ctx = ctx::get();
    {
        let mut m = ctx.downloads.borrow_mut();
        let Some(a) = m.active.as_mut() else { return };
        if a.job.id != id {
            return;
        }
        match &event {
            DownloadEvent::Preparing(text) => a.phase = text.clone(),
            DownloadEvent::Total { bytes, .. } => {
                a.total = *bytes;
                a.phase.clear();
            }
            DownloadEvent::Progress { done, total } => {
                a.done = *done;
                a.total = *total;
                let now = Instant::now();
                let dt = now.duration_since(a.sample.0).as_secs_f64();
                if dt >= 1.0 {
                    let instant = done.saturating_sub(a.sample.1) as f64 / dt;
                    a.speed = if a.speed == 0.0 {
                        instant
                    } else {
                        0.7 * a.speed + 0.3 * instant
                    };
                    a.sample = (now, *done);
                }
            }
            _ => {}
        }
    }
    match event {
        DownloadEvent::Finished(summary) => finished(&ctx, summary.files_completed),
        DownloadEvent::Failed(failure) => failed(&ctx, failure),
        DownloadEvent::Cancelled => cancelled(&ctx),
        _ => refresh_ui(&ctx),
    }
}

fn take_active(ctx: &Ctx) -> Option<Job> {
    ctx.downloads.borrow_mut().active.take().map(|a| a.job)
}

fn finished(ctx: &Ctx, files_fetched: u64) {
    let Some(job) = take_active(ctx) else { return };
    tracing::info!(name = %job.name, kind = ?job.kind, files_fetched, "download finished");
    if job.kind.is_server() {
        finish_server(ctx, &job);
    } else {
        finish_client(ctx, &job);
    }
    refresh_ui(ctx);
    pump(ctx);
}

fn finish_client(ctx: &Ctx, job: &Job) {
    let marker = marker_for(job);
    match install::finish_install(&job.dir, &marker, &format::iso_utc(now_unix())) {
        Ok(()) => match scan::version_from_dir(&job.dir) {
            Some(version) => {
                ctx.state.borrow_mut().add_version(version);
                ctx.save();
                ctx.refresh();
                ctx.toast(
                    &format!("Downloaded {}. It is in your Library.", job.name),
                    "success",
                );
            }
            None => ctx.toast(
                "The download finished, but the game files were not found where expected.",
                "error",
            ),
        },
        Err(err) => {
            tracing::error!(%err, "could not finish the install");
            ctx.toast(
                &format!("The download finished, but saving its details failed: {err}"),
                "error",
            );
        }
    }
}

fn finish_server(ctx: &Ctx, job: &Job) {
    // A new server gets fresh settings (with a random RCON password). An update keeps the existing ones.
    let previous = rl_server::read_server_file(&job.dir).ok().flatten();
    let install_type = match job.kind {
        Kind::ServerPinned => ServerInstallType::Pinned,
        _ => ServerInstallType::Latest,
    };
    let created_at = previous
        .as_ref()
        .and_then(|f| f.install.as_ref())
        .and_then(|i| i.created_at.clone())
        .unwrap_or_else(|| format::iso_utc(now_unix()));
    let file = ServerFile {
        config: previous
            .map(|f| f.config)
            .unwrap_or_else(|| ServerLaunchConfig::for_new_server(&job.name)),
        install: Some(InstallInfo {
            name: job.name.clone(),
            install_type,
            manifest_id: Some(job.primary.clone()),
            common_manifest_id: Some(job.secondary.clone()),
            created_at: Some(created_at),
        }),
    };
    let result = rl_server::write_server_file(&job.dir, &file)
        .map_err(|e| e.to_string())
        .and_then(|()| install::remove_marker(&job.dir).map_err(|e| e.to_string()));
    match result {
        Ok(()) => match rl_server::server_from_dir(&job.dir) {
            Some(server) => {
                ctx.state.borrow_mut().upsert_server(server);
                ctx.save();
                ctx.refresh();
                let verb = if job.kind == Kind::ServerUpdate {
                    "updated"
                } else {
                    "installed"
                };
                ctx.toast(&format!("Server {} {verb}.", job.name), "success");
            }
            None => ctx.toast(
                "The download finished, but the server files were not found where expected.",
                "error",
            ),
        },
        Err(err) => {
            tracing::error!(%err, "could not finish the server install");
            ctx.toast(
                &format!("The download finished, but saving the server's details failed: {err}"),
                "error",
            );
        }
    }
}

fn failed(ctx: &Ctx, failure: rl_steam::Failure) {
    let Some(job) = take_active(ctx) else { return };
    let waiting = {
        let mut m = ctx.downloads.borrow_mut();
        let n = m.queue.len();
        m.queue.clear();
        n
    };
    tracing::warn!(name = %job.name, kind = ?failure.kind, detail = %failure.detail, "download failed");
    let mut text = failure.message.clone();
    if waiting > 0 {
        text.push_str(&format!(
            " The {waiting} download(s) waiting in line were cancelled."
        ));
    }
    text.push_str(" Your progress is kept; resume from the Download page.");
    ctx.toast(&text, "error");
    if failure.kind == FailureKind::NeedsSignIn {
        ctx.with_app(|a| a.invoke_open_signin());
    }
    refresh_ui(ctx);
}

fn cancelled(ctx: &Ctx) {
    let Some(job) = take_active(ctx) else { return };
    tracing::info!(name = %job.name, "download cancelled");
    ctx.toast(
        "Download cancelled. You can resume it from the Download page.",
        "info",
    );
    refresh_ui(ctx);
    pump(ctx);
}

/// Cancel the running download (`id` < 0 means "the running one") or remove a waiting one.
pub fn cancel(id: i32) {
    let ctx = ctx::get();
    {
        let m = ctx.downloads.borrow();
        if let Some(a) = m.active.as_ref() {
            if id < 0 || a.job.id as i64 == id as i64 {
                a.handle.cancel();
                return;
            }
        }
    }
    ctx.downloads
        .borrow_mut()
        .queue
        .retain(|j| j.id as i64 != id as i64);
    refresh_ui(&ctx);
}

// ---------------------------------------------------------------- showing it

fn status_text(a: &Active) -> String {
    if a.total == 0 {
        return if a.phase.is_empty() {
            "Starting...".to_string()
        } else {
            a.phase.clone()
        };
    }
    let pct = (a.done as f64 / a.total as f64 * 100.0).round() as i32;
    let mut parts = vec![
        format!("{pct}%"),
        format!("{} of {}", format_bytes(a.done), format_bytes(a.total)),
    ];
    // The first seconds give a wildly wrong speed (and "15 hours left"), so wait a little.
    if a.speed > 0.0 && a.started.elapsed() >= std::time::Duration::from_secs(8) {
        parts.push(format::speed(a.speed));
        let eta = format::eta(a.total.saturating_sub(a.done), a.speed);
        if !eta.is_empty() {
            parts.push(eta);
        }
    }
    parts.join("  ·  ")
}

fn display_name(job: &Job) -> String {
    if job.kind.is_server() {
        format!("Server: {}", job.name)
    } else {
        job.name.clone()
    }
}

pub fn refresh_ui(ctx: &Ctx) {
    let m = ctx.downloads.borrow();
    let mut rows: Vec<DownloadRow> = Vec::new();
    let mut banner = (false, String::new(), -1.0f32);
    if let Some(a) = m.active.as_ref() {
        let progress = if a.total == 0 {
            -1.0
        } else {
            (a.done as f64 / a.total as f64) as f32
        };
        let status = status_text(a);
        banner = (
            true,
            format!("Downloading {}:  {status}", display_name(&a.job)),
            progress,
        );
        rows.push(DownloadRow {
            id: a.job.id as i32,
            name: display_name(&a.job).into(),
            status: status.into(),
            progress,
            active: true,
        });
    }
    for j in &m.queue {
        rows.push(DownloadRow {
            id: j.id as i32,
            name: display_name(j).into(),
            status: "Waiting in line".into(),
            progress: 0.0,
            active: false,
        });
    }

    // Unfinished downloads that are not running or waiting right now.
    let busy_dirs: Vec<PathBuf> = m
        .active
        .iter()
        .map(|a| a.job.dir.clone())
        .chain(m.queue.iter().map(|j| j.dir.clone()))
        .collect();
    let bases: Vec<PathBuf> = ctx
        .state
        .borrow()
        .install_paths
        .iter()
        .map(|p| p.path.clone())
        .collect();
    let unfinished: Vec<UnfinishedRow> = install::find_unfinished(&bases)
        .into_iter()
        .filter(|(dir, _)| !busy_dirs.contains(dir))
        .map(|(dir, marker)| UnfinishedRow {
            path: dir.to_string_lossy().as_ref().into(),
            name: format!(
                "{}: {}",
                Kind::from_marker(&marker.kind).label(),
                marker.name
            )
            .into(),
            detail: format!(
                "Manifests {} and {}",
                marker.client_manifest, marker.bundles_manifest
            )
            .into(),
        })
        .collect();
    let dir_text = current_dir(ctx)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    ctx.with_app(|app| {
        app.set_downloads(ModelRc::from(Rc::new(VecModel::from(rows))));
        app.set_unfinished(ModelRc::from(Rc::new(VecModel::from(unfinished))));
        app.set_download_dir(dir_text.into());
        app.set_download_active(banner.0);
        app.set_download_text(banner.1.into());
        app.set_download_progress(banner.2);
    });
}
