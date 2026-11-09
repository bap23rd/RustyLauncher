//! Where the Steam refresh token is kept: Windows Credential Manager.
//!
//! The password is never stored. The token only lets this app ask Steam for downloads; it is kept
//! in the user's own credential vault instead of a plain file.

use rl_core::secret::Secret;
use std::collections::HashMap;
use std::sync::Mutex;

pub trait TokenStore: Send + Sync {
    fn load(&self, account: &str) -> Result<Option<Secret<String>>, String>;
    fn save(&self, account: &str, token: &Secret<String>) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

const SERVICE: &str = "RustyLauncher Steam login";

/// Windows Credential Manager.
#[derive(Default)]
pub struct CredentialManager;

impl CredentialManager {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
    }
}

impl TokenStore for CredentialManager {
    fn load(&self, account: &str) -> Result<Option<Secret<String>>, String> {
        match Self::entry(account)?.get_password() {
            Ok(token) => Ok(Some(Secret::new(token))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn save(&self, account: &str, token: &Secret<String>) -> Result<(), String> {
        Self::entry(account)?
            .set_password(token.expose())
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// For tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl TokenStore for MemoryStore {
    fn load(&self, account: &str) -> Result<Option<Secret<String>>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(account)
            .cloned()
            .map(Secret::new))
    }
    fn save(&self, account: &str, token: &Secret<String>) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .insert(account.to_string(), token.expose().clone());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trip() {
        let s = MemoryStore::default();
        assert!(s.load("a").unwrap().is_none());
        s.save("a", &Secret::new("tok".into())).unwrap();
        assert_eq!(s.load("a").unwrap().unwrap().expose(), "tok");
        s.delete("a").unwrap();
        assert!(s.load("a").unwrap().is_none());
    }

    /// Uses the real Credential Manager with a throw-away account name; cleans up after itself.
    #[cfg(windows)]
    #[test]
    fn credential_manager_round_trip() {
        let store = CredentialManager;
        let account = "rustylauncher-test-account-delete-me";
        let _ = store.delete(account);
        assert!(store.load(account).unwrap().is_none());
        store
            .save(account, &Secret::new("not-a-real-token".into()))
            .unwrap();
        assert_eq!(
            store.load(account).unwrap().unwrap().expose(),
            "not-a-real-token"
        );
        store.delete(account).unwrap();
        assert!(store.load(account).unwrap().is_none());
    }
}
