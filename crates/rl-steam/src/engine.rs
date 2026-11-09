//! The entry point the app uses. It owns a small async runtime, started the first time it is needed
//! so that the launcher's start-up does not pay for it.

use crate::download::{self, Auth, DepotSpec, DownloadEvent, DownloadRequest};
use crate::error::{Failure, FailureKind};
use crate::signin::{self, SignInEvent, SignInHandle};
use rl_core::secret::Secret;
use std::sync::{Arc, OnceLock};
use steamroom::apps::AccessToken;
use steamroom::depot::AppId;
use steamroom::types::key_value::{self, KeyValue};
use tokio::runtime::{Builder, Handle, Runtime};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct Engine {
    rt: OnceLock<Runtime>,
}

/// Stops a running download.
#[derive(Clone)]
pub struct DownloadHandle {
    cancel: CancellationToken,
}

impl DownloadHandle {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    fn handle(&self) -> Handle {
        self.rt
            .get_or_init(|| {
                Builder::new_multi_thread()
                    .worker_threads(3)
                    .thread_name("steam")
                    .enable_all()
                    .build()
                    .expect("could not start the Steam runtime")
            })
            .handle()
            .clone()
    }

    /// Run a future to completion on the engine's runtime (tests and one-off calls).
    pub fn block_on<T>(&self, fut: impl std::future::Future<Output = T>) -> T {
        self.handle().block_on(fut)
    }

    /// Start signing in. Events arrive on whatever thread the runtime uses; forward them to the UI thread.
    pub fn sign_in(
        &self,
        username: String,
        password: Secret<String>,
        emit: impl Fn(SignInEvent) + Send + Sync + 'static,
    ) -> SignInHandle {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.handle()
            .spawn(signin::run(username, password, rx, Arc::new(emit)));
        SignInHandle { tx }
    }

    /// Start a download. Progress and the final result arrive through `emit`.
    pub fn download(
        &self,
        req: DownloadRequest,
        auth: Auth,
        emit: impl Fn(DownloadEvent) + Send + Sync + 'static,
    ) -> DownloadHandle {
        let cancel = CancellationToken::new();
        self.handle()
            .spawn(download::run(req, auth, Arc::new(emit), cancel.clone()));
        DownloadHandle { cancel }
    }

    /// The manifest ids of the newest build on a branch, for the given depots.
    pub fn latest_manifests(
        &self,
        auth: Auth,
        app_id: u32,
        depot_ids: Vec<u32>,
        branch: String,
        done: impl FnOnce(Result<Vec<DepotSpec>, Failure>) + Send + 'static,
    ) {
        self.handle().spawn(async move {
            done(latest_manifests(&auth, app_id, &depot_ids, &branch).await);
        });
    }
}

pub async fn latest_manifests(
    auth: &Auth,
    app_id: u32,
    depot_ids: &[u32],
    branch: &str,
) -> Result<Vec<DepotSpec>, Failure> {
    let signed_in = auth.is_signed_in();
    let client = download::login(auth).await?;
    let app = AppId(app_id);
    let tokens = client
        .pics_get_access_tokens(&[app])
        .await
        .map_err(|e| Failure::from_error(&e, signed_in))?;
    let token = tokens.into_iter().next().unwrap_or(AccessToken {
        app_id: app,
        token: 0,
    });
    let infos = client
        .pics_get_product_info(&[token])
        .await
        .map_err(|e| Failure::from_error(&e, signed_in))?;
    let kv_data = infos
        .into_iter()
        .next()
        .and_then(|i| i.kv_data)
        .ok_or_else(|| {
            Failure::new(
                FailureKind::Other,
                "Steam did not return information about this game.",
                "no product info",
            )
        })?;
    let kv: KeyValue = if kv_data.first() == Some(&0x00) {
        key_value::parse_binary_kv(&kv_data).map_err(|e| Failure::from_error(&e, signed_in))?
    } else {
        key_value::parse_text_kv(&String::from_utf8_lossy(&kv_data))
            .map_err(|e| Failure::from_error(&e, signed_in))?
    };
    let depots = kv.get("depots").ok_or_else(|| {
        Failure::new(
            FailureKind::Other,
            "Steam did not list any depots for this game.",
            "no depots",
        )
    })?;

    let mut out = Vec::new();
    for &id in depot_ids {
        let manifest = depots
            .get(&id.to_string())
            .and_then(|d| d.get("manifests"))
            .and_then(|m| m.get(branch))
            .and_then(|b| b.get("gid").and_then(|g| g.as_str()).or_else(|| b.as_str()))
            .and_then(|s| s.parse::<u64>().ok());
        match manifest {
            Some(manifest_id) => out.push(DepotSpec {
                depot_id: id,
                manifest_id,
            }),
            None => {
                return Err(Failure::new(
                    FailureKind::Other,
                    format!("Steam has no current build of depot {id} on the '{branch}' branch."),
                    "manifest not found in PICS data",
                ));
            }
        }
    }
    Ok(out)
}
