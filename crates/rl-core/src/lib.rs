//! RustyLauncher core: everything that is not UI, Steam or server control.
//!
//! * [`paths`]   where things live on disk
//! * [`model`]   the data types (versions, servers, settings)
//! * [`state`]   `state.json` load and atomic save
//! * [`sidecar`] the per-install `.rustylauncher.json` files (the source of truth)
//! * [`scan`]    rebuilding the library by scanning install folders
//! * [`launch`]  starting the game
//! * [`library`] rename, delete and size of installed versions
//! * [`archive`] zipping a version up and restoring it
//! * [`curated`] the curated build lists and their cache
//! * [`logging`] rolling log file setup
//! * [`secret`]  a wrapper that never prints its contents

pub mod archive;
pub mod curated;
pub mod error;
mod fsutil;
pub mod launch;
pub mod library;
pub mod logging;
pub mod model;
pub mod paths;
pub mod scan;
pub mod secret;
pub mod sidecar;
pub mod state;

pub use error::{Error, Result};
