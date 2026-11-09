//! What the buttons do. Each handler runs on the UI thread; slow work goes to a worker thread
//! (see `tasks`) and the result comes back to a `done` closure.

use crate::ctx::{self, Ctx, now_unix};
use crate::tasks;
use rl_core::archive::{self, ArchiveError};
use rl_core::curated::{self, CuratedKind};
use rl_core::launch::{self, LaunchError};
use rl_core::library::{self, format_bytes};
use rl_core::model::Version;
use rl_core::paths::{ARCHIVE_DIR_NAME, versions_dir};
use rl_core::scan;
use rl_core::sidecar::{VersionSidecar, write_version_sidecar};
use slint::ComponentHandle;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ---------------------------------------------------------------- launching

pub fn launch_version(id: &str) {
    let ctx = ctx::get();
    do_launch(&ctx, id);
}

pub fn launch_selected() {
    let ctx = ctx::get();
    match ctx.selected_id() {
        Some(id) => do_launch(&ctx, &id),
        None => ctx.toast("There is no version to launch yet.", "info"),
    }
}

fn do_launch(ctx: &Ctx, id: &str) {
    let found = {
        let s = ctx.state.borrow();
        s.version_by_id(id)
            .map(|v| (v.install_path.clone(), v.name.clone()))
    };
    let Some((dir, name)) = found else {
        ctx.toast("That version is no longer in the library.", "error");
        return;
    };
    let eac = ctx
        .app
        .upgrade()
        .map(|a| a.get_launch_with_eac())
        .unwrap_or(true);
    match launch::launch(&dir, eac) {
        Ok(()) => {
            tracing::info!(version = %name, eac, "launched the game");
            ctx.state.borrow_mut().mark_played(id, eac, now_unix());
            ctx.save();
            ctx.refresh();
            ctx.toast(&format!("Launching {name}..."), "success");
        }
        Err(err) => {
            tracing::warn!(?err, "could not launch");
            let hint =
                if eac && matches!(err, LaunchError::MissingExe { .. }) && launch::has_client(&dir)
                {
                    " Try turning Easy Anti-Cheat off."
                } else {
                    ""
                };
            ctx.toast(&format!("{err}{hint}"), "error");
        }
    }
}

pub fn pick_version(id: &str) {
    let ctx = ctx::get();
    ctx.state.borrow_mut().settings.last_played_version = Some(id.to_string());
    ctx.save();
    ctx.refresh();
}

pub fn eac_changed(on: bool) {
    let ctx = ctx::get();
    ctx.state.borrow_mut().settings.launch_with_eac = on;
    ctx.save();
}

pub fn browse(id: &str) {
    let ctx = ctx::get();
    let path = ctx
        .state
        .borrow()
        .version_by_id(id)
        .map(|v| v.install_path.clone());
    match path {
        Some(p) if p.is_dir() => open_in_explorer(&ctx, &p),
        _ => ctx.toast("That folder could not be found.", "error"),
    }
}

fn open_in_explorer(ctx: &Ctx, path: &Path) {
    if let Err(err) = std::process::Command::new("explorer").arg(path).spawn() {
        tracing::warn!(%err, "could not open Explorer");
        ctx.toast("Could not open the folder.", "error");
    }
}

// ---------------------------------------------------------------- dialogs

/// A dialog was confirmed: `kind` says which, `arg` is what it was about, `text` is any text typed.
pub fn dialog_confirmed(kind: &str, arg: &str, text: &str) {
    let ctx = ctx::get();
    match kind {
        "rename" => rename(&ctx, arg, text),
        "delete" => delete_versions(&ctx, vec![arg.to_string()]),
        "archive" => archive_versions(&ctx, vec![arg.to_string()]),
        "archive-all" => {
            let ids = ctx
                .state
                .borrow()
                .versions
                .iter()
                .map(|v| v.id.clone())
                .collect();
            archive_versions(&ctx, ids);
        }
        "delete-all" => {
            let ids = ctx
                .state
                .borrow()
                .versions
                .iter()
                .map(|v| v.id.clone())
                .collect();
            delete_versions(&ctx, ids);
        }
        "delete-archive" => delete_archive(&ctx, arg),
        "remove-path" => remove_install_path(&ctx, arg),
        "download" => crate::downloads::start_from_dialog(arg),
        "download-server" => {
            let mut p = arg.splitn(3, '|');
            let (win64, common, name) = (
                p.next().unwrap_or("").to_string(),
                p.next().unwrap_or("").to_string(),
                p.next().unwrap_or("").to_string(),
            );
            crate::downloads::start_server_pinned(&name, &win64, &common);
        }
        "rename-server" => crate::servers::rename(&ctx, arg, text),
        "delete-server" => crate::servers::delete(&ctx, arg),
        "quit" => ctx.with_app(|a| {
            let _ = a.hide();
        }),
        "discard-download" => crate::downloads::discard(arg),
        "test" => ctx.toast("Confirmed.", "success"),
        other => tracing::warn!(kind = other, "unknown dialog kind"),
    }
}

