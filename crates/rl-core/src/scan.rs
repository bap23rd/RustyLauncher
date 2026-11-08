//! Rebuilding the library by scanning install folders (the sidecars are the source of truth).

use crate::error::{Error, Result};
use crate::model::{State, Version};
use crate::paths::{ARCHIVE_DIR_NAME, TEMP_BUNDLES_DIR_NAME, versions_dir};
use crate::sidecar::read_version_sidecar;
use std::path::Path;

/// File that marks a folder as an installed game client.
const CLIENT_EXE: &str = "RustClient.exe";

/// Read one install folder as a version, or `None` if it is not an install (no game executable).
/// A damaged sidecar does not hide the install: it is listed under a generated name.
pub fn version_from_dir(dir: &Path) -> Option<Version> {
    if !dir.join(CLIENT_EXE).is_file() {
        return None;
    }
    let folder = dir.file_name()?.to_string_lossy().into_owned();
    let sidecar = match read_version_sidecar(dir) {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(%err, "ignoring damaged version sidecar");
            None
        }
    };
    let (name, manifest_id, download_date, bundles) = match sidecar {
        Some(s) => (
            s.name,
            if s.manifest_id.is_empty() {
                folder.clone()
            } else {
                s.manifest_id
            },
            s.download_date,
            s.bundles_manifest_id,
        ),
        None => (String::new(), folder.clone(), None, None),
    };
    let name = if name.trim().is_empty() {
        let short: String = folder.chars().take(16).collect();
        format!("Discovered: {short}...")
    } else {
        name
    };
    Some(Version {
        id: folder,
        name,
        manifest_id,
        bundles_manifest_id: bundles,
        install_path: dir.to_path_buf(),
        download_date,
        eac_enabled: true,
        last_played: None,
    })
}

/// Find installed game versions under one install path. Never touches the disk beyond reading.
pub fn scan_versions(install_path: &Path) -> Result<Vec<Version>> {
    let base = versions_dir(install_path);
    let entries = match std::fs::read_dir(&base) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::io(&base, e)),
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        // `archive` and `tempBundles` are ours; dot-folders are half-finished restores.
        if folder.starts_with('.')
            || folder.eq_ignore_ascii_case(ARCHIVE_DIR_NAME)
            || folder.eq_ignore_ascii_case(TEMP_BUNDLES_DIR_NAME)
        {
            continue;
        }
        if let Some(v) = version_from_dir(&entry.path()) {
            found.push(v);
        }
    }
    found.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(found)
}

/// Add scanned versions that the state does not know yet. Returns how many were added.
pub fn merge_versions(state: &mut State, found: Vec<Version>) -> usize {
    found
        .into_iter()
        .filter(|v| state.add_version(v.clone()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::{VersionSidecar, write_version_sidecar};

    fn make_install(base: &Path, folder: &str, with_exe: bool, sidecar: Option<VersionSidecar>) {
        let dir = base.join("RustyLauncherVersions").join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        if with_exe {
            std::fs::write(dir.join("RustClient.exe"), b"x").unwrap();
        }
        if let Some(s) = sidecar {
            write_version_sidecar(&dir, &s).unwrap();
        }
    }

    #[test]
    fn finds_installs_and_skips_other_folders() {
        let root = tempfile::tempdir().unwrap();
        make_install(
            root.path(),
            "2486676083669216181",
            true,
            Some(VersionSidecar {
                name: "Rust Stable July 2023".into(),
                manifest_id: "2486676083669216181".into(),
                download_date: Some("2026-01-01T00:00:00Z".into()),
                bundles_manifest_id: Some("3732538865539399361".into()),
                depot_ids: vec![],
            }),
        );
        make_install(root.path(), "1234567890123456789012345", true, None); // no sidecar
        make_install(root.path(), "9999", false, None); // not an install (no exe)
        make_install(root.path(), "archive", true, None);
        make_install(root.path(), "tempBundles", true, None);

        let found = scan_versions(root.path()).unwrap();
        assert_eq!(found.len(), 2);
        let named = found
            .iter()
            .find(|v| v.name == "Rust Stable July 2023")
            .unwrap();
        assert_eq!(
            named.bundles_manifest_id.as_deref(),
            Some("3732538865539399361")
        );
        let unnamed = found
            .iter()
            .find(|v| v.id == "1234567890123456789012345")
            .unwrap();
        assert_eq!(unnamed.name, "Discovered: 1234567890123456...");
    }

    #[test]
    fn damaged_sidecar_still_lists_the_install() {
        let root = tempfile::tempdir().unwrap();
        make_install(root.path(), "42", true, None);
        std::fs::write(
            root.path()
                .join("RustyLauncherVersions")
                .join("42")
                .join(".rustylauncher.json"),
            "garbage",
        )
        .unwrap();
        let found = scan_versions(root.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Discovered: 42...");
    }

    #[test]
    fn missing_install_path_is_empty_not_an_error() {
        let root = tempfile::tempdir().unwrap();
        assert!(scan_versions(&root.path().join("nope")).unwrap().is_empty());
    }

    #[test]
    fn accepts_the_versions_folder_itself_and_merge_does_not_duplicate() {
        let root = tempfile::tempdir().unwrap();
        make_install(root.path(), "7", true, None);
        let direct = root.path().join("RustyLauncherVersions");
        let mut state = State::default();
        assert_eq!(
            merge_versions(&mut state, scan_versions(&direct).unwrap()),
            1
        );
        assert_eq!(
            merge_versions(&mut state, scan_versions(root.path()).unwrap()),
            0
        );
        assert_eq!(state.versions.len(), 1);
    }
}
