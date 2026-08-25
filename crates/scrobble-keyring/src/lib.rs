//! macOS Keychain and Windows Credential Manager implementation of SecretVault.

use keyring::{Entry, Error};
use scrobble_storage::{SecretVault, VaultError};

#[derive(Clone, Debug)]
pub struct OsKeyringVault {
    service: String,
}

impl OsKeyringVault {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, name: &str) -> Result<Entry, VaultError> {
        Entry::new(&self.service, name).map_err(|error| backend_error(&error))
    }
}

impl SecretVault for OsKeyringVault {
    fn get(&self, name: &str) -> Result<Option<String>, VaultError> {
        match self.entry(name)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(backend_error(&error)),
        }
    }

    fn set(&self, name: &str, value: &str) -> Result<(), VaultError> {
        self.entry(name)?
            .set_password(value)
            .map_err(|error| backend_error(&error))
    }

    fn delete(&self, name: &str) -> Result<(), VaultError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(backend_error(&error)),
        }
    }
}

fn backend_error(error: &Error) -> VaultError {
    VaultError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_contains_credentials() {
        let vault = OsKeyringVault::new("com.scrobblebridge.test");
        let debug = format!("{vault:?}");
        assert!(debug.contains("com.scrobblebridge.test"));
    }
}
