//! The data model. Field names are camelCase on disk to match the V2 sidecar files.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Bump when `state.json` changes shape in a way old files cannot be read.
pub const STATE_SCHEMA_VERSION: u32 = 1;

pub const CLIENT_APP: u32 = 252490;
/// Client depot with the Win64 game binaries.
pub const CLIENT_DEPOT: u32 = 252495;
/// Client "common" depot with the `Bundles` content.
pub const CLIENT_BUNDLES_DEPOT: u32 = 252494;
pub const SERVER_APP: u32 = 258550;
pub const SERVER_WIN64_DEPOT: u32 = 258551;
pub const SERVER_LINUX64_DEPOT: u32 = 258552;
pub const SERVER_COMMON_DEPOT: u32 = 258554;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub schema_version: u32,
    pub settings: Settings,
    pub install_paths: Vec<InstallPath>,
    pub versions: Vec<Version>,
    pub servers: Vec<Server>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            settings: Settings::default(),
            install_paths: Vec::new(),
            versions: Vec::new(),
            servers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Steam account name only. The password is never stored; the refresh token lives in Credential Manager.
    pub steam_username: Option<String>,
    /// Id of the last version the user played (for the quick-launch footer).
    pub last_played_version: Option<String>,
    /// Launch with Easy Anti-Cheat (`Rust.exe`) rather than `RustClient.exe`.
    pub launch_with_eac: bool,
    /// Where new downloads go when the user does not pick a folder.
    pub default_install_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            steam_username: None,
            last_played_version: None,
            launch_with_eac: true,
            default_install_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPath {
    pub path: PathBuf,
    /// Unix seconds; used for the "most recent" list.
    #[serde(default)]
    pub last_used: i64,
}

/// One installed game client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Version {
    /// Stable id: the version folder's name (the client manifest id).
    pub id: String,
    pub name: String,
    pub manifest_id: String,
    /// Recorded from V3 on; V1/V2 never stored it.
    pub bundles_manifest_id: Option<String>,
    /// The version folder itself (`...\RustyLauncherVersions\<manifest>`).
    pub install_path: PathBuf,
    /// ISO 8601 text, as written by V1/V2.
    pub download_date: Option<String>,
    pub eac_enabled: bool,
    /// Unix seconds.
    pub last_played: Option<i64>,
}

impl Default for Version {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            manifest_id: String::new(),
            bundles_manifest_id: None,
            install_path: PathBuf::new(),
            download_date: None,
            eac_enabled: true,
            last_played: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ServerInstallType {
    /// Pinned to a manifest pair (needs a Steam login to download).
    #[default]
    Pinned,
    /// Tracks the newest build (anonymous download).
    Latest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub install_type: ServerInstallType,
    pub manifest_id: Option<String>,
    pub common_manifest_id: Option<String>,
    pub install_path: PathBuf,
    pub created_at: Option<String>,
}

fn same_path(a: &Path, b: &Path) -> bool {
    // Windows paths are case-insensitive and may differ in slash direction.
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    norm(a) == norm(b)
}

impl State {
    pub fn version_by_id(&self, id: &str) -> Option<&Version> {
        self.versions.iter().find(|v| v.id == id)
    }

    pub fn has_version_at(&self, path: &Path) -> bool {
        self.versions
            .iter()
            .any(|v| same_path(&v.install_path, path))
    }

    /// Add a version unless one already exists at the same folder. Returns whether it was added.
    pub fn add_version(&mut self, v: Version) -> bool {
        if self.has_version_at(&v.install_path) {
            return false;
        }
        self.versions.push(v);
        true
    }

    pub fn remove_version(&mut self, id: &str) -> Option<Version> {
        let i = self.versions.iter().position(|v| v.id == id)?;
        let removed = self.versions.remove(i);
        if self.settings.last_played_version.as_deref() == Some(id) {
            self.settings.last_played_version = None;
        }
        Some(removed)
    }

