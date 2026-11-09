//! Steam access for RustyLauncher: signing in, keeping the login token safe, and downloading depots.
//!
//! This crate is the only place that talks to the `steamroom` library. The app sees only the
//! types exported here, so if Steam changes its protocol or `steamroom` is replaced, the change
//! stays inside this crate.

mod download;
mod engine;
mod error;
mod signin;
mod tokens;

pub use download::{Auth, DepotSpec, DownloadEvent, DownloadRequest, DownloadSummary};
pub use engine::{DownloadHandle, Engine, latest_manifests};
pub use error::{Failure, FailureKind};
pub use signin::{GuardKind, SignInEvent, SignInHandle};
pub use tokens::{CredentialManager, MemoryStore, TokenStore};
