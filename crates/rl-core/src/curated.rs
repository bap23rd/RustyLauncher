//! Curated build lists (`versionRL.json`, `versionOther.json`).
//!
//! The files live in the GitHub repository and are fetched in the background. A cached copy is
//! shown straight away, so the Download page never waits on the network. V1 and V2 read the same
//! files from `main`, so their format must not change.

use crate::error::{Error, Result};
use crate::fsutil::write_atomic;
use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://raw.githubusercontent.com/bap23rd/RustyLauncher/main/resources/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CuratedKind {
    /// "Recommended" builds (`versionRL.json`, key `recommended`).
    Recommended,
    /// Everything else (`versionOther.json`, key `other`).
    Other,
}

impl CuratedKind {
    pub fn file_name(self) -> &'static str {
        match self {
            CuratedKind::Recommended => "versionRL.json",
            CuratedKind::Other => "versionOther.json",
        }
    }
    pub fn url(self) -> String {
        format!("{BASE_URL}{}", self.file_name())
    }
    fn key(self) -> &'static str {
        match self {
            CuratedKind::Recommended => "recommended",
            CuratedKind::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CuratedBuild {
    pub title: String,
    pub client_manifest: String,
    pub bundle_manifest: String,
    /// Shown as written in the file (`dd-mm-yyyy`).
    pub version_date: String,
}

/// Read the list out of the JSON text. Entries without a manifest id are skipped.
pub fn parse(kind: CuratedKind, json: &str) -> Result<Vec<CuratedBuild>> {
    let path = std::path::PathBuf::from(kind.file_name());
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| Error::Json {
        path: path.clone(),
        source: e,
    })?;
    let list = value
        .get(kind.key())
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let builds: Vec<CuratedBuild> =
        serde_json::from_value(list).map_err(|e| Error::Json { path, source: e })?;
    Ok(builds
        .into_iter()
        .filter(|b| !b.client_manifest.trim().is_empty())
        .collect())
}

/// The cached list, or an empty one if there is no (usable) cache yet.
pub fn load_cached(paths: &AppPaths, kind: CuratedKind) -> Vec<CuratedBuild> {
    std::fs::read_to_string(paths.cache_dir().join(kind.file_name()))
        .ok()
        .and_then(|text| parse(kind, &text).ok())
        .unwrap_or_default()
}

/// Validate freshly fetched JSON and, if it is good, store it as the new cache.
pub fn store_fetched(paths: &AppPaths, kind: CuratedKind, json: &str) -> Result<Vec<CuratedBuild>> {
    let builds = parse(kind, json)?;
    write_atomic(&paths.cache_dir().join(kind.file_name()), json.as_bytes())?;
    Ok(builds)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RL: &str = r#"{ "recommended": [
        { "title": "Rust Stable July 2023", "clientManifest": "2486676083669216181", "bundleManifest": "3732538865539399361", "versionDate": "09-07-2023" },
        { "title": "No manifest", "clientManifest": "", "bundleManifest": "1", "versionDate": "" }
    ] }"#;

    #[test]
    fn parses_the_real_file_format() {
        let list = parse(CuratedKind::Recommended, RL).unwrap();
        assert_eq!(list.len(), 1, "entries without a manifest are skipped");
        assert_eq!(list[0].title, "Rust Stable July 2023");
        assert_eq!(list[0].bundle_manifest, "3732538865539399361");
    }

    #[test]
    fn the_other_file_uses_a_different_key() {
        let json = r#"{ "other": [ { "title": "Old", "clientManifest": "5", "bundleManifest": "6", "versionDate": "27-01-2018" } ] }"#;
        assert_eq!(parse(CuratedKind::Other, json).unwrap().len(), 1);
        assert!(parse(CuratedKind::Recommended, json).unwrap().is_empty());
    }

    #[test]
    fn the_files_in_this_repository_parse() {
        let rl = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../resources/versionRL.json"
        ))
        .unwrap();
        let other = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../resources/versionOther.json"
        ))
        .unwrap();
        assert!(!parse(CuratedKind::Recommended, &rl).unwrap().is_empty());
        assert!(!parse(CuratedKind::Other, &other).unwrap().is_empty());
    }

    #[test]
    fn cache_round_trip_and_bad_data_is_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        assert!(load_cached(&paths, CuratedKind::Recommended).is_empty());
        store_fetched(&paths, CuratedKind::Recommended, RL).unwrap();
        assert_eq!(load_cached(&paths, CuratedKind::Recommended).len(), 1);
        assert!(
            store_fetched(
                &paths,
                CuratedKind::Recommended,
                "<html>rate limited</html>"
            )
            .is_err()
        );
        assert_eq!(
            load_cached(&paths, CuratedKind::Recommended).len(),
            1,
            "a bad download must not replace a good cache"
        );
    }

    #[test]
    fn url_points_at_main() {
        assert_eq!(
            CuratedKind::Recommended.url(),
            "https://raw.githubusercontent.com/bap23rd/RustyLauncher/main/resources/versionRL.json"
        );
    }
}

