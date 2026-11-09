//! Signing in to Steam from the window: the dialog flow, token storage and sign-out.

use crate::ctx::{self, Ctx};
use rl_core::secret::Secret;
use rl_steam::{Auth, GuardKind, SignInEvent};

/// The account name we are signed in as (from settings), if any.
pub fn account(ctx: &Ctx) -> Option<String> {
    ctx.state.borrow().settings.steam_username.clone()
}

/// What a download should authenticate as. `None` means the user has to sign in first.
pub fn auth(ctx: &Ctx) -> Option<Auth> {
    let account = account(ctx)?;
    match ctx.store.load(&account) {
        Ok(Some(token)) => Some(Auth::Token {
            account,
            refresh_token: token,
        }),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(%err, "could not read the Steam token from Credential Manager");
            None
        }
    }
}

pub fn begin_sign_in(username: &str, password: &str) {
    let ctx = ctx::get();
    let username = username.trim().to_string();
    if username.is_empty() || password.is_empty() {
        set_state(
            &ctx,
            "form",
            "Enter your Steam account name and password.",
            None,
        );
        return;
    }
    if ctx.signin.borrow().is_some() {
        return;
    }
    set_state(&ctx, "working", "", None);
    let handle = ctx.engine.sign_in(
        username.clone(),
        Secret::new(password.to_string()),
        |event| {
            let _ = slint::invoke_from_event_loop(move || on_event(event));
        },
    );
    *ctx.signin.borrow_mut() = Some(handle);
}

pub fn submit_code(code: &str) {
    let ctx = ctx::get();
    if code.trim().is_empty() {
        return;
    }
    set_state(&ctx, "working", "", None);
    if let Some(handle) = ctx.signin.borrow().as_ref() {
        handle.submit_code(code.trim().to_string());
    }
}

pub fn cancel_sign_in() {
    let ctx = ctx::get();
    if let Some(handle) = ctx.signin.borrow_mut().take() {
        handle.cancel();
    }
    ctx.with_app(|a| a.set_signin_open(false));
}

pub fn sign_out() {
    let ctx = ctx::get();
    if let Some(account) = account(&ctx) {
        if let Err(err) = ctx.store.delete(&account) {
            tracing::warn!(%err, "could not remove the Steam token");
        }
    }
    ctx.state.borrow_mut().settings.steam_username = None;
    ctx.save();
    ctx.refresh();
    ctx.toast("Signed out of Steam.", "success");
}

fn set_state(ctx: &Ctx, state: &str, message: &str, guard: Option<&str>) {
    ctx.with_app(|a| {
        a.set_signin_state(state.into());
        a.set_signin_message(message.into());
        if let Some(kind) = guard {
            a.set_signin_guard_kind(kind.into());
        }
    });
}

fn on_event(event: SignInEvent) {
    let ctx = ctx::get();
    match event {
        SignInEvent::NeedGuard { code, phone, retry } => {
            // `none` means there is no code to type: the phone approval is the only way.
            let kind = match code {
                Some(GuardKind::Authenticator) => "authenticator",
                Some(GuardKind::Email) => "email",
                None => "none",
            };
            let message = if retry {
                "That code was not accepted. Check it and try again."
            } else {
                ""
            };
            ctx.with_app(|a| a.set_signin_phone(phone));
            set_state(&ctx, "guard", message, Some(kind));
        }
        SignInEvent::SignedIn {
            account,
            refresh_token,
        } => {
            *ctx.signin.borrow_mut() = None;
            match ctx.store.save(&account, &refresh_token) {
                Ok(()) => {
                    tracing::info!("signed in to Steam");
                    ctx.state.borrow_mut().settings.steam_username = Some(account.clone());
                    ctx.save();
                    ctx.refresh();
                    ctx.with_app(|a| a.set_signin_open(false));
                    ctx.toast(&format!("Signed in as {account}."), "success");
                }
                Err(err) => {
                    tracing::error!(%err, "could not store the Steam token");
                    set_state(
                        &ctx,
                        "error",
                        "You signed in, but Windows Credential Manager would not store the sign-in token, so it cannot be kept.",
                        None,
                    );
                }
            }
        }
        SignInEvent::Failed(failure) => {
            *ctx.signin.borrow_mut() = None;
            tracing::warn!(kind = ?failure.kind, detail = %failure.detail, "sign-in failed");
            set_state(&ctx, "error", &failure.message, None);
        }
    }
}
