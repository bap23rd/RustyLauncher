//! Bookkeeping for downloads in progress.
//!
//! While a build is downloading, its folder holds a marker file. That keeps a half-finished
//! download out of the library (the game executable appears early in the download) and lets the
//! launcher offer to resume it after a crash, a cancel or a lost connection.

use crate::error::{Error, Result};
use crate::fsutil::write_atomic;
use crate::model::{CLIENT_BUNDLES_DEPOT, CLIENT_DEPOT};
use crate::paths::{ARCHIVE_DIR_NAME, TEMP_BUNDLES_DIR_NAME, versions_dir};
use crate::sidecar::{VersionSidecar, write_version_sidecar};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const INSTALL_MARKER: &str = ".rustylauncher-installing.json";

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallMarker {
    pub name: String,
    pub client_manifest: String,
    pub bundles_manifest: String,
}

pub fn write_marker(dir: &Path, marker: &InstallMarker) -> Result<()> {
    let file = dir.join(INSTALL_MARKER);
    let json = serde_json::to_vec_pretty(marker).map_err(|e| Error::Json {
        path: file.clone(),
        source: e,
    })?;
    write_atomic(&file, &json)
}

pub fn read_marker(dir: &Path) -> Option<InstallMarker> {
    let text = std::fs::read_to_string(dir.join(INSTALL_MARKER)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn has_marker(dir: &Path) -> bool {
    dir.join(INSTALL_MARKER).exists()
}

/// A download finished: write the sidecar (the source of truth for the version's name and manifests)
/// and remove the marker, which makes the folder an ordinary installed version.
pub fn finish_install(dir: &Path, marker: &InstallMarker, download_date_iso: &str) -> Result<()> {
    write_version_sidecar(
        dir,
        &VersionSidecar {
            name: marker.name.clone(),
            manifest_id: marker.client_manifest.clone(),
            download_date: Some(download_date_iso.to_string()),
            bundles_manifest_id: Some(marker.bundles_manifest.clone()).filter(|s| !s.is_empty()),
            depot_ids: vec![CLIENT_DEPOT.to_string(), CLIENT_BUNDLES_DEPOT.to_string()],
        },
    )?;
    std::fs::remove_file(dir.join(INSTALL_MARKER)).map_err(|e| Error::Io {
        path: dir.join(INSTALL_MARKER),
        source: e,
    })
}

/// Where a build's files go: `<install path>\RustyLauncherVersions\<client manifest>`.
pub fn install_dir(install_path: &Path, client_manifest: &str) -> PathBuf {
    versions_dir(install_path).join(client_manifest)
}

/// Is this a manifest id (digits only)? Anything else must never become a folder name.
pub fn valid_manifest_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}

/// Downloads that were started but not finished, found in the given install paths.
pub fn find_unfinished(install_paths: &[PathBuf]) -> Vec<(PathBuf, InstallMarker)> {
    let mut out = Vec::new();
    for base in install_paths {
        let Ok(entries) = std::fs::read_dir(versions_dir(base)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.')
                || name.eq_ignore_ascii_case(ARCHIVE_DIR_NAME)
                || name.eq_ignore_ascii_case(TEMP_BUNDLES_DIR_NAME)
            {
                continue;
            }
            if let Some(marker) = read_marker(&entry.path()) {
                out.push((entry.path(), marker));
            }
        }
    }
    out.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
    out
}

/// Free space, in bytes, on the drive that holds `path` (or its nearest existing parent).
#[cfg(windows)]
pub fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut probe = path;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    let wide: Vec<u16> = probe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available: u64 = 0;
    // SAFETY: `wide` is a NUL-terminated path; the out-pointers are valid for the call.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(available)
}

#[cfg(not(windows))]
pub fn free_space(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_round_trip_and_finish() {
        let dir = tempfile::tempdir().unwrap();
        let m = InstallMarker {
            name: "July 2023".into(),
            client_manifest: "2486676083669216181".into(),
            bundles_manifest: "3732538865539399361".into(),
        };
        write_marker(dir.path(), &m).unwrap();
        assert!(has_marker(dir.path()));
        assert_eq!(read_marker(dir.path()).unwrap(), m);

        finish_install(dir.path(), &m, "2026-11-09T00:00:00Z").unwrap();
        assert!(!has_marker(dir.path()));
        let sc = crate::sidecar::read_version_sidecar(dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(sc.name, "July 2023");
        assert_eq!(
            sc.bundles_manifest_id.as_deref(),
            Some("3732538865539399361")
        );
        assert_eq!(
            sc.depot_ids,
            vec!["252495".to_string(), "252494".to_string()]
        );
    }

    #[test]
    fn only_digits_are_valid_manifest_ids() {
        assert!(valid_manifest_id("2486676083669216181"));
        assert!(!valid_manifest_id(""));
        assert!(!valid_manifest_id("12 34"));
        assert!(!valid_manifest_id(r"..\..\evil"));
        assert!(!valid_manifest_id("123456789012345678901"));
    }

    #[test]
    fn finds_unfinished_downloads_and_ignores_finished_ones() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().to_path_buf();
        let done = install_dir(&base, "1");
        let partial = install_dir(&base, "2");
        std::fs::create_dir_all(&done).unwrap();
        std::fs::create_dir_all(&partial).unwrap();
        write_marker(
            &partial,
            &InstallMarker {
                name: "Half".into(),
                client_manifest: "2".into(),
                bundles_manifest: "3".into(),
            },
        )
        .unwrap();
        let found = find_unfinished(&[base]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1.name, "Half");
        assert_eq!(found[0].0, partial);
    }

    #[test]
    fn install_dir_uses_the_versions_folder() {
        assert_eq!(
            install_dir(Path::new(r"D:\Games"), "55"),
            Path::new(r"D:\Games\RustyLauncherVersions\55")
        );
    }

    #[cfg(windows)]
    #[test]
    fn free_space_of_the_temp_drive_is_known() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_space(dir.path()).unwrap() > 0);
        // A folder that does not exist yet uses its nearest existing parent.
        assert!(free_space(&dir.path().join("not").join("there")).unwrap() > 0);
    }
}
