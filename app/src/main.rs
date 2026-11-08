// A release build is a GUI program: no console window.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod actions;
mod ctx;
mod format;
mod single_instance;
mod tasks;

slint::include_modules!();

use ctx::Ctx;
use rl_core::curated::{CuratedKind, load_cached};
use rl_core::paths::AppPaths;
use rl_core::state;
use std::cell::RefCell;
use std::collections::HashMap;
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
            state: Default::default(),
            recovered_from: None,
            first_run: true,
        }
    });
    let startup_notice = outcome.recovered_from.as_ref().map(|moved| {
        tracing::warn!(moved = %moved.display(), "state.json was damaged and has been set aside");
        "Your settings file was damaged. It was set aside and a fresh one started.".to_string()
    });

    let app = App::new().expect("could not create the window");
    app.set_app_version(env!("CARGO_PKG_VERSION").into());
    app.set_data_dir(paths.root().display().to_string().into());

    let cached = (
        load_cached(&paths, CuratedKind::Recommended),
        load_cached(&paths, CuratedKind::Other),
    );
    let ctx = Rc::new(Ctx {
        paths: paths.clone(),
        state: RefCell::new(outcome.state),
        app: app.as_weak(),
        cancel: RefCell::new(None),
        archives: RefCell::new(Vec::new()),
        usage: RefCell::new(HashMap::new()),
        curated: RefCell::new(cached),
    });
    ctx::install(ctx.clone());
    ctx.refresh();
    ctx.refresh_curated("");
    if let Ok(page) = std::env::var("RL_START_PAGE") {
        app.set_page(page.into());
    }

    wire_callbacks(&app);

    if let Some(notice) = startup_notice {
        app.invoke_show_toast(notice.into(), "error".into());
    }

    // Work that must not delay the first frame runs once the event loop is up.
    slint::Timer::single_shot(Duration::from_millis(100), || {
        let ctx = ctx::get();
        actions::scan_in_background(true);
        actions::reload_archives(&ctx);
        actions::refresh_usage(&ctx);
        actions::load_curated(&ctx);
    });

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

fn wire_callbacks(app: &App) {
    app.on_launch_version(|id| actions::launch_version(&id));
    app.on_launch_selected(actions::launch_selected);
    app.on_pick_version(|id| actions::pick_version(&id));
    app.on_eac_changed(actions::eac_changed);
    app.on_browse_version(|id| actions::browse(&id));
    app.on_restore_archive(|path| actions::restore_archive(&path));
    app.on_rescan(actions::rescan);
    app.on_add_install_path(actions::add_install_path);
    app.on_cancel_task(actions::cancel_task);
    app.on_dialog_confirmed(|kind, arg, text| actions::dialog_confirmed(&kind, &arg, &text));
    // The window has no standard title bar, so the close button is ours: hiding the only window ends the event loop.
    let weak = app.as_weak();
    app.on_close_clicked(move || {
        if let Some(app) = weak.upgrade() {
            let _ = app.hide();
        }
    });
}
