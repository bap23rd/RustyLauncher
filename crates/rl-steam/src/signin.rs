//! Signing in to Steam with an account name and password, including Steam Guard.
//!
//! The flow runs as a task that talks to the UI through events and a small input channel:
//! the UI shows a dialog when it receives `NeedGuardCode` or `NeedMobileApproval`, and sends the
//! typed code back through [`SignInHandle::submit_code`]. The password only lives in memory,
//! inside this task, and is dropped as soon as Steam has answered.

use crate::error::{Failure, FailureKind};
use rl_core::secret::Secret;
use std::sync::Arc;
use steamroom_client::login::{CredentialsLoginFlow, GuardType, LoginBuilder, LoginError};
use tokio::sync::mpsc;

const DEVICE_NAME: &str = "RustyLauncher";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardKind {
    /// A code from the Steam Mobile app (or another authenticator).
    Authenticator,
    /// A code Steam emailed to the account's address.
    Email,
}

pub enum SignInEvent {
    /// Ask the user for a Steam Guard code. `retry` is true after a wrong code.
    NeedGuardCode {
        kind: GuardKind,
        retry: bool,
    },
    /// Ask the user to approve the sign-in in the Steam Mobile app.
    NeedMobileApproval,
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

fn guard_kind(kinds: &[GuardType]) -> (GuardKind, GuardType) {
    if kinds.contains(&GuardType::DeviceCode) {
        (GuardKind::Authenticator, GuardType::DeviceCode)
    } else if kinds.contains(&GuardType::EmailCode) {
        (GuardKind::Email, GuardType::EmailCode)
    } else {
        (
            GuardKind::Authenticator,
            kinds.first().copied().unwrap_or(GuardType::DeviceCode),
        )
    }
}

fn login_failure(err: &LoginError) -> Failure {
    match err {
        LoginError::InvalidPassword => Failure::new(
            FailureKind::InvalidPassword,
            "Steam did not accept that account name and password.",
            err.to_string(),
        ),
        LoginError::InvalidGuardCode => Failure::new(
            FailureKind::InvalidGuardCode,
            "That Steam Guard code was not accepted.",
            err.to_string(),
        ),
        other => Failure::from_error(other, false),
    }
}

pub(crate) async fn run(
    username: String,
    password: Secret<String>,
    mut rx: mpsc::UnboundedReceiver<Input>,
    emit: Arc<dyn Fn(SignInEvent) + Send + Sync>,
) {
    let result = drive(&username, password, &mut rx, &emit).await;
    match result {
        Ok(Some((account, token))) => emit(SignInEvent::SignedIn {
            account,
            refresh_token: Secret::new(token),
        }),
        Ok(None) => {} // cancelled by the user: nothing to report
        Err(failure) => emit(SignInEvent::Failed(failure)),
    }
}

async fn drive(
    username: &str,
    password: Secret<String>,
    rx: &mut mpsc::UnboundedReceiver<Input>,
    emit: &Arc<dyn Fn(SignInEvent) + Send + Sync>,
) -> Result<Option<(String, String)>, Failure> {
    let begin = LoginBuilder::new()
        .device_name(DEVICE_NAME)
        .with_credentials(username, password.expose().clone())
        .begin();
    // The password has been copied into the request; drop our copy right away.
    drop(password);

    let flow = tokio::select! {
        r = begin => r.map_err(|e| login_failure(&e))?,
        Some(Input::Cancel) = rx.recv() => return Ok(None),
    };

    let approved = match flow {
        CredentialsLoginFlow::Approved(a) => a,
        CredentialsLoginFlow::NeedsGuardCode(mut challenge) => {
            let (kind, wire_kind) = guard_kind(challenge.allowed_kinds());
            let mut retry = false;
            loop {
                emit(SignInEvent::NeedGuardCode { kind, retry });
                let code = match rx.recv().await {
                    Some(Input::Code(c)) => c,
                    Some(Input::Cancel) | None => return Ok(None),
                };
                match challenge.submit_code(code.trim(), wire_kind).await {
                    Ok(a) => break a,
                    Err((c, LoginError::InvalidGuardCode)) => {
                        challenge = c;
                        retry = true;
                    }
                    Err((_, e)) => return Err(login_failure(&e)),
                }
            }
        }
        CredentialsLoginFlow::NeedsMobileConfirm(mobile) => {
            emit(SignInEvent::NeedMobileApproval);
            tokio::select! {
                r = mobile.wait_for_confirmation() => r.map_err(|e| login_failure(&e))?,
                Some(Input::Cancel) = rx.recv() => return Ok(None),
            }
        }
        _ => {
            return Err(Failure::new(
                FailureKind::Other,
                "Steam asked for a sign-in step this version does not know how to handle.",
                "unknown CredentialsLoginFlow variant",
            ));
        }
    };

    let tokens = approved.tokens();
    let account = tokens
        .account_name
        .clone()
        .unwrap_or_else(|| username.to_string());
    Ok(Some((account, tokens.refresh_token.clone())))
}
