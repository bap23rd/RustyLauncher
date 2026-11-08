//! Shared application context: the state, the window handle, and pushing state into the UI.
//!
//! Everything here lives on the UI thread. Background threads never touch it directly; they send
//! plain data back with `slint::invoke_from_event_loop` and the closure fetches the context with [`get`].

use crate::{App, ArchiveTile, CuratedTile, PathRow, VersionTile, format};
use rl_core::archive::Archive;
use rl_core::curated::CuratedBuild;
use rl_core::launch::{has_client, has_eac};
use rl_core::library::format_bytes;
use rl_core::model::{State, Version};
use rl_core::paths::{AppPaths, versions_dir};
use rl_core::state;
use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub struct Ctx {
    pub paths: AppPaths,
    pub state: RefCell<State>,
    pub app: slint::Weak<App>,
    /// Set while a background job runs; the flag is how the Cancel button reaches it.
    pub cancel: RefCell<Option<Arc<AtomicBool>>>,
    pub archives: RefCell<Vec<Archive>>,
    /// Disk usage text per install path (filled in by a background thread).
    pub usage: RefCell<HashMap<String, String>>,
    pub curated: RefCell<(Vec<CuratedBuild>, Vec<CuratedBuild>)>,
}

thread_local! {
    static CTX: RefCell<Option<Rc<Ctx>>> = const { RefCell::new(None) };
}

pub fn install(ctx: Rc<Ctx>) {
    CTX.with(|c| *c.borrow_mut() = Some(ctx));
}

/// The context. Only callable on the UI thread, after [`install`].
pub fn get() -> Rc<Ctx> {
    CTX.with(|c| {
        c.borrow()
            .clone()
            .expect("context is installed at start-up")
    })
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Ctx {
    pub fn with_app(&self, f: impl FnOnce(&App)) {
        if let Some(app) = self.app.upgrade() {
            f(&app);
        }
    }

    pub fn toast(&self, text: &str, kind: &str) {
        self.with_app(|a| a.invoke_show_toast(text.into(), kind.into()));
    }

    pub fn busy(&self) -> bool {
        self.cancel.borrow().is_some()
    }

    /// Save `state.json`; tell the user if that fails.
    pub fn save(&self) {
        if let Err(err) = state::save(&self.paths, &self.state.borrow()) {
            tracing::error!(%err, "could not save state");
            self.toast("Could not save your settings.", "error");
        }
    }

    /// Folders that can hold versions or an `archive` folder: every install path's versions folder,
    /// and the folder each known version sits in.
    pub fn versions_dirs(&self) -> Vec<PathBuf> {
        let state = self.state.borrow();
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let candidates = state
            .install_paths
            .iter()
            .map(|p| versions_dir(&p.path))
            .chain(
                state
                    .versions
                    .iter()
                    .filter_map(|v| v.install_path.parent().map(|p| p.to_path_buf())),
            );
        for dir in candidates {
            if seen.insert(dir.to_string_lossy().to_lowercase()) {
                out.push(dir);
            }
        }
        out
    }

    /// The version the quick-launch bar targets: last played, or the first one if none.
    pub fn selected_id(&self) -> Option<String> {
        let state = self.state.borrow();
        state
            .settings
            .last_played_version
            .clone()
            .filter(|id| state.version_by_id(id).is_some())
            .or_else(|| state.versions.first().map(|v| v.id.clone()))
    }

    /// Push the state into the UI.
    pub fn refresh(&self) {
        let state = self.state.borrow();
        let selected = self.selected_id();
        let usage = self.usage.borrow();
        self.with_app(|app| {
            let mut tiles: Vec<VersionTile> = state.versions.iter().map(tile).collect();
            tiles.sort_by_key(|t| t.name.to_lowercase());
            app.set_versions(ModelRc::from(Rc::new(VecModel::from(tiles))));
            app.set_server_count(state.servers.len() as i32);
            app.set_launch_with_eac(state.settings.launch_with_eac);

            match selected.as_deref().and_then(|id| state.version_by_id(id)) {
                Some(v) => {
                    app.set_has_last_played(true);
                    app.set_selected_id(v.id.as_str().into());
                    app.set_last_played_name(v.name.as_str().into());
                    app.set_last_played_detail(played_text(v).into());
                }
                None => {
                    app.set_has_last_played(false);
                    app.set_selected_id("".into());
                    app.set_last_played_name("".into());
                    app.set_last_played_detail("Download a version to get started".into());
                }
            }

            let rows: Vec<PathRow> = state
                .install_paths
                .iter()
                .map(|p| {
                    let key = p.path.to_string_lossy().into_owned();
                    PathRow {
                        usage: usage
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| "Calculating...".to_string())
                            .into(),
                        path: key.into(),
                    }
                })
                .collect();
            app.set_path_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
        });
    }

    pub fn refresh_archives(&self) {
        let tiles: Vec<ArchiveTile> = self
            .archives
            .borrow()
            .iter()
            .map(|a| ArchiveTile {
                name: a.name.as_str().into(),
                size: format_bytes(a.size).into(),
                path: a.path.to_string_lossy().as_ref().into(),
            })
            .collect();
        self.with_app(|app| app.set_archives(ModelRc::from(Rc::new(VecModel::from(tiles)))));
    }

    pub fn refresh_curated(&self, status: &str) {
        let (rec, other) = &*self.curated.borrow();
        let to_tiles = |list: &Vec<CuratedBuild>| -> ModelRc<CuratedTile> {
            let v: Vec<CuratedTile> = list
                .iter()
                .map(|b| CuratedTile {
                    title: b.title.as_str().into(),
                    date: b.version_date.as_str().into(),
                    client: b.client_manifest.as_str().into(),
                    bundle: b.bundle_manifest.as_str().into(),
                })
                .collect();
            ModelRc::from(Rc::new(VecModel::from(v)))
        };
        self.with_app(|app| {
            app.set_curated_recommended(to_tiles(rec));
            app.set_curated_other(to_tiles(other));
            app.set_curated_status(status.into());
        });
    }
}

fn played_text(v: &Version) -> String {
    match v.last_played {
        Some(t) => format!("Played {}", format::ymd(t)),
        None => "Never played".to_string(),
    }
}

fn tile(v: &Version) -> VersionTile {
    VersionTile {
        id: v.id.as_str().into(),
        name: v.name.as_str().into(),
        manifest: v.manifest_id.as_str().into(),
        date: v
            .download_date
            .as_deref()
            .map(format::iso_date)
            .unwrap_or_default()
            .into(),
        played: v
            .last_played
            .map(format::ymd)
            .unwrap_or_else(|| "Never".to_string())
            .into(),
        eac: has_eac(&v.install_path),
        missing: !has_client(&v.install_path),
    }
}
