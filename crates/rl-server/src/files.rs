//! Files a server install carries: the settings file, the `.bat` that starts it, and Oxide.
//!
//! `.rustyserver.json` keeps V2's launch settings at its top level (so V2 still reads it) and adds an
//! `install` section with the server's name, type and manifests.

use crate::config::ServerLaunchConfig;
use rl_core::error::{Error, Result};
use rl_core::fsutil::write_atomic;
use rl_core::model::{Server, ServerInstallType};
use rl_core::paths::servers_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SERVER_FILE: &str = ".rustyserver.json";
pub const BAT_FILE: &str = "RustServer.bat";
pub const SERVER_EXE: &str = "RustDedicated.exe";
/// Where to get Oxide (uMod).
pub const UMOD_URL: &str = "https://umod.org/games/rust";

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallInfo {
    pub name: String,
    pub install_type: ServerInstallType,
    /// The win64 server depot's manifest (258551).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_id: Option<String>,
    /// The shared content depot's manifest (258554).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_manifest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerFile {
    #[serde(flatten)]
    pub config: ServerLaunchConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<InstallInfo>,
}

fn json_err(path: &Path, e: serde_json::Error) -> Error {
    Error::Json {
        path: path.to_path_buf(),
        source: e,
    }
}

/// `Ok(None)` if the server has no settings file yet; a damaged one is an error.
pub fn read_server_file(dir: &Path) -> Result<Option<ServerFile>> {
    let file = dir.join(SERVER_FILE);
    match std::fs::read_to_string(&file) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| json_err(&file, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io {
            path: file,
            source: e,
        }),
    }
}

/// The text of `RustServer.bat`. A `%` in a value would be read as a variable by the batch
/// processor, so it is doubled.
pub fn bat_contents(config: &ServerLaunchConfig) -> String {
    let args = config.args(true).join(" ").replace('%', "%%");
    format!("@echo off\r\ncd /d \"%~dp0\"\r\n{SERVER_EXE} {args}\r\npause\r\n")
}

/// Save the settings file and regenerate `RustServer.bat`, so the server can still be started
/// without the launcher.
pub fn write_server_file(dir: &Path, file: &ServerFile) -> Result<()> {
    let path = dir.join(SERVER_FILE);
    let json = serde_json::to_vec_pretty(file).map_err(|e| json_err(&path, e))?;
    write_atomic(&path, &json)?;
    write_atomic(&dir.join(BAT_FILE), bat_contents(&file.config).as_bytes())
}

/// Change the launch settings, keeping the install section.
pub fn save_config(dir: &Path, config: ServerLaunchConfig) -> Result<()> {
    let install = read_server_file(dir).ok().flatten().and_then(|f| f.install);
    write_server_file(dir, &ServerFile { config, install })
}

/// Rename a server (in its settings file, which is the source of truth for the name).
pub fn rename(dir: &Path, new_name: &str) -> Result<()> {
    let mut file = read_server_file(dir)?.unwrap_or_default();
    let mut install = file.install.take().unwrap_or_default();
    install.name = new_name.trim().to_string();
    file.install = Some(install);
    write_server_file(dir, &file)
}

/// Oxide drops an `oxide` folder next to the server executable once installed.
pub fn has_oxide(dir: &Path) -> bool {
    dir.join("oxide").is_dir()
}

pub fn mods_dir(dir: &Path) -> PathBuf {
    dir.join("oxide")
}

/// Does this look like a server install? Used to keep delete from touching other folders.
pub fn looks_like_server(dir: &Path) -> bool {
    dir.components().count() >= 3
        && dir.is_dir()
        && (dir.join(SERVER_EXE).is_file() || dir.join(SERVER_FILE).is_file())
}

/// One server folder as a library entry, or `None` if it is not a server.
pub fn server_from_dir(dir: &Path) -> Option<Server> {
    if !dir.join(SERVER_EXE).is_file() || rl_core::install::has_marker(dir) {
        return None;
    }
    let folder = dir.file_name()?.to_string_lossy().into_owned();
    let info = read_server_file(dir).ok().flatten().and_then(|f| f.install);
    let short: String = folder.chars().take(8).collect();
    Some(match info {
        Some(i) => Server {
            id: folder.clone(),
            name: if i.name.trim().is_empty() {
                format!("Server {short}")
            } else {
                i.name
            },
            install_type: i.install_type,
            manifest_id: i.manifest_id.or_else(|| Some(folder.clone())),
            common_manifest_id: i.common_manifest_id,
            install_path: dir.to_path_buf(),
            created_at: i.created_at,
        },
        None => Server {
            id: folder.clone(),
            name: format!("Server {short}"),
            install_type: ServerInstallType::Pinned,
            manifest_id: Some(folder),
            common_manifest_id: None,
            install_path: dir.to_path_buf(),
            created_at: None,
        },
    })
}

