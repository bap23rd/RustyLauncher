//! Signing in to Steam with an account name and password, including Steam Guard.
//!
//! Steam usually offers two ways to confirm a sign-in at the same time: approve the request in the
//! Steam Mobile app ("is this you?"), or type a code. This module accepts both: while it waits it keeps
//! asking Steam whether the phone approved, and it also takes a code from the UI. Whichever comes first
//! completes the sign-in. (Approving on the phone and then typing a code used to fail, because the sign-in
//! was already complete.)
//!
//! The password only lives in memory, inside this task, and is dropped as soon as Steam has answered.

use crate::error::{Failure, FailureKind};
use base64::Engine as _;
use rl_core::secret::Secret;
use std::sync::Arc;
use std::time::Duration;
use steamroom::auth::{AuthTokens, GuardType};
use steamroom::client::{Ready, SteamClient};
use steamroom::connection::{CmServer, Protocol};
use steamroom::generated::CAuthenticationBeginAuthSessionViaCredentialsRequest;
use steamroom::transport::tcp::TcpTransport;
use steamroom::transport::websocket::WebSocketTransport;
use tokio::sync::mpsc;

const DEVICE_NAME: &str = "RustyLauncher";
/// `EAuthTokenPlatformType::SteamClient`: asks Steam for a token that works for downloads.
const PLATFORM_STEAM_CLIENT: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardKind {
    /// A code from the Steam Mobile app (or another authenticator).
    Authenticator,
    /// A code Steam emailed to the account's address.
    Email,
}

pub enum SignInEvent {
    /// Steam wants a second step. The user can approve on their phone (`phone`), type a code
    /// (`code`), or both; the sign-in finishes by itself when either happens.
    /// `retry` is true after a wrong code.
    NeedGuard {
        code: Option<GuardKind>,
        phone: bool,
        retry: bool,
    },
    /// Signed in. The caller stores the token and forgets the password.
    SignedIn {
        account: String,
        refresh_token: Secret<String>,
    },
    Failed(Failure),
}

pub(crate) enum Input {
    Code(String),
    Cancel,
}

/// Lets the UI answer a Steam Guard question or cancel the sign-in.
#[derive(Clone)]
pub struct SignInHandle {
    pub(crate) tx: mpsc::UnboundedSender<Input>,
}

impl SignInHandle {
    pub fn submit_code(&self, code: String) {
        let _ = self.tx.send(Input::Code(code));
    }
    pub fn cancel(&self) {
        let _ = self.tx.send(Input::Cancel);
    }
}

/// What the confirmation kinds Steam allows mean for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Confirmations {
    /// The kind of code that can be typed, with the kind to send back to Steam.
    pub code: Option<(GuardKind, GuardType)>,
    /// Can the sign-in be approved in the Steam Mobile app?
    pub phone: bool,
}

pub(crate) fn confirmations(kinds: &[GuardType]) -> Confirmations {
    let code = if kinds.contains(&GuardType::DeviceCode) {
        Some((GuardKind::Authenticator, GuardType::DeviceCode))
    } else if kinds.contains(&GuardType::EmailCode) {
        Some((GuardKind::Email, GuardType::EmailCode))
    } else {
        None
    };
    Confirmations {
        code,
        phone: kinds.contains(&GuardType::DeviceConfirmation),
    }
}

// ---------------------------------------------------------------- connecting

async fn try_connect(server: &CmServer) -> Result<SteamClient<Ready>, Failure> {
    let net = |e: &dyn std::fmt::Debug| Failure::from_text(&format!("{e:?}"), false);
    match server.protocol {
        Protocol::Tcp => {
            let transport = TcpTransport::connect(server).await.map_err(|e| net(&e))?;
            let (client, _rx) = SteamClient::connect(transport).await.map_err(|e| net(&e))?;
            let client = client.encrypt().await.map_err(|e| net(&e))?;
            client.prepare().await.map_err(|e| net(&e))
        }
        Protocol::WebSocket => {
            let transport = WebSocketTransport::connect(server)
                .await
                .map_err(|e| net(&e))?;
            let (client, _rx) = SteamClient::connect_ws(transport)
                .await
                .map_err(|e| net(&e))?;
            client.prepare().await.map_err(|e| net(&e))
        }
    }
}

/// Connect to a Steam server, preferring TCP and falling back to WebSocket.
async fn connect() -> Result<SteamClient<Ready>, Failure> {
    let servers = CmServer::fetch()
        .await
        .unwrap_or_else(|_| CmServer::defaults());
    let mut last = None;
    for protocol in [Protocol::Tcp, Protocol::WebSocket] {
        if let Some(server) = servers.iter().find(|s| s.protocol == protocol) {
            match try_connect(server).await {
                Ok(client) => return Ok(client),
                Err(f) => last = Some(f),
            }
        }
    }
    Err(last.unwrap_or_else(|| {
        Failure::new(
            FailureKind::Network,
            "Could not reach Steam. Check your internet connection and try again.",
            "no CM servers",
        )
    }))
}

// ---------------------------------------------------------------- the flow

pub(crate) async fn run(
    username: String,
    password: Secret<String>,
    mut rx: mpsc::UnboundedReceiver<Input>,
    emit: Arc<dyn Fn(SignInEvent) + Send + Sync>,
) {
    match drive(&username, password, &mut rx, &emit).await {
        Ok(Some(tokens)) => {
            let account = tokens.account_name.clone().unwrap_or(username);
            emit(SignInEvent::SignedIn {
                account,
                refresh_token: Secret::new(tokens.refresh_token),
            });
        }
        Ok(None) => {} // cancelled by the user: nothing to report
        Err(failure) => emit(SignInEvent::Failed(failure)),
    }
}

