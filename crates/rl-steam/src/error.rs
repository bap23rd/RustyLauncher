//! Turning library errors into messages a player can act on.

/// What went wrong, in terms the app can react to (for example by asking the user to sign in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The saved login is missing or no longer accepted; the user must sign in.
    NeedsSignIn,
    InvalidPassword,
    InvalidGuardCode,
    /// The account does not own the game, or the build is not available to it.
    NotOwned,
    RateLimited,
    Network,
    DiskFull,
    Cancelled,
    Other,
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub kind: FailureKind,
    /// Plain English, safe to show to the user.
    pub message: String,
    /// Technical detail for the log file.
    pub detail: String,
}

impl Failure {
    pub fn new(kind: FailureKind, message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            detail: detail.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self::new(
            FailureKind::Cancelled,
            "Cancelled.",
            "cancelled by the user",
        )
    }

    /// Classify an error. `signed_in` says whether the failing request used a real account:
    /// an "access denied" for an anonymous request means "sign in", for an account it means "not owned".
    pub fn from_error(err: &(dyn std::error::Error + 'static), signed_in: bool) -> Self {
        // Walk the whole chain: the useful detail is usually a few levels down.
        let mut detail = err.to_string();
        let mut source = err.source();
        while let Some(s) = source {
            detail.push_str(": ");
            detail.push_str(&s.to_string());
            source = s.source();
        }
        let debug = format!("{err:?}");
        Self::classify(detail, &debug, signed_in)
    }

    /// Classify an error that is only available as text (for example a library report object).
    pub fn from_text(text: &str, signed_in: bool) -> Self {
        Self::classify(text.to_string(), text, signed_in)
    }

    fn classify(detail: String, debug: &str, signed_in: bool) -> Self {
        let text = format!("{detail} | {debug}").to_lowercase();
        let has = |needle: &str| text.contains(needle);

        let (kind, message) = if has("no space left") || has("os error 112") || has("disk full") {
            (
                FailureKind::DiskFull,
                "The disk is full. Free some space (or pick another folder) and resume the download.",
            )
        } else if has("invalidpassword") {
            (
                FailureKind::InvalidPassword,
                "Steam did not accept that account name and password.",
            )
        } else if has("twofactorcodemismatch") || has("invalidguardcode") {
            (
                FailureKind::InvalidGuardCode,
                "That Steam Guard code was not accepted. Check it and try again.",
            )
        } else if has("ratelimit") || has("throttle") || has("toomany") {
            (
                FailureKind::RateLimited,
                "Steam is asking you to slow down. Wait a few minutes and try again.",
            )
        } else if has("depotaccessdenied")
            || has("accessdenied")
            || has("access denied")
            || has("http 401")
            || has("http 403")
            || has("expiredlogin")
            || has("logonfailed")
        {
            if signed_in {
                (
                    FailureKind::NotOwned,
                    "Steam refused this download. Check that the account you signed in with owns Rust, and that the build IDs are right. If your login is old, sign out and sign in again.",
                )
            } else {
                (
                    FailureKind::NeedsSignIn,
                    "Steam only gives older builds to signed-in accounts that own the game. Sign in to Steam in Settings.",
                )
            }
        } else if has("dns")
            || has("disconnected")
            || has("timed out")
            || has("timeout")
            || has("connection")
            || has("http:")
            || has("error sending request")
            || has("no cm servers")
            || has("no cdn servers")
        {
            (
                FailureKind::Network,
                "Could not reach Steam. Check your internet connection and try again.",
            )
        } else {
            (
                FailureKind::Other,
                "The download failed. The log file has the details.",
            )
        };
        Failure::new(kind, message, detail)
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    enum Fake {
        #[error("{0}")]
        Msg(&'static str),
    }

    fn kind(msg: &'static str, signed_in: bool) -> FailureKind {
        Failure::from_error(&Fake::Msg(msg), signed_in).kind
    }

    #[test]
    fn access_denied_means_sign_in_when_anonymous_and_not_owned_when_signed_in() {
        assert_eq!(
            kind("Steam API call failed: access denied", false),
            FailureKind::NeedsSignIn
        );
        assert_eq!(
            kind("Steam API call failed: access denied", true),
            FailureKind::NotOwned
        );
        assert_eq!(
            kind("CDN returned HTTP 401", false),
            FailureKind::NeedsSignIn
        );
        assert_eq!(
            kind("depot access denied: depot 252494", true),
            FailureKind::NotOwned
        );
    }

    #[test]
    fn common_failures_get_their_own_kind() {
        assert_eq!(
            kind("io: No space left on device (os error 112)", true),
            FailureKind::DiskFull
        );
        assert_eq!(kind("DNS resolution failed", true), FailureKind::Network);
        assert_eq!(
            kind("service method failed: RateLimitExceeded", true),
            FailureKind::RateLimited
        );
        assert_eq!(
            kind("service method failed: InvalidPassword", true),
            FailureKind::InvalidPassword
        );
        assert_eq!(kind("something nobody expected", true), FailureKind::Other);
    }

    #[test]
    fn messages_are_plain_english() {
        let f = Failure::from_error(&Fake::Msg("DNS resolution failed"), true);
        assert!(f.message.contains("internet connection"));
        assert!(
            f.detail.contains("DNS"),
            "the technical text is kept for the log"
        );
    }
}
