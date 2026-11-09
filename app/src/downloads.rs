//! Game downloads: a queue with one running download at a time, progress in the window,
//! and resuming downloads that did not finish.
//!
//! A build is two Steam depots (the game files and the content bundles) downloaded straight into
//! `<install folder>\RustyLauncherVersions\<client manifest>`. While it runs the folder carries an
//! install marker, so the unfinished build never shows up in the library and can be resumed.

use crate::ctx::{self, Ctx, now_unix};
use crate::{DownloadRow, UnfinishedRow, format, steam, tasks};
use rl_core::install::{self, InstallMarker};
use rl_core::library::format_bytes;
use rl_core::model::{CLIENT_APP, CLIENT_BUNDLES_DEPOT, CLIENT_DEPOT};
use rl_core::paths::VERSIONS_DIR_NAME;
use rl_core::scan;
use rl_steam::{DepotSpec, DownloadEvent, DownloadHandle, DownloadRequest, FailureKind};
use slint::{ModelRc, VecModel};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

pub struct Job {
    pub id: u64,
    pub name: String,
    pub client: String,
    pub bundles: String,
    /// The install folder the user chose (the build goes in a subfolder of it).
    pub base: PathBuf,
    /// The folder this build is downloaded into.
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

// ---------------------------------------------------------------- starting

/// A confirmed "Download" dialog. `arg` is `client|bundles|name`.
pub fn start_from_dialog(arg: &str) {
    let mut parts = arg.splitn(3, '|');
    let client = parts.next().unwrap_or("").trim().to_string();
    let bundles = parts.next().unwrap_or("").trim().to_string();
    let name = parts.next().unwrap_or("").trim().to_string();
    start(&name, &client, &bundles);
}

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
    {
        let m = ctx.downloads.borrow();
        let same = |j: &Job| j.client == client;
        if m.active.as_ref().is_some_and(|a| same(&a.job)) || m.queue.iter().any(same) {
            ctx.toast(
                "That build is already downloading or waiting in line.",
                "info",
            );
            return;
        }
    }
    if steam::account(&ctx).is_none() {
        ctx.toast(
            "Sign in to Steam first, then start the download again.",
            "info",
        );
        ctx.with_app(|a| a.invoke_open_signin());
        return;
    }
    let name = if name.is_empty() {
        format!("Rust build {}", client.chars().take(8).collect::<String>())
    } else {
        name.to_string()
    };
    let id = {
        let mut m = ctx.downloads.borrow_mut();
        m.next_id += 1;
        m.next_id
    };
    enqueue(
        &ctx,
        Job {
            id,
            name,
            client: client.to_string(),
            bundles: bundles.to_string(),
            base,
            dir,
        },
    );
}

pub fn resume(path: &str) {
    let ctx = ctx::get();
    let dir = PathBuf::from(path);
    let Some(marker) = install::read_marker(&dir) else {
        ctx.toast("That download can no longer be resumed.", "error");
        refresh_ui(&ctx);
        return;
    };
    if steam::account(&ctx).is_none() {
        ctx.toast("Sign in to Steam first, then resume the download.", "info");
        ctx.with_app(|a| a.invoke_open_signin());
        return;
    }
    // dir = <base>\RustyLauncherVersions\<manifest>
    let versions = dir.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = if versions.file_name().is_some_and(|n| n == VERSIONS_DIR_NAME) {
        versions.parent().map(Path::to_path_buf).unwrap_or_default()
    } else {
        versions
    };
    let id = {
        let mut m = ctx.downloads.borrow_mut();
        m.next_id += 1;
        m.next_id
    };
    enqueue(
        &ctx,
        Job {
            id,
            name: marker.name,
            client: marker.client_manifest,
            bundles: marker.bundles_manifest,
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
        .set_title("Choose where downloaded builds are saved")
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
    let Some(auth) = steam::auth(ctx) else {
        ctx.toast(
            "Your Steam sign-in is missing. Please sign in again.",
            "error",
        );
        ctx.with_app(|a| a.invoke_open_signin());
        refresh_ui(ctx);
        return;
    };
    let marker = InstallMarker {
        name: job.name.clone(),
        client_manifest: job.client.clone(),
        bundles_manifest: job.bundles.clone(),
    };
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

    let request = DownloadRequest {
        app_id: CLIENT_APP,
        depots: vec![
            DepotSpec {
                depot_id: CLIENT_DEPOT,
                manifest_id: job.client.parse().unwrap_or(0),
            },
            DepotSpec {
                depot_id: CLIENT_BUNDLES_DEPOT,
                manifest_id: job.bundles.parse().unwrap_or(0),
            },
        ],
        install_dir: job.dir.clone(),
        manifest_cache_dir: ctx.paths.cache_dir().join("manifests"),
        branch: "public".to_string(),
        only_files: None,
        max_downloads: None,
    };
    let id = job.id;
    tracing::info!(name = %job.name, client = %job.client, bundles = %job.bundles, "starting a download");
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
    });
    refresh_ui(ctx);
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
    let marker = InstallMarker {
        name: job.name.clone(),
        client_manifest: job.client.clone(),
        bundles_manifest: job.bundles.clone(),
    };
    tracing::info!(name = %job.name, files_fetched, "download finished");
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
    refresh_ui(ctx);
    pump(ctx);
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
    if a.speed > 0.0 {
        parts.push(format::speed(a.speed));
        let eta = format::eta(a.total.saturating_sub(a.done), a.speed);
        if !eta.is_empty() {
            parts.push(eta);
        }
    }
    parts.join("  ·  ")
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
            format!("Downloading {}:  {status}", a.job.name),
            progress,
        );
        rows.push(DownloadRow {
            id: a.job.id as i32,
            name: a.job.name.as_str().into(),
            status: status.into(),
            progress,
            active: true,
        });
    }
    for j in &m.queue {
        rows.push(DownloadRow {
            id: j.id as i32,
            name: j.name.as_str().into(),
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
            name: marker.name.as_str().into(),
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