/// Servers found under one install path (`<path>\RustServers`).
pub fn scan_servers(install_path: &Path) -> Vec<Server> {
    let Ok(entries) = std::fs::read_dir(servers_dir(install_path)) else {
        return Vec::new();
    };
    let mut found: Vec<Server> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| server_from_dir(&e.path()))
        .collect();
    found.sort_by_key(|s| s.name.to_lowercase());
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server(root: &Path, folder: &str) -> PathBuf {
        let dir = root.join("RustServers").join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SERVER_EXE), b"exe").unwrap();
        dir
    }

    #[test]
    fn bat_file_has_the_v2_layout_and_crlf_line_endings() {
        let bat = bat_contents(&ServerLaunchConfig::default());
        assert!(bat.starts_with(
            "@echo off\r\ncd /d \"%~dp0\"\r\nRustDedicated.exe -batchmode +server.secure 1 "
        ));
        assert!(bat.ends_with("\r\npause\r\n"));
        assert_eq!(bat.matches("\r\n").count(), 4);
    }

    #[test]
    fn percent_signs_are_doubled_in_the_bat_only() {
        let mut c = ServerLaunchConfig::default();
        c.hostname.value = "100% fun".into();
        assert!(bat_contents(&c).contains("\"100%% fun\""));
        assert!(c.args(false).contains(&"100% fun".to_string()));
    }

    #[test]
    fn saving_writes_both_files_and_keeps_the_install_info() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_server(root.path(), "111");
        write_server_file(
            &dir,
            &ServerFile {
                config: ServerLaunchConfig::default(),
                install: Some(InstallInfo {
                    name: "Weekend".into(),
                    install_type: ServerInstallType::Latest,
                    manifest_id: Some("111".into()),
                    common_manifest_id: Some("222".into()),
                    created_at: Some("2026-11-09T00:00:00Z".into()),
                }),
            },
        )
        .unwrap();
        assert!(dir.join(BAT_FILE).is_file());

        let mut changed = ServerLaunchConfig::default();
        changed.port.value = "30000".into();
        save_config(&dir, changed).unwrap();
        let back = read_server_file(&dir).unwrap().unwrap();
        assert_eq!(back.config.port.value, "30000");
        assert_eq!(back.install.unwrap().name, "Weekend");
        assert!(
            std::fs::read_to_string(dir.join(BAT_FILE))
                .unwrap()
                .contains("+server.port 30000")
        );
    }

    #[test]
    fn the_file_still_reads_as_a_plain_v2_settings_file() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_server(root.path(), "1");
        rename(&dir, "Named").unwrap();
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(SERVER_FILE)).unwrap()).unwrap();
        // V2 reads these top-level keys and ignores `install`.
        assert_eq!(raw["hostname"]["value"], "My Rust Server");
        assert_eq!(raw["rconWeb"]["enabled"], true);
        assert_eq!(raw["install"]["name"], "Named");
    }

    #[test]
    fn scanning_finds_servers_with_and_without_a_settings_file() {
        let root = tempfile::tempdir().unwrap();
        let named = make_server(root.path(), "754785054187022296");
        rename(&named, "My Server").unwrap();
        make_server(root.path(), "8417394343345843224"); // a V2 server: no install info
        std::fs::create_dir_all(root.path().join("RustServers").join("empty")).unwrap(); // no exe
        let half = make_server(root.path(), "999");
        rl_core::install::write_marker(&half, &rl_core::install::InstallMarker::default()).unwrap();

        let found = scan_servers(root.path());
        assert_eq!(
            found.len(),
            2,
            "the folder without the exe and the unfinished download are skipped"
        );
        let n = found.iter().find(|s| s.name == "My Server").unwrap();
        assert_eq!(n.id, "754785054187022296");
        let v2 = found
            .iter()
            .find(|s| s.id == "8417394343345843224")
            .unwrap();
        assert_eq!(v2.name, "Server 84173943");
        assert_eq!(v2.install_type, ServerInstallType::Pinned);
    }

    #[test]
    fn oxide_is_detected_by_its_folder() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_server(root.path(), "1");
        assert!(!has_oxide(&dir));
        std::fs::create_dir_all(dir.join("oxide").join("plugins")).unwrap();
        assert!(has_oxide(&dir));
        assert_eq!(mods_dir(&dir), dir.join("oxide"));
    }

    #[test]
    fn only_server_folders_look_like_servers() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_server(root.path(), "1");
        assert!(looks_like_server(&dir));
        let other = root.path().join("Documents");
        std::fs::create_dir_all(&other).unwrap();
        assert!(!looks_like_server(&other));
    }
}
