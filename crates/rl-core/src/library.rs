//! Library operations on installed versions: rename, delete, size.
//!
//! Deleting folders is the one dangerous thing this launcher does, so it refuses anything that
//! does not look like a game install.

use crate::launch::has_client;
use crate::model::State;
use crate::sidecar::{
    VERSION_SIDECAR, VersionSidecar, read_version_sidecar, write_version_sidecar,
};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("That version is no longer in the library.")]
    NotFound,
    #[error("Please enter a name.")]
    EmptyName,
    #[error(
        "Refusing to delete {0}: it does not look like a Rust install, and deleting the wrong folder could lose your files."
    )]
    NotAnInstall(PathBuf),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// A folder is only treated as an install if it holds the game or one of our sidecars,
/// and is not a drive root or a top-level folder.
pub fn looks_like_install(dir: &Path) -> bool {
    let deep_enough = dir.components().count() >= 3;
    deep_enough && dir.is_dir() && (has_client(dir) || dir.join(VERSION_SIDECAR).is_file())
}

/// Rename a version, in memory and in its sidecar (the sidecar is the source of truth,
/// so the name survives a lost `state.json`). The caller saves the state.
pub fn rename_version(state: &mut State, id: &str, new_name: &str) -> Result<(), LibraryError> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err(LibraryError::EmptyName);
    }
    let v = state
        .versions
        .iter_mut()
        .find(|v| v.id == id)
        .ok_or(LibraryError::NotFound)?;
    v.name = new_name.to_string();

    // Update the sidecar when the folder is there; a missing folder only changes the name in state.
    if v.install_path.is_dir() {
        let mut sidecar = read_version_sidecar(&v.install_path)
            .ok()
            .flatten()
            .unwrap_or_else(|| VersionSidecar {
                manifest_id: v.manifest_id.clone(),
                download_date: v.download_date.clone(),
                bundles_manifest_id: v.bundles_manifest_id.clone(),
                ..Default::default()
            });
        sidecar.name = new_name.to_string();
        write_version_sidecar(&v.install_path, &sidecar).map_err(|e| LibraryError::Io {
            path: v.install_path.clone(),
            source: std::io::Error::other(e.to_string()),
        })?;
    }
    Ok(())
}

/// Delete an install folder from disk. Does nothing if it is already gone.
pub fn delete_version_files(dir: &Path) -> Result<(), LibraryError> {
    if !dir.exists() {
        return Ok(());
    }
    if !looks_like_install(dir) {
        return Err(LibraryError::NotAnInstall(dir.to_path_buf()));
    }
    std::fs::remove_dir_all(dir).map_err(|source| LibraryError::Io {
        path: dir.to_path_buf(),
        source,
    })
}

/// Total size in bytes of everything under `dir` (links are not followed; unreadable entries are skipped).
pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

/// `20.5 GB`, `730 MB`, `12 KB`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Version;

    fn install(root: &Path, name: &str) -> PathBuf {
        let dir = root.join("RustyLauncherVersions").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("RustClient.exe"), b"exe").unwrap();
        dir
    }

    fn state_with(dir: &Path) -> State {
        let mut s = State::default();
        s.add_version(Version {
            id: "100".into(),
            name: "Old".into(),
            manifest_id: "100".into(),
            install_path: dir.to_path_buf(),
            ..Default::default()
        });
        s
    }

    #[test]
    fn rename_updates_state_and_sidecar() {
        let root = tempfile::tempdir().unwrap();
        let dir = install(root.path(), "100");
        let mut state = state_with(&dir);
        rename_version(&mut state, "100", "  July 2023  ").unwrap();
        assert_eq!(state.versions[0].name, "July 2023");
        let sc = read_version_sidecar(&dir).unwrap().unwrap();
        assert_eq!(sc.name, "July 2023");
        assert_eq!(sc.manifest_id, "100");
    }

    #[test]
    fn rename_keeps_other_sidecar_fields() {
        let root = tempfile::tempdir().unwrap();
        let dir = install(root.path(), "100");
        write_version_sidecar(
            &dir,
            &VersionSidecar {
                name: "Old".into(),
                manifest_id: "100".into(),
                bundles_manifest_id: Some("200".into()),
                depot_ids: vec!["252495".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let mut state = state_with(&dir);
        rename_version(&mut state, "100", "New").unwrap();
        let sc = read_version_sidecar(&dir).unwrap().unwrap();
        assert_eq!(sc.bundles_manifest_id.as_deref(), Some("200"));
        assert_eq!(sc.depot_ids, vec!["252495".to_string()]);
    }

    #[test]
    fn rename_rejects_empty_and_unknown() {
        let root = tempfile::tempdir().unwrap();
        let dir = install(root.path(), "100");
        let mut state = state_with(&dir);
        assert!(matches!(
            rename_version(&mut state, "100", "   "),
            Err(LibraryError::EmptyName)
        ));
        assert!(matches!(
            rename_version(&mut state, "nope", "x"),
            Err(LibraryError::NotFound)
        ));
        assert_eq!(state.versions[0].name, "Old");
    }

    #[test]
    fn rename_works_when_the_folder_is_missing() {
        let mut state = state_with(Path::new(r"Z:\gone\RustyLauncherVersions\100"));
        rename_version(&mut state, "100", "Still renamed").unwrap();
        assert_eq!(state.versions[0].name, "Still renamed");
    }

    #[test]
    fn delete_removes_an_install() {
        let root = tempfile::tempdir().unwrap();
        let dir = install(root.path(), "100");
        std::fs::create_dir_all(dir.join("Bundles").join("items")).unwrap();
        std::fs::write(dir.join("Bundles").join("items").join("a.json"), b"{}").unwrap();
        delete_version_files(&dir).unwrap();
        assert!(!dir.exists());
        assert!(
            root.path().join("RustyLauncherVersions").exists(),
            "the parent folder stays"
        );
    }

    #[test]
    fn delete_refuses_a_folder_that_is_not_an_install() {
        let root = tempfile::tempdir().unwrap();
        let other = root.path().join("MyDocuments");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("important.txt"), b"keep me").unwrap();
        assert!(matches!(
            delete_version_files(&other),
            Err(LibraryError::NotAnInstall(_))
        ));
        assert!(other.join("important.txt").exists());
    }

    #[test]
    fn delete_of_a_missing_folder_is_fine() {
        delete_version_files(Path::new(r"Z:\definitely\not\here\123")).unwrap();
    }

    #[test]
    fn delete_refuses_shallow_paths() {
        assert!(!looks_like_install(Path::new(r"C:\")));
        assert!(!looks_like_install(Path::new(r"C:\Users")));
    }

    #[test]
    fn sizes_add_up_and_format_nicely() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("a").join("b")).unwrap();
        std::fs::write(root.path().join("one"), vec![0u8; 1000]).unwrap();
        std::fs::write(root.path().join("a").join("two"), vec![0u8; 500]).unwrap();
        std::fs::write(root.path().join("a").join("b").join("three"), vec![0u8; 24]).unwrap();
        assert_eq!(dir_size(root.path()), 1524);
        assert_eq!(dir_size(&root.path().join("missing")), 0);
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(21_990_232_555), "20.5 GB");
        assert_eq!(format_bytes(765_000_000), "730 MB");
    }
}