// ---------------------------------------------------------------- server builds

/// A curated dedicated-server build (`versionServers.json`, key `servers`). This is a new file:
/// V1 and V2 never read it, so it can use its own field names.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CuratedServerBuild {
    pub title: String,
    /// Manifest of the win64 server depot (258551).
    pub server_manifest: String,
    /// Manifest of the shared content depot (258554).
    pub server_common_manifest: String,
    pub version_date: String,
}

pub const SERVERS_FILE: &str = "versionServers.json";

/// The list shipped inside the program, used until a newer one has been fetched.
pub const BUNDLED_SERVERS: &str = include_str!("../../../resources/versionServers.json");

pub fn servers_url() -> String {
    format!("{BASE_URL}{SERVERS_FILE}")
}

pub fn parse_servers(json: &str) -> Result<Vec<CuratedServerBuild>> {
    let path = std::path::PathBuf::from(SERVERS_FILE);
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| Error::Json {
        path: path.clone(),
        source: e,
    })?;
    let list = value
        .get("servers")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let builds: Vec<CuratedServerBuild> =
        serde_json::from_value(list).map_err(|e| Error::Json { path, source: e })?;
    Ok(builds
        .into_iter()
        .filter(|b| {
            !b.server_manifest.trim().is_empty() && !b.server_common_manifest.trim().is_empty()
        })
        .collect())
}

/// The cached list if there is one, else the bundled one.
pub fn load_servers(paths: &AppPaths) -> Vec<CuratedServerBuild> {
    std::fs::read_to_string(paths.cache_dir().join(SERVERS_FILE))
        .ok()
        .and_then(|t| parse_servers(&t).ok())
        .filter(|l| !l.is_empty())
        .or_else(|| parse_servers(BUNDLED_SERVERS).ok())
        .unwrap_or_default()
}

pub fn store_fetched_servers(paths: &AppPaths, json: &str) -> Result<Vec<CuratedServerBuild>> {
    let builds = parse_servers(json)?;
    write_atomic(&paths.cache_dir().join(SERVERS_FILE), json.as_bytes())?;
    Ok(builds)
}

#[cfg(test)]
mod server_tests {
    use super::*;

    #[test]
    fn the_bundled_server_list_parses_and_has_a_build() {
        let list = parse_servers(BUNDLED_SERVERS).unwrap();
        assert!(!list.is_empty());
        assert!(list[0].server_manifest.bytes().all(|b| b.is_ascii_digit()));
        assert!(
            list[0]
                .server_common_manifest
                .bytes()
                .all(|b| b.is_ascii_digit())
        );
    }

    #[test]
    fn incomplete_entries_are_skipped_and_the_cache_wins_over_the_bundle() {
        let json = r#"{ "servers": [
            { "title": "ok", "serverManifest": "1", "serverCommonManifest": "2", "versionDate": "x" },
            { "title": "missing common", "serverManifest": "1" }
        ] }"#;
        assert_eq!(parse_servers(json).unwrap().len(), 1);
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        assert_eq!(
            load_servers(&paths)[0].title,
            parse_servers(BUNDLED_SERVERS).unwrap()[0].title
        );
        store_fetched_servers(&paths, json).unwrap();
        assert_eq!(load_servers(&paths)[0].title, "ok");
        assert!(store_fetched_servers(&paths, "<html>").is_err());
        assert_eq!(
            load_servers(&paths)[0].title,
            "ok",
            "a bad download must not replace a good cache"
        );
    }
}