fn rename(ctx: &Ctx, id: &str, new_name: &str) {
    let result = library::rename_version(&mut ctx.state.borrow_mut(), id, new_name);
    match result {
        Ok(()) => {
            ctx.save();
            ctx.refresh();
            ctx.toast("Renamed.", "success");
        }
        Err(err) => ctx.toast(&err.to_string(), "error"),
    }
}

// ---------------------------------------------------------------- archive and delete

struct Job {
    id: String,
    name: String,
    dir: PathBuf,
}

fn jobs_for(ctx: &Ctx, ids: &[String]) -> Vec<Job> {
    let state = ctx.state.borrow();
    ids.iter()
        .filter_map(|id| state.version_by_id(id))
        .map(|v| Job {
            id: v.id.clone(),
            name: v.name.clone(),
            dir: v.install_path.clone(),
        })
        .collect()
}

/// Result of one item in a batch: `Ok(bytes)` or the message to show.
type ItemResult = (String, String, Result<u64, String>);

fn summarize(verb_past: &str, results: &[ItemResult], cancelled: bool) -> (String, &'static str) {
    let ok = results.iter().filter(|r| r.2.is_ok()).count();
    let first_err = results.iter().find_map(|r| r.2.as_ref().err());
    if results.len() == 1 {
        let (_, name, res) = &results[0];
        return match res {
            Ok(bytes) if *bytes > 0 => (
                format!("{verb_past} {name} ({}).", format_bytes(*bytes)),
                "success",
            ),
            Ok(_) => (format!("{verb_past} {name}."), "success"),
            Err(e) => (e.clone(), if cancelled { "info" } else { "error" }),
        };
    }
    match (first_err, cancelled) {
        (None, false) => (format!("{verb_past} {ok} versions."), "success"),
        (_, true) => (
            format!("Cancelled after {ok} of {} versions.", results.len()),
            "info",
        ),
        (Some(e), false) => (
            format!("{verb_past} {ok} of {} versions. {e}", results.len()),
            "error",
        ),
    }
}

fn archive_versions(ctx: &Ctx, ids: Vec<String>) {
    let jobs = jobs_for(ctx, &ids);
    if jobs.is_empty() {
        ctx.toast("There is nothing to archive.", "info");
        return;
    }
    let label = if jobs.len() == 1 {
        format!("Archiving {}", jobs[0].name)
    } else {
        "Archiving versions".to_string()
    };
    tasks::run(
        ctx,
        label,
        true,
        move |rep, cancel| {
            let n = jobs.len();
            let mut results: Vec<ItemResult> = Vec::new();
            let mut cancelled = false;
            for (i, job) in jobs.iter().enumerate() {
                let archives_dir = job
                    .dir
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(ARCHIVE_DIR_NAME);
                let text = format!("Archiving {}", job.name);
                let outcome = archive::archive_version(
                    &job.dir,
                    &archives_dir,
                    &job.name,
                    &mut |d, t| {
                        rep.step(i, n, if t == 0 { 1.0 } else { d as f32 / t as f32 }, &text)
                    },
                    cancel,
                );
                let result = match outcome {
                    Ok(zip) => {
                        let size = std::fs::metadata(&zip).map(|m| m.len()).unwrap_or(0);
                        // Only now that the archive is written and checked do we remove the original.
                        rep.note(&format!(
                            "Archive checked. Removing the original folder of {}...",
                            job.name
                        ));
                        match library::delete_version_files(&job.dir) {
                            Ok(()) => Ok(size),
                            Err(e) => Err(format!(
                                "{} was archived, but its folder could not be removed: {e}",
                                job.name
                            )),
                        }
                    }
                    Err(ArchiveError::Cancelled) => {
                        cancelled = true;
                        Err("Archiving cancelled. Your files were not changed.".to_string())
                    }
                    Err(e) => Err(format!("Could not archive {}: {e}", job.name)),
                };
                results.push((job.id.clone(), job.name.clone(), result));
                if cancelled {
                    break;
                }
            }
            (results, cancelled)
        },
        |ctx, (results, cancelled)| {
            {
                let mut state = ctx.state.borrow_mut();
                for (id, _, res) in &results {
                    if res.is_ok() {
                        state.remove_version(id);
                    }
                }
            }
            ctx.save();
            ctx.refresh();
            reload_archives(ctx);
            let (text, kind) = summarize("Archived", &results, cancelled);
            ctx.toast(&text, kind);
        },
    );
}

