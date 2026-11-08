//! `state.json`: load, and save atomically.

use crate::error::{Error, Result};
use crate::fsutil::write_atomic;
use crate::model::State;
use crate::paths::AppPaths;
use std::path::PathBuf;

pub struct LoadOutcome {
    pub state: State,
    /// Set when `state.json` existed but could not be read. The bad file was moved here
    /// and a fresh state returned; the caller should rebuild the library from sidecars.
    pub recovered_from: Option<PathBuf>,
    /// True when there was no `state.json` at all (first run, so the legacy importer should run).
    pub first_run: bool,
}

pub fn load(paths: &AppPaths) -> Result<LoadOutcome> {
    let file = paths.state_file();
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoadOutcome {
                state: State::default(),
                recovered_from: None,
                first_run: true,
            });
        }
        Err(e) => return Err(Error::io(&file, e)),
    };
    match serde_json::from_str::<State>(&text) {
        Ok(state) => Ok(LoadOutcome {
            state,
            recovered_from: None,
            first_run: false,
        }),
        Err(err) => {
            tracing::warn!(%err, file = %file.display(), "state.json is unreadable; starting fresh");
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut moved = file.as_os_str().to_owned();
            moved.push(format!(".corrupt-{stamp}"));
            let moved = PathBuf::from(moved);
            std::fs::rename(&file, &moved).map_err(|e| Error::io(&file, e))?;
            Ok(LoadOutcome {
                state: State::default(),
                recovered_from: Some(moved),
                first_run: false,
            })
        }
    }
}

pub fn save(paths: &AppPaths, state: &State) -> Result<()> {
    let file = paths.state_file();
    let json = serde_json::to_vec_pretty(state).map_err(|e| Error::json(&file, e))?;
    write_atomic(&file, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Version;

    #[test]
    fn missing_file_is_a_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let out = load(&AppPaths::at(dir.path())).unwrap();
        assert!(out.first_run);
        assert_eq!(out.state, State::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        let mut s = State::default();
        s.settings.steam_username = Some("bap23rd".into());
        s.add_version(Version {
            id: "1".into(),
            name: "One".into(),
            install_path: r"D:\x\1".into(),
            ..Default::default()
        });
        save(&paths, &s).unwrap();
        let out = load(&paths).unwrap();
        assert!(!out.first_run);
        assert_eq!(out.state, s);
        assert!(
            !dir.path().join("state.json.tmp").exists(),
            "temp file must be renamed away"
        );
    }

    #[test]
    fn corrupt_file_is_set_aside_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        std::fs::write(paths.state_file(), "{ this is not json").unwrap();
        let out = load(&paths).unwrap();
        let moved = out.recovered_from.expect("should report the moved file");
        assert!(moved.exists());
        assert_eq!(
            std::fs::read_to_string(moved).unwrap(),
            "{ this is not json"
        );
        assert_eq!(out.state, State::default());
        assert!(!paths.state_file().exists());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        std::fs::write(
            paths.state_file(),
            r#"{"settings":{"launchWithEac":false},"somethingNew":1}"#,
        )
        .unwrap();
        let out = load(&paths).unwrap();
        assert!(!out.state.settings.launch_with_eac);
        assert!(out.state.versions.is_empty());
    }
}