    /// Record a launch, for the quick-launch footer and the "last played" label.
    pub fn mark_played(&mut self, id: &str, eac: bool, now_unix: i64) -> bool {
        let Some(v) = self.versions.iter_mut().find(|v| v.id == id) else {
            return false;
        };
        v.last_played = Some(now_unix);
        v.eac_enabled = eac;
        self.settings.last_played_version = Some(id.to_string());
        self.settings.launch_with_eac = eac;
        true
    }

    pub fn server_by_id(&self, id: &str) -> Option<&Server> {
        self.servers.iter().find(|s| s.id == id)
    }

    /// Add a server, or replace the entry at the same folder (an update changes its manifests).
    pub fn upsert_server(&mut self, s: Server) {
        if let Some(existing) = self
            .servers
            .iter_mut()
            .find(|x| same_path(&x.install_path, &s.install_path))
        {
            *existing = s;
        } else {
            self.servers.push(s);
        }
    }

    pub fn remove_server(&mut self, id: &str) -> Option<Server> {
        let i = self.servers.iter().position(|s| s.id == id)?;
        Some(self.servers.remove(i))
    }

    /// Add an install path or move it to the front of the recent list.
    pub fn touch_install_path(&mut self, path: PathBuf, now_unix: i64) {
        if let Some(p) = self
            .install_paths
            .iter_mut()
            .find(|p| same_path(&p.path, &path))
        {
            p.last_used = now_unix;
        } else {
            self.install_paths.push(InstallPath {
                path,
                last_used: now_unix,
            });
        }
        self.install_paths
            .sort_by_key(|p| std::cmp::Reverse(p.last_used));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, path: &str) -> Version {
        Version {
            id: id.into(),
            name: id.into(),
            manifest_id: id.into(),
            install_path: path.into(),
            ..Default::default()
        }
    }

    #[test]
    fn duplicate_paths_are_ignored_case_insensitively() {
        let mut s = State::default();
        assert!(s.add_version(v("1", r"D:\Games\RustyLauncherVersions\1")));
        assert!(!s.add_version(v("1b", r"d:/games/rustylauncherversions/1/")));
        assert_eq!(s.versions.len(), 1);
    }

    #[test]
    fn mark_played_updates_footer_state() {
        let mut s = State::default();
        s.add_version(v("a", r"D:\a"));
        assert!(s.mark_played("a", false, 42));
        assert_eq!(s.settings.last_played_version.as_deref(), Some("a"));
        assert!(!s.settings.launch_with_eac);
        assert_eq!(s.versions[0].last_played, Some(42));
        assert!(!s.mark_played("missing", true, 1));
    }

    #[test]
    fn removing_the_last_played_version_clears_the_footer() {
        let mut s = State::default();
        s.add_version(v("a", r"D:\a"));
        s.mark_played("a", true, 1);
        s.remove_version("a");
        assert_eq!(s.settings.last_played_version, None);
    }

    #[test]
    fn servers_are_added_replaced_and_removed() {
        let mut s = State::default();
        let srv = |id: &str, manifest: &str| Server {
            id: id.into(),
            name: "S".into(),
            manifest_id: Some(manifest.into()),
            install_path: format!("D:/RustServers/{id}").into(),
            ..Default::default()
        };
        s.upsert_server(srv("a", "1"));
        s.upsert_server(srv("a", "2")); // an update: same folder, new manifest
        assert_eq!(s.servers.len(), 1);
        assert_eq!(
            s.server_by_id("a").unwrap().manifest_id.as_deref(),
            Some("2")
        );
        s.upsert_server(srv("b", "3"));
        assert!(s.remove_server("a").is_some());
        assert!(s.remove_server("a").is_none());
        assert_eq!(s.servers.len(), 1);
    }

    #[test]
    fn install_paths_sort_most_recent_first() {
        let mut s = State::default();
        s.touch_install_path(r"D:\one".into(), 10);
        s.touch_install_path(r"E:\two".into(), 20);
        s.touch_install_path(r"d:\ONE".into(), 30);
        assert_eq!(s.install_paths.len(), 2);
        assert_eq!(s.install_paths[0].last_used, 30);
    }
}