fn delete_versions(ctx: &Ctx, ids: Vec<String>) {
    let jobs = jobs_for(ctx, &ids);
    if jobs.is_empty() {
        ctx.toast("There is nothing to delete.", "info");
        return;
    }
    let label = if jobs.len() == 1 {
        format!("Deleting {}", jobs[0].name)
    } else {
        "Deleting versions".to_string()
    };
    tasks::run(
        ctx,
        label,
        false,
        move |rep, _cancel| {
            let n = jobs.len();
            let mut results: Vec<ItemResult> = Vec::new();
            for (i, job) in jobs.iter().enumerate() {
                rep.note(&if n > 1 {
                    format!("Deleting {} ({} of {n})...", job.name, i + 1)
                } else {
                    format!("Deleting {}...", job.name)
                });
                let size = library::dir_size(&job.dir);
                let result = library::delete_version_files(&job.dir)
                    .map(|()| size)
                    .map_err(|e| format!("Could not delete {}: {e}", job.name));
                results.push((job.id.clone(), job.name.clone(), result));
            }
            results
        },
        |ctx, results| {
            {
                let mut state = ctx.state.borrow_mut();
                for (id, _, res) in &results {
                    if res.is_ok() {
                        state.remove_version(id);
                    }
                }
            }
            ctx.save();
            ctx.refresh();
            let (text, kind) = summarize("Deleted", &results, false);
            ctx.toast(&text, kind);
            refresh_usage(ctx);
        },
    );
}

pub fn restore_archive(zip_path: &str) {
    let ctx = ctx::get();
    let zip = PathBuf::from(zip_path);
    let Some(versions_dir) = zip.parent().and_then(|p| p.parent()).map(Path::to_path_buf) else {
        ctx.toast("That archive is not in a valid folder.", "error");
        return;
    };
    let stem = zip
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    tasks::run(
        &ctx,
        format!("Restoring {stem}"),
        true,
        move |rep, cancel| -> Result<Version, String> {
            let restored =
                archive::unarchive(&zip, &versions_dir, &mut |d, t| rep.bytes(d, t), cancel)
                    .map_err(|e| match e {
                        ArchiveError::Cancelled => {
                            "Restoring cancelled. The archive was kept.".to_string()
                        }
                        other => format!("Could not restore {stem}: {other}"),
                    })?;
            let Some(mut version) = scan::version_from_dir(&restored) else {
                let _ = std::fs::remove_dir_all(&restored);
                return Err(format!(
                    "{stem} does not contain a Rust client, so it was not restored."
                ));
            };
            // Archives made by V2 carry no sidecar: name the version after the archive and write one.
            if version.name.starts_with("Discovered:") {
                version.name = stem.clone();
                let _ = write_version_sidecar(
                    &restored,
                    &VersionSidecar {
                        name: stem.clone(),
                        manifest_id: version.manifest_id.clone(),
                        ..Default::default()
                    },
                );
            }
            // The version is back; the archive has done its job.
            let _ = std::fs::remove_file(&zip);
            Ok(version)
        },
        |ctx, result| {
            match result {
                Ok(version) => {
                    let name = version.name.clone();
                    ctx.state.borrow_mut().add_version(version);
                    ctx.save();
                    ctx.refresh();
                    ctx.toast(&format!("Restored {name}."), "success");
                }
                Err(msg) => ctx.toast(&msg, "error"),
            }
            reload_archives(ctx);
        },
    );
}

