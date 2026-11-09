//! Talks to the real Steam servers (no account needed). Run on purpose with:
//!
//!     cargo test -p rl-steam -- --ignored --nocapture
//!
//! It finds the current dedicated-server build, then downloads one small file from it through the
//! same engine the launcher uses.

use rl_steam::{Auth, DownloadEvent, DownloadRequest, Engine, latest_manifests};
use std::sync::{Arc, Mutex};

const SERVER_APP: u32 = 258550;
const SERVER_WIN64_DEPOT: u32 = 258551;
const SERVER_COMMON_DEPOT: u32 = 258554;

#[test]
#[ignore = "uses the network"]
fn finds_the_current_server_build_anonymously() {
    let engine = Engine::new();
    let specs = engine
        .block_on(latest_manifests(
            &Auth::Anonymous,
            SERVER_APP,
            &[SERVER_WIN64_DEPOT, SERVER_COMMON_DEPOT],
            "public",
        ))
        .expect("PICS lookup");
    println!("current manifests: {specs:?}");
    assert_eq!(specs.len(), 2);
    assert!(specs.iter().all(|s| s.manifest_id > 0));
}

#[test]
#[ignore = "uses the network"]
fn downloads_one_small_file_anonymously() {
    let engine = Engine::new();
    let specs = engine
        .block_on(latest_manifests(
            &Auth::Anonymous,
            SERVER_APP,
            &[SERVER_WIN64_DEPOT],
            "public",
        ))
        .expect("PICS lookup");

    let dir = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let events: Arc<Mutex<Vec<DownloadEvent>>> = Arc::default();
    let sink = events.clone();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let _handle = engine.download(
        DownloadRequest {
            app_id: SERVER_APP,
            depots: specs,
            install_dir: dir.path().to_path_buf(),
            manifest_cache_dir: cache.path().to_path_buf(),
            branch: "public".into(),
            only_files: Some(vec![r"^steam_api64\.dll$".into()]),
            max_downloads: None,
        },
        Auth::Anonymous,
        move |ev| {
            let finished = matches!(
                ev,
                DownloadEvent::Finished(_) | DownloadEvent::Failed(_) | DownloadEvent::Cancelled
            );
            sink.lock().unwrap().push(ev);
            if finished {
                let _ = done_tx.send(());
            }
        },
    );
    done_rx
        .recv_timeout(std::time::Duration::from_secs(120))
        .expect("the download finished in time");

    let events = events.lock().unwrap();
    for e in events.iter() {
        println!("{e:?}");
    }
    assert!(
        matches!(events.last(), Some(DownloadEvent::Finished(_))),
        "last event: {:?}",
        events.last()
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DownloadEvent::Total { bytes, .. } if *bytes > 0))
    );
    let file = dir.path().join("steam_api64.dll");
    assert!(file.is_file(), "the file was written");
    assert!(std::fs::metadata(&file).unwrap().len() > 10_000);
    // Bookkeeping for resume is left beside the files.
    assert!(dir.path().join(".DepotDownloader").exists());
    // The manifest was cached.
    assert!(std::fs::read_dir(cache.path()).unwrap().count() > 0);

    // Running it again finds the file already correct and fetches nothing.
    let (tx2, rx2) = std::sync::mpsc::channel();
    let specs2 = engine
        .block_on(latest_manifests(
            &Auth::Anonymous,
            SERVER_APP,
            &[SERVER_WIN64_DEPOT],
            "public",
        ))
        .unwrap();
    let _h2 = engine.download(
        DownloadRequest {
            app_id: SERVER_APP,
            depots: specs2,
            install_dir: dir.path().to_path_buf(),
            manifest_cache_dir: cache.path().to_path_buf(),
            branch: "public".into(),
            only_files: Some(vec![r"^steam_api64\.dll$".into()]),
            max_downloads: None,
        },
        Auth::Anonymous,
        move |ev| {
            if let DownloadEvent::Finished(s) = &ev {
                let _ = tx2.send(s.clone());
            }
        },
    );
    let summary = rx2
        .recv_timeout(std::time::Duration::from_secs(120))
        .expect("second run finished");
    println!("second run: {summary:?}");
    assert_eq!(
        summary.files_completed, 0,
        "nothing to fetch the second time"
    );
}

