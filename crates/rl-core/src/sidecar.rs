//! Per-install sidecar files. They sit inside each install folder and are the source of truth:
//! if `state.json` is lost, the library is rebuilt from them.
//!
//! The V1/V2 format `{ name, manifestId, downloadDate }` is kept and extended with optional fields,
//! so old installs still read, and V3 installs still open in V1/V2.

use crate::error::{Error, Result};
use crate::fsutil::write_atomic;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const VERSION_SIDECAR: &str = ".rustylauncher.json";
pub const SERVER_SIDECAR: &str = ".rustyserver.json";

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VersionSidecar {
    pub name: String,
    pub manifest_id: String,
    pub download_date: Option<String>,
    /// New in V3: the bundles depot manifest this install was built from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundles_manifest_id: Option<String>,
    /// New in V3: depot ids that were downloaded into this folder.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub depot_ids: Vec<String>,
}

/// Read a version sidecar. `Ok(None)` means there is no sidecar; a damaged one is an error.
pub fn read_version_sidecar(version_dir: &Path) -> Result<Option<VersionSidecar>> {
    let file = version_dir.join(VERSION_SIDECAR);
    match std::fs::read_to_string(&file) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| Error::json(&file, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(&file, e)),
    }
}

pub fn write_version_sidecar(version_dir: &Path, sidecar: &VersionSidecar) -> Result<()> {
    let file = version_dir.join(VERSION_SIDECAR);
    let json = serde_json::to_vec_pretty(sidecar).map_err(|e| Error::json(&file, e))?;
    write_atomic(&file, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_v2_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(VERSION_SIDECAR),
            r#"{ "name": "Rust Stable July 2023", "manifestId": "2486676083669216181", "downloadDate": "2025-01-02T03:04:05.000Z" }"#,
        )
        .unwrap();
        let s = read_version_sidecar(dir.path()).unwrap().unwrap();
        assert_eq!(s.name, "Rust Stable July 2023");
        assert_eq!(s.manifest_id, "2486676083669216181");
        assert_eq!(s.bundles_manifest_id, None);
    }

    #[test]
    fn missing_sidecar_is_none_and_damaged_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_version_sidecar(dir.path()).unwrap().is_none());
        std::fs::write(dir.path().join(VERSION_SIDECAR), "nope").unwrap();
        assert!(read_version_sidecar(dir.path()).is_err());
    }

    #[test]
    fn v3_fields_round_trip_and_v2_readers_still_work() {
        let dir = tempfile::tempdir().unwrap();
        let s = VersionSidecar {
            name: "x".into(),
            manifest_id: "1".into(),
            download_date: Some("2026-11-08T00:00:00Z".into()),
            bundles_manifest_id: Some("2".into()),
            depot_ids: vec!["252495".into(), "252494".into()],
        };
        write_version_sidecar(dir.path(), &s).unwrap();
        assert_eq!(read_version_sidecar(dir.path()).unwrap().unwrap(), s);
        // V1/V2 only look at name / manifestId / downloadDate, which are still present.
        let raw: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(VERSION_SIDECAR)).unwrap(),
        )
        .unwrap();
        assert_eq!(raw["name"], "x");
        assert_eq!(raw["manifestId"], "1");
        assert_eq!(raw["downloadDate"], "2026-11-08T00:00:00Z");
    }
}
