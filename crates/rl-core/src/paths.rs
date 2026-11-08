//! Where RustyLauncher keeps its own files.
//!
//! Same folder as V1/V2 (`%APPDATA%\RustyLauncher`) so the legacy importer can find the old databases.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// Folder name under the user's roaming app data.
pub const APP_DIR_NAME: &str = "RustyLauncher";
/// Subfolder of an install path that holds game versions (V1/V2 convention, kept so old installs are found).
pub const VERSIONS_DIR_NAME: &str = "RustyLauncherVersions";
/// Subfolder of an install path that holds dedicated servers (V2 convention).
pub const SERVERS_DIR_NAME: &str = "RustServers";
/// Folder (inside the versions folder) for archived versions.
pub const ARCHIVE_DIR_NAME: &str = "archive";
/// Scratch folder V1/V2 used while merging bundles. V3 never creates it, but it must be skipped when scanning.
pub const TEMP_BUNDLES_DIR_NAME: &str = "tempBundles";

#[derive(Debug, Clone)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    /// The real location: `%APPDATA%\RustyLauncher`.
    pub fn default_location() -> Result<Self> {
        let base = dirs::config_dir().ok_or(Error::NoDataDir)?;
        Ok(Self::at(base.join(APP_DIR_NAME)))
    }

    /// A custom root (used by tests).
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn state_file(&self) -> PathBuf {
        self.root.join("state.json")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
    /// Cached curated version lists downloaded from GitHub.
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }
    /// V1/V2 database files, read once by the legacy importer.
    pub fn legacy_versions_db(&self) -> PathBuf {
        self.root.join("rusty_launcher.db")
    }
    pub fn legacy_settings_db(&self) -> PathBuf {
        self.root.join("settings.db")
    }
}

/// Ensure a game install path ends with `RustyLauncherVersions`, as V1/V2 did.
pub fn versions_dir(install_path: &Path) -> PathBuf {
    if install_path
        .file_name()
        .is_some_and(|n| n == VERSIONS_DIR_NAME)
    {
        install_path.to_path_buf()
    } else {
        install_path.join(VERSIONS_DIR_NAME)
    }
}

/// Ensure a server install path ends with `RustServers`.
pub fn servers_dir(install_path: &Path) -> PathBuf {
    if install_path
        .file_name()
        .is_some_and(|n| n == SERVERS_DIR_NAME)
    {
        install_path.to_path_buf()
    } else {
        install_path.join(SERVERS_DIR_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_dir_is_appended_once() {
        let base = Path::new(r"D:\Games");
        assert_eq!(versions_dir(base), base.join("RustyLauncherVersions"));
        assert_eq!(
            versions_dir(&versions_dir(base)),
            base.join("RustyLauncherVersions")
        );
    }

    #[test]
    fn servers_dir_is_appended_once() {
        let base = Path::new(r"D:\Games");
        assert_eq!(servers_dir(&servers_dir(base)), base.join("RustServers"));
    }
}