/// Cancel a real download part-way, then resume it. The resume must keep what was already fetched and
/// end with every file correct. Roughly 730 MB.
#[test]
#[ignore = "uses the network and downloads about 730 MB"]
fn cancel_then_resume_a_whole_depot() {
    let engine = Engine::new();
    let specs = engine
        .block_on(latest_manifests(
            &Auth::Anonymous,
            SERVER_APP,
            &[SERVER_WIN64_DEPOT],
            "public",
        ))
        .expect("PICS lookup");
    let dir = std::env::var_os("RL_TEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("rl-resume-test"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = tempfile::tempdir().unwrap();
    let request = |specs: Vec<rl_steam::DepotSpec>| DownloadRequest {
        app_id: SERVER_APP,
        depots: specs,
        install_dir: dir.clone(),
        manifest_cache_dir: cache.path().to_path_buf(),
        branch: "public".into(),
        only_files: None,
        max_downloads: None,
    };

    // 1) start, then cancel once a good part has arrived
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_done = tx.clone();
    let handle = engine.download(
        request(specs.clone()),
        Auth::Anonymous,
        move |ev| match ev {
            DownloadEvent::Progress { done, total } => {
                let _ = tx.send(Some((done, total)));
            }
            DownloadEvent::Cancelled | DownloadEvent::Finished(_) | DownloadEvent::Failed(_) => {
                let _ = tx_done.send(None);
            }
            _ => {}
        },
    );
    let started = std::time::Instant::now();
    let mut at_cancel = 0u64;
    let mut total = 0u64;
    while let Ok(msg) = rx.recv_timeout(std::time::Duration::from_secs(120)) {
        match msg {
            Some((done, t)) => {
                total = t;
                if done > t / 3 {
                    at_cancel = done;
                    handle.cancel();
                    break;
                }
            }
            None => break,
        }
    }
    // drain until the engine reports it stopped
    while let Ok(msg) = rx.recv_timeout(std::time::Duration::from_secs(30)) {
        if msg.is_none() {
            break;
        }
    }
    println!(
        "cancelled at {} of {} bytes after {:.1}s",
        at_cancel,
        total,
        started.elapsed().as_secs_f32()
    );
    assert!(at_cancel > 0, "should have made progress before the cancel");
    let on_disk_after_cancel = rl_core::library::dir_size(&dir);
    println!("on disk after cancel: {on_disk_after_cancel} bytes");

    // 2) resume: verify keeps finished files, fetches the rest
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let t1 = std::time::Instant::now();
    let _h = engine.download(request(specs), Auth::Anonymous, move |ev| {
        if let DownloadEvent::Finished(s) = &ev {
            let _ = done_tx.send(Ok(s.clone()));
        } else if let DownloadEvent::Failed(f) = &ev {
            let _ = done_tx.send(Err(f.message.clone()));
        }
    });
    let summary = done_rx
        .recv_timeout(std::time::Duration::from_secs(300))
        .expect("resume finished in time")
        .expect("resume succeeded");
    println!("resume: {summary:?} in {:.1}s", t1.elapsed().as_secs_f32());
    assert!(
        summary.files_skipped > 0,
        "files fetched before the cancel are kept"
    );
    let final_size = rl_core::library::dir_size(&dir);
    println!("final size on disk: {final_size}");
    assert!(final_size >= total, "everything is on disk");
    assert!(dir.join("RustDedicated.exe").is_file());
}

/// Measures download speed for several values of `max_downloads` (about 730 MB each run):
///
///     RL_PARALLEL=16,32,8,16 cargo test --release -p rl-steam --test anonymous_download \
///         throughput_by_parallelism -- --ignored --nocapture
#[test]
#[ignore = "uses the network and downloads about 730 MB per value"]
fn throughput_by_parallelism() {
    let values: Vec<usize> = std::env::var("RL_PARALLEL")
        .unwrap_or_else(|_| "16".into())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let engine = Engine::new();
    let specs = engine
        .block_on(latest_manifests(
            &Auth::Anonymous,
            SERVER_APP,
            &[SERVER_WIN64_DEPOT],
            "public",
        ))
        .expect("PICS lookup");
    for n in values {
        let dir = tempfile::tempdir_in(
            std::env::var_os("RL_TEST_DIR").unwrap_or_else(|| std::env::temp_dir().into()),
        )
        .unwrap();
        let cache = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let t0 = std::time::Instant::now();
        let _h = engine.download(
            DownloadRequest {
                app_id: SERVER_APP,
                depots: specs.clone(),
                install_dir: dir.path().to_path_buf(),
                manifest_cache_dir: cache.path().to_path_buf(),
                branch: "public".into(),
                only_files: None,
                max_downloads: Some(n),
            },
            Auth::Anonymous,
            move |ev| {
                if matches!(ev, DownloadEvent::Finished(_) | DownloadEvent::Failed(_)) {
                    let _ = tx.send(ev);
                }
            },
        );
        let ev = rx
            .recv_timeout(std::time::Duration::from_secs(600))
            .expect("finished");
        let secs = t0.elapsed().as_secs_f64();
        assert!(matches!(ev, DownloadEvent::Finished(_)), "{ev:?}");
        let bytes = rl_core::library::dir_size(dir.path()) as f64;
        println!(
            "max_downloads={n:>3}: {:.1}s  {:.1} MB/s",
            secs,
            bytes / 1e6 / secs
        );
    }
}
