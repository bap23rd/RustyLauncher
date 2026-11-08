// A release build is a GUI program: no console window.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod format;
mod single_instance;

slint::include_modules!();

use rl_core::model::{State, Version};
use rl_core::paths::AppPaths;
use rl_core::state;
use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

const WINDOW_TITLE: &str = "RustyLauncher";

fn main() {
    let started = Instant::now();

    // `RL_DATA_DIR` points the app at a different data folder (testing, or a portable setup).
    let paths = match std::env::var_os("RL_DATA_DIR") {
        Some(dir) => AppPaths::at(PathBuf::from(dir)),
        None => AppPaths::default_location()
            .unwrap_or_else(|_| AppPaths::at(PathBuf::from(".rustylauncher"))),
    };
    let _log = rl_core::logging::init(&paths).ok();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "RustyLauncher starting"
    );

    if !single_instance::acquire(WINDOW_TITLE) {
        tracing::info!("another instance is already running; focused it and exiting");
        return;
    }

    let outcome = state::load(&paths).unwrap_or_else(|err| {
        tracing::error!(%err, "could not load state; using defaults");
        state::LoadOutcome {
            state: State::default(),
            recovered_from: None,
            first_run: true,
        }
    });
    let startup_notice = outcome.recovered_from.as_ref().map(|moved| {
        tracing::warn!(moved = %moved.display(), "state.json was damaged and has been set aside");
        "Your settings file was damaged. It was set aside and a fresh one started.".to_string()
    });
    let state = Rc::new(RefCell::new(outcome.state));

    let app = App::new().expect("could not create the window");
    app.set_app_version(env!("CARGO_PKG_VERSION").into());
    app.set_data_dir(paths.root().display().to_string().into());
    refresh(&app, &state.borrow());
    if let Ok(page) = std::env::var("RL_START_PAGE") {
        app.set_page(page.into());
    }

    {
        let weak = app.as_weak();
        app.on_launch_clicked(move || {
            if let Some(app) = weak.upgrade() {
                app.invoke_show_toast("Launching games arrives in Phase 3.".into(), "info".into());
            }
        });
    }
    {
        let state = state.clone();
        let paths = paths.clone();
        let weak = app.as_weak();
        app.on_eac_changed(move |on| {
            state.borrow_mut().settings.launch_with_eac = on;
            if let Err(err) = state::save(&paths, &state.borrow()) {
                tracing::error!(%err, "could not save settings");
                if let Some(app) = weak.upgrade() {
                    app.invoke_show_toast("Could not save your settings.".into(), "error".into());
                }
            }
        });
    }

    if let Some(notice) = startup_notice {
        app.invoke_show_toast(notice.into(), "error".into());
    }

    // Work that must not delay the first frame runs from the event loop once it is up.
    {
        let weak = app.as_weak();
        slint::Timer::single_shot(Duration::from_millis(100), move || {
            if let Some(app) = weak.upgrade() {
                load_backgrounds(&app);
            }
        });
    }
    // `RL_STARTUP_PROBE=1` prints the time of the first event-loop turn and exits (used to check the startup budget).
    if std::env::var_os("RL_STARTUP_PROBE").is_some() {
        slint::Timer::single_shot(Duration::from_millis(30), move || {
            let unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            eprintln!(
                "startup-ms={} unix-ms={unix_ms}",
                started.elapsed().as_millis()
            );
            let _ = slint::quit_event_loop();
        });
    }

    tracing::info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        "window ready, entering event loop"
    );
    app.run().expect("the UI event loop failed");
    tracing::info!("RustyLauncher exited");
}

/// Push state into the UI.
fn refresh(app: &App, state: &State) {
    let tiles: Vec<VersionTile> = state.versions.iter().map(tile).collect();
    app.set_versions(ModelRc::from(Rc::new(VecModel::from(tiles))));
    app.set_server_count(state.servers.len() as i32);
    app.set_launch_with_eac(state.settings.launch_with_eac);

    let last = state
        .settings
        .last_played_version
        .as_deref()
        .and_then(|id| state.version_by_id(id))
        .or_else(|| {
            state
                .versions
                .iter()
                .filter(|v| v.last_played.is_some())
                .max_by_key(|v| v.last_played)
        });
    match last {
        Some(v) => {
            app.set_has_last_played(true);
            app.set_last_played_name(v.name.as_str().into());
            app.set_last_played_detail(played_text(v).into());
        }
        None => {
            app.set_has_last_played(false);
            app.set_last_played_name("".into());
            app.set_last_played_detail("Download a version to get started".into());
        }
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
        eac: v.eac_enabled,
    }
}

/// Load background pictures from `assets/backgrounds` next to the program, after the first frame.
/// With no pictures the background simply stays the plain dark brown.
fn load_backgrounds(app: &App) {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("assets").join("backgrounds")))
    else {
        return;
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg"))
        })
        .collect();
    files.sort();
    let mut images = files
        .iter()
        .filter_map(|p| slint::Image::load_from_path(p).ok());
    if let Some(a) = images.next() {
        // With one picture, use it for both layers so the fade is a no-op.
        let b = images.next().unwrap_or_else(|| a.clone());
        app.set_bg_a(a);
        app.set_bg_b(b);
        app.set_bg_ready(true);
    }
}
