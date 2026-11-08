//! Logging to a rolling file in `%APPDATA%\RustyLauncher\logs\`.
//!
//! Secrets are never logged: wrap them in [`crate::secret::Secret`], and never put a password,
//! Steam Guard code, refresh token or RCON password in a log message.

use crate::error::{Error, Result};
use crate::paths::AppPaths;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

/// Keep this alive for the whole run: dropping it flushes and stops the log writer.
pub struct LogGuard {
    _guard: WorkerGuard,
}

/// Daily files `rustylauncher.<date>.log`, last 7 kept. Honours `RUST_LOG`; defaults to `info`.
pub fn init(paths: &AppPaths) -> Result<LogGuard> {
    let dir = paths.logs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("rustylauncher")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .map_err(|e| Error::io(&dir, std::io::Error::other(e.to_string())))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Ignore the error: a subscriber may already be installed (tests, or a second call).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();

    // Panics are written to the log too, since a release build has no console.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "the application panicked");
        previous(info);
    }));

    Ok(LogGuard { _guard: guard })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_the_log_folder_and_a_log_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        {
            let _guard = init(&paths).unwrap();
            tracing::info!("hello from the test");
        } // dropping the guard flushes the writer
        let files: Vec<_> = std::fs::read_dir(paths.logs_dir())
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().to_string_lossy().into_owned();
        assert!(
            name.starts_with("rustylauncher") && name.ends_with(".log"),
            "{name}"
        );
    }
}
