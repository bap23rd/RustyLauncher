//! Starting the game.
//!
//! With Easy Anti-Cheat the game is started through `Rust.exe`; without it, `RustClient.exe`
//! is started directly. The game is detached, so closing the launcher never closes the game.

use std::path::{Path, PathBuf};

const EAC_EXE: &str = "Rust.exe";
const NO_EAC_EXE: &str = "RustClient.exe";

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("{exe} was not found in this version's folder.")]
    MissingExe { exe: &'static str, dir: PathBuf },
    #[error("Windows could not start {exe}: {source}")]
    Spawn {
        exe: &'static str,
        #[source]
        source: std::io::Error,
    },
}

/// Which program starts the game.
pub fn game_exe(eac: bool) -> &'static str {
    if eac { EAC_EXE } else { NO_EAC_EXE }
}

/// Does this version have the Easy Anti-Cheat launcher?
pub fn has_eac(version_dir: &Path) -> bool {
    version_dir.join(EAC_EXE).is_file()
}

/// Is the game itself present (what a "version" needs to be playable at all)?
pub fn has_client(version_dir: &Path) -> bool {
    version_dir.join(NO_EAC_EXE).is_file()
}

/// Start the game from `version_dir` and return immediately.
pub fn launch(version_dir: &Path, eac: bool) -> Result<(), LaunchError> {
    let exe_name = game_exe(eac);
    let exe = version_dir.join(exe_name);
    if !exe.is_file() {
        return Err(LaunchError::MissingExe {
            exe: exe_name,
            dir: version_dir.to_path_buf(),
        });
    }
    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(version_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    // Dropping the child handle does not stop the process.
    cmd.spawn().map(drop).map_err(|source| LaunchError::Spawn {
        exe: exe_name,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_right_executable() {
        assert_eq!(game_exe(true), "Rust.exe");
        assert_eq!(game_exe(false), "RustClient.exe");
    }

    #[test]
    fn missing_exe_gives_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = launch(dir.path(), true).unwrap_err();
        assert!(matches!(
            err,
            LaunchError::MissingExe {
                exe: "Rust.exe",
                ..
            }
        ));
        assert!(err.to_string().contains("Rust.exe was not found"));
    }

    #[test]
    fn detects_eac_and_client() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_eac(dir.path()) && !has_client(dir.path()));
        std::fs::write(dir.path().join("RustClient.exe"), b"x").unwrap();
        assert!(has_client(dir.path()) && !has_eac(dir.path()));
        std::fs::write(dir.path().join("Rust.exe"), b"x").unwrap();
        assert!(has_eac(dir.path()));
    }

    #[cfg(windows)]
    #[test]
    fn starts_a_real_program_in_the_version_folder() {
        // cmd.exe stands in for the game: it starts, finds no input, and exits.
        let dir = tempfile::tempdir().unwrap();
        let cmd = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("cmd.exe");
        std::fs::copy(cmd, dir.path().join("RustClient.exe")).unwrap();
        launch(dir.path(), false).unwrap();
    }
}