fn delete_archive(ctx: &Ctx, path: &str) {
    let path = PathBuf::from(path);
    let is_zip = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !is_zip {
        ctx.toast("That is not an archive file.", "error");
        return;
    }
    match std::fs::remove_file(&path) {
        Ok(()) => {
            ctx.toast("Archive deleted.", "success");
            reload_archives(ctx);
        }
        Err(e) => ctx.toast(&format!("Could not delete the archive: {e}"), "error"),
    }
}

/// Open a web page in the default browser. Only `https://steamdb.info/` links are used by the app.
pub fn open_url(url: &str) {
    if !(url.starts_with("https://steamdb.info/") || url.starts_with("https://umod.org/")) {
        tracing::warn!(url, "refusing to open an unexpected link");
        return;
    }
    if let Err(err) = std::process::Command::new("explorer").arg(url).spawn() {
        tracing::warn!(%err, "could not open the browser");
    }
}

pub fn cancel_task() {
    tasks::cancel(&ctx::get());
}

// ---------------------------------------------------------------- install paths, scanning

pub fn add_install_path() {
    let ctx = ctx::get();
    let picked = rfd::FileDialog::new()
        .set_title("Choose a folder for your Rust versions")
        .pick_folder();
    let Some(path) = picked else { return };
    ctx.state.borrow_mut().touch_install_path(path, now_unix());
    ctx.save();
    ctx.refresh();
    scan_in_background(false);
    refresh_usage(&ctx);
    reload_archives(&ctx);
}

fn remove_install_path(ctx: &Ctx, path: &str) {
    let base = versions_dir(Path::new(path));
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    let base_key = norm(&base);
    {
        let mut state = ctx.state.borrow_mut();
        state
            .install_paths
            .retain(|p| norm(&p.path) != norm(Path::new(path)));
        // Versions that lived in that folder leave the library (their files are not touched).
        let doomed: Vec<String> = state
            .versions
            .iter()
            .filter(|v| v.install_path.parent().is_some_and(|p| norm(p) == base_key))
            .map(|v| v.id.clone())
            .collect();
        for id in doomed {
            state.remove_version(&id);
        }
    }
    ctx.save();
    ctx.refresh();
    reload_archives(ctx);
    ctx.toast("Folder removed. Your files were not touched.", "success");
}

pub fn rescan() {
    scan_in_background(false);
}

/// Look in every install path for versions the library does not know about yet.
pub fn scan_in_background(quiet: bool) {
    let ctx = ctx::get();
    let dirs: Vec<PathBuf> = ctx
        .state
        .borrow()
        .install_paths
        .iter()
        .map(|p| p.path.clone())
        .collect();
    if dirs.is_empty() {
        if !quiet {
            ctx.toast("Add a folder in Settings first.", "info");
        }
        return;
    }
    std::thread::spawn(move || {
        let found: Vec<Version> = dirs
            .iter()
            .flat_map(|d| scan::scan_versions(d).unwrap_or_default())
            .collect();
        let servers: Vec<rl_core::model::Server> = dirs
            .iter()
            .flat_map(|d| rl_server::scan_servers(d))
            .collect();
        let _ = slint::invoke_from_event_loop(move || {
            let ctx = ctx::get();
            let added = scan::merge_versions(&mut ctx.state.borrow_mut(), found);
            // Servers already known (by folder) are left as they are.
            let added_servers = {
                let mut state = ctx.state.borrow_mut();
                let mut n = 0;
                for s in servers {
                    if !state
                        .servers
                        .iter()
                        .any(|x| x.install_path == s.install_path)
                    {
                        state.servers.push(s);
                        n += 1;
                    }
                }
                n
            };
            if added > 0 || added_servers > 0 {
                ctx.save();
                ctx.refresh();
            }
            if added > 0 || added_servers > 0 {
                let mut parts = Vec::new();
                if added > 0 {
                    parts.push(format!(
                        "{added} new version{}",
                        if added == 1 { "" } else { "s" }
                    ));
                }
                if added_servers > 0 {
                    parts.push(format!(
                        "{added_servers} new server{}",
                        if added_servers == 1 { "" } else { "s" }
                    ));
                }
                ctx.toast(&format!("Found {}.", parts.join(" and ")), "success");
            } else if !quiet {
                ctx.toast("Nothing new found.", "info");
            }
        });
    });
}