async fn drive(
    username: &str,
    password: Secret<String>,
    rx: &mut mpsc::UnboundedReceiver<Input>,
    emit: &Arc<dyn Fn(SignInEvent) + Send + Sync>,
) -> Result<Option<AuthTokens>, Failure> {
    // Connecting can be cancelled too.
    let client = tokio::select! {
        r = connect() => r?,
        Some(Input::Cancel) = rx.recv() => return Ok(None),
    };

    let rsa = client
        .get_password_rsa_public_key(username)
        .await
        .map_err(|e| Failure::from_error(&e, false))?;
    let modulus = rsa.publickey_mod.ok_or_else(|| missing("publickey_mod"))?;
    let exponent = rsa.publickey_exp.ok_or_else(|| missing("publickey_exp"))?;
    let encrypted = steamroom::crypto::rsa::encrypt_with_rsa_public_key(
        password.expose().as_bytes(),
        &modulus,
        &exponent,
    )
    .map_err(|e| Failure::from_error(&e, false))?;
    // The password has been encrypted; drop our copy right away.
    drop(password);

    let request = CAuthenticationBeginAuthSessionViaCredentialsRequest {
        account_name: Some(username.to_string()),
        encrypted_password: Some(base64::engine::general_purpose::STANDARD.encode(encrypted)),
        encryption_timestamp: Some(rsa.timestamp.unwrap_or(0)),
        remember_login: Some(true),
        persistence: Some(1),
        platform_type: Some(PLATFORM_STEAM_CLIENT),
        device_friendly_name: Some(DEVICE_NAME.to_string()),
        ..Default::default()
    };
    let session = client
        .begin_auth_session_via_credentials(request)
        .await
        .map_err(|e| Failure::from_error(&e, false))?;
    let client_id = session.client_id.ok_or_else(|| missing("client_id"))?;
    let request_id = session.request_id.ok_or_else(|| missing("request_id"))?;
    let steam_id = session.steam_id.ok_or_else(|| missing("steamid"))?;
    let interval = Duration::from_secs_f32(session.poll_interval.unwrap_or(5.0).clamp(1.0, 10.0));

    let needs_second_step = !session.allowed_confirmations.is_empty()
        && !session.allowed_confirmations.contains(&GuardType::None);
    let confirm = confirmations(&session.allowed_confirmations);
    if needs_second_step {
        emit(SignInEvent::NeedGuard {
            code: confirm.code.map(|c| c.0),
            phone: confirm.phone,
            retry: false,
        });
    }

    // Wait for Steam to hand over tokens. They arrive when the phone approves, when a typed code is
    // accepted, or straight away when no second step is needed.
    let mut ticker = tokio::time::interval(interval);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Some(tokens) = client
                    .poll_auth_session(client_id, &request_id)
                    .await
                    .map_err(|e| Failure::from_error(&e, false))?
                {
                    return Ok(Some(tokens));
                }
            }
            input = rx.recv() => match input {
                Some(Input::Code(code)) => {
                    let Some((_, wire_kind)) = confirm.code else { continue };
                    let submitted = client
                        .submit_steam_guard_code(client_id, steam_id, code.trim(), wire_kind)
                        .await;
                    // A code can fail simply because the phone approved a moment earlier,
                    // so look for tokens before reporting an error.
                    let polled = client
                        .poll_auth_session(client_id, &request_id)
                        .await
                        .map_err(|e| Failure::from_error(&e, false))?;
                    match (polled, submitted) {
                        (Some(tokens), _) => return Ok(Some(tokens)),
                        (None, Ok(())) => {} // accepted; the next poll collects the tokens
                        (None, Err(e)) => {
                            let failure = Failure::from_error(&e, false);
                            if failure.kind == FailureKind::InvalidGuardCode {
                                emit(SignInEvent::NeedGuard {
                                    code: confirm.code.map(|c| c.0),
                                    phone: confirm.phone,
                                    retry: true,
                                });
                            } else {
                                return Err(failure);
                            }
                        }
                    }
                }
                Some(Input::Cancel) | None => return Ok(None),
            },
        }
    }
}

fn missing(field: &str) -> Failure {
    Failure::new(
        FailureKind::Other,
        "Steam sent an incomplete answer. Please try again.",
        format!("Steam response missing field: {field}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_and_code_are_both_offered_when_steam_allows_both() {
        let c = confirmations(&[GuardType::DeviceCode, GuardType::DeviceConfirmation]);
        assert!(c.phone);
        assert_eq!(c.code.map(|c| c.0), Some(GuardKind::Authenticator));
    }

    #[test]
    fn phone_only() {
        let c = confirmations(&[GuardType::DeviceConfirmation]);
        assert!(c.phone);
        assert!(c.code.is_none());
    }

    #[test]
    fn email_code_only() {
        let c = confirmations(&[GuardType::EmailCode]);
        assert!(!c.phone);
        assert_eq!(c.code, Some((GuardKind::Email, GuardType::EmailCode)));
    }

    #[test]
    fn an_authenticator_code_is_preferred_over_an_email_code() {
        let c = confirmations(&[GuardType::EmailCode, GuardType::DeviceCode]);
        assert_eq!(c.code.map(|c| c.0), Some(GuardKind::Authenticator));
    }
}
