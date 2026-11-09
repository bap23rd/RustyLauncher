//! Dedicated server support: launch settings, the `.bat` file, scanning for installs and Oxide detection.
//!
//! Running the server process and talking RCON are added by the Active Server work (Phase 6).

pub mod config;
pub mod files;

pub use config::{ConfigProblem, FlagOption, ServerLaunchConfig, TextOption, random_password};
pub use files::{
    BAT_FILE, InstallInfo, SERVER_EXE, SERVER_FILE, ServerFile, UMOD_URL, bat_contents, has_oxide,
    looks_like_server, mods_dir, read_server_file, rename, save_config, scan_servers,
    server_from_dir, write_server_file,
};