/// List the archive folders next to the known versions, off the UI thread.
pub fn reload_archives(ctx: &Ctx) {
    let dirs = ctx.versions_dirs();
    std::thread::spawn(move || {
        let mut all = Vec::new();
        for d in dirs {
            all.extend(archive::list_archives(&d.join(ARCHIVE_DIR_NAME)));
        }
        all.sort_by_key(|a| a.name.to_lowercase());
        let _ = slint::invoke_from_event_loop(move || {
            let ctx = ctx::get();
            *ctx.archives.borrow_mut() = all;
            ctx.refresh_archives();
        });
    });
}

/// Work out how much disk each install path uses, off the UI thread.
pub fn refresh_usage(ctx: &Ctx) {
    let paths: Vec<PathBuf> = ctx
        .state
        .borrow()
        .install_paths
        .iter()
        .map(|p| p.path.clone())
        .collect();
    ctx.usage.borrow_mut().clear();
    ctx.refresh();
    std::thread::spawn(move || {
        for p in paths {
            let size = library::dir_size(&versions_dir(&p));
            let key = p.to_string_lossy().into_owned();
            let text = format_bytes(size);
            let _ = slint::invoke_from_event_loop(move || {
                let ctx = ctx::get();
                ctx.usage.borrow_mut().insert(key, text);
                ctx.refresh();
            });
        }
    });
}

// ---------------------------------------------------------------- curated lists

fn fetch_text(url: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

/// Show the cached lists straight away, then refresh them from GitHub in the background.
pub fn load_curated(ctx: &Ctx) {
    let rec = curated::load_cached(&ctx.paths, CuratedKind::Recommended);
    let other = curated::load_cached(&ctx.paths, CuratedKind::Other);
    let have_cache = !rec.is_empty() || !other.is_empty();
    *ctx.curated.borrow_mut() = (rec, other);
    *ctx.curated_servers.borrow_mut() = curated::load_servers(&ctx.paths);
    ctx.refresh_curated(if have_cache {
        "Checking for updates..."
    } else {
        "Loading..."
    });

    let paths = ctx.paths.clone();
    std::thread::spawn(move || {
        let fetch = |kind: CuratedKind| {
            fetch_text(&kind.url()).and_then(|text| {
                curated::store_fetched(&paths, kind, &text).map_err(|e| e.to_string())
            })
        };
        let rec = fetch(CuratedKind::Recommended);
        let other = fetch(CuratedKind::Other);
        // The server list may not be published yet; a failure here is not worth a warning on the page.
        let servers = fetch_text(&curated::servers_url()).and_then(|text| {
            curated::store_fetched_servers(&paths, &text).map_err(|e| e.to_string())
        });
        let _ = slint::invoke_from_event_loop(move || {
            let ctx = ctx::get();
            let ok = rec.is_ok() && other.is_ok();
            match servers {
                Ok(list) if !list.is_empty() => *ctx.curated_servers.borrow_mut() = list,
                Ok(_) => {}
                Err(e) => {
                    tracing::info!(error = %e, "server build list not fetched; using the saved or bundled one")
                }
            }
            if let Err(e) = &rec {
                tracing::warn!(error = %e, "could not fetch versionRL.json");
            }
            if let Err(e) = &other {
                tracing::warn!(error = %e, "could not fetch versionOther.json");
            }
            {
                let mut cur = ctx.curated.borrow_mut();
                if let Ok(list) = rec {
                    cur.0 = list;
                }
                if let Ok(list) = other {
                    cur.1 = list;
                }
            }
            ctx.refresh_curated(if ok {
                "Up to date."
            } else {
                "Could not reach GitHub, showing the saved list."
            });
        });
    });
}
