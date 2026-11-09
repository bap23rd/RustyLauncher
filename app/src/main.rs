// A release build is a GUI program: no console window.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod actions;
mod ctx;
mod downloads;
mod format;
mod servers;
mod single_instance;
mod steam;
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
use std::sync::Arc;
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

    let data_dir_key = std::env::var("RL_DATA_DIR").ok();
    if !single_instance::acquire(WINDOW_TITLE, data_dir_key.as_deref()) {
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
        curated_servers: RefCell::new(rl_core::curated::load_servers(&paths)),
        engine: rl_steam::Engine::new(),
        store: Arc::new(rl_steam::CredentialManager),
        signin: RefCell::new(None),
        downloads: RefCell::new(Default::default()),
        download_dir: RefCell::new(None),
    });
    ctx::install(ctx.clone());
    ctx.refresh();
    ctx.refresh_curated("");
    if let Ok(page) = std::env::var("RL_START_PAGE") {
        app.set_page(page.into());
    }

    wire_callbacks(&app);

    // Debug builds only: `RL_DEMO_SIGNIN=phone` (or `phone-only`) opens the sign-in dialog in its Steam Guard state.
    #[cfg(debug_assertions)]
    if let Ok(mode) = std::env::var("RL_DEMO_SIGNIN") {
        app.set_signin_state("guard".into());
        app.set_signin_phone(true);
        app.set_signin_guard_kind(
            if mode == "phone-only" {
                "none"
            } else {
                "authenticator"
            }
            .into(),
        );
        app.set_signin_open(true);
    }

    // Debug builds only: `RL_DEMO_DOWNLOAD=1` shows a made-up running download, for checking its layout.
    #[cfg(debug_assertions)]
    if std::env::var_os("RL_DEMO_DOWNLOAD").is_some() {
        use slint::{ModelRc, VecModel};
        app.set_downloads(ModelRc::from(Rc::new(VecModel::from(vec![
            DownloadRow {
                id: 1,
                name: "Rust Stable July 2023".into(),
                status: "78%  ·  16.4 GB of 21.1 GB  ·  23.4 MB/s  ·  about 3 min left".into(),
                progress: 0.78,
                active: true,
            },
            DownloadRow {
                id: 2,
                name: "Rust pre-graphics overhaul".into(),
                status: "Waiting in line".into(),
                progress: 0.0,
                active: false,
            },
        ]))));
        app.set_download_active(true);
        app.set_download_text(
            "Downloading Rust Stable July 2023:  78%  ·  16.4 GB of 21.1 GB  ·  23.4 MB/s  ·  about 3 min left"
                .into(),
        );
        app.set_download_progress(0.78);
    }

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
        // (the debug-only demo download would be wiped by this refresh)
        if !(cfg!(debug_assertions) && std::env::var_os("RL_DEMO_DOWNLOAD").is_some()) {
            downloads::refresh_ui(&ctx);
        }
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
    app.on_install_server_latest(|name| downloads::start_server_latest(&name));
    app.on_install_server_pinned(|name, win64, common| {
        downloads::start_server_pinned(&name, &win64, &common)
    });
    app.on_configure_server(|id| servers::configure(&id));
    app.on_save_server_config(servers::save_config);
    app.on_new_server_password(servers::new_password);
    app.on_browse_server(|id| servers::browse(&id));
    app.on_server_mods(|id| servers::mods(&id));
    app.on_update_server(|id| downloads::update_server(&id));
    app.on_signin_submit(|user, pass| steam::begin_sign_in(&user, &pass));
    app.on_signin_code_submit(|code| steam::submit_code(&code));
    app.on_signin_cancel(steam::cancel_sign_in);
    app.on_sign_out(steam::sign_out);
    app.on_download_build(|_| {}); // the page asks for confirmation first; see dialog_confirmed
    app.on_start_manual_download(|_, _, _| {});
    app.on_cancel_download(downloads::cancel);
    app.on_resume_download(|path| downloads::resume(&path));
    app.on_change_download_dir(downloads::change_dir);
    app.on_open_url(|url| actions::open_url(&url));
    app.on_dialog_confirmed(|kind, arg, text| actions::dialog_confirmed(&kind, &arg, &text));
    // The window has no standard title bar, so the close button is ours: hiding the only window ends the event loop.
    let weak = app.as_weak();
    app.on_close_clicked(move || {
        if let Some(app) = weak.upgrade() {
            if downloads::is_busy() {
                app.invoke_ask_quit();
            } else {
                let _ = app.hide();
            }
        }
    });
}
