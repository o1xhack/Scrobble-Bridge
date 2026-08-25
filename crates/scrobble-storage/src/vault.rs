use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit},
};
use rand::random;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const VAULT_VERSION: u8 = 1;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("secret vault I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("secret vault payload is invalid: {0}")]
    InvalidPayload(#[from] serde_json::Error),
    #[error("secret vault encoding is invalid")]
    InvalidEncoding,
    #[error("secret vault could not be decrypted")]
    Decryption,
    #[error("secret vault lock is poisoned")]
    LockPoisoned,
    #[error("secret vault key must contain exactly 32 bytes")]
    InvalidKey,
    #[error("secret vault backend failed: {0}")]
    Backend(String),
}

pub trait SecretVault: Send + Sync + std::fmt::Debug {
    fn get(&self, name: &str) -> Result<Option<String>, VaultError>;
    fn set(&self, name: &str, value: &str) -> Result<(), VaultError>;
    fn delete(&self, name: &str) -> Result<(), VaultError>;
}

#[derive(Debug, Default)]
pub struct MemoryVault {
    values: Mutex<BTreeMap<String, String>>,
}

impl SecretVault for MemoryVault {
    fn get(&self, name: &str) -> Result<Option<String>, VaultError> {
        Ok(self
            .values
            .lock()
            .map_err(|_| VaultError::LockPoisoned)?
            .get(name)
            .cloned())
    }

    fn set(&self, name: &str, value: &str) -> Result<(), VaultError> {
        self.values
            .lock()
            .map_err(|_| VaultError::LockPoisoned)?
            .insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), VaultError> {
        self.values
            .lock()
            .map_err(|_| VaultError::LockPoisoned)?
            .remove(name);
        Ok(())
    }
}

pub struct EncryptedFileVault {
    path: PathBuf,
    cipher: ChaCha20Poly1305,
    values: Mutex<BTreeMap<String, String>>,
}

impl std::fmt::Debug for EncryptedFileVault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EncryptedFileVault")
            .field("path", &self.path)
            .field("cipher", &"[REDACTED]")
            .field("values", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    version: u8,
    nonce: String,
    ciphertext: String,
}

impl EncryptedFileVault {
    pub fn open(path: impl Into<PathBuf>, key: &[u8]) -> Result<Self, VaultError> {
        if key.len() != 32 {
            return Err(VaultError::InvalidKey);
        }
        let path = path.into();
        let key = Key::try_from(key).map_err(|_| VaultError::InvalidKey)?;
        let cipher = ChaCha20Poly1305::new(&key);
        let values = if path.exists() {
            decrypt_file(&path, &cipher)?
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            path,
            cipher,
            values: Mutex::new(values),
        })
    }

    fn persist(&self, values: &BTreeMap<String, String>) -> Result<(), VaultError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let nonce_bytes = random::<[u8; 12]>();
        let plaintext = serde_json::to_vec(values)?;
        let nonce = Nonce::try_from(nonce_bytes.as_slice())
            .expect("a 12-byte array always forms a valid nonce");
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_ref())
            .map_err(|_| VaultError::Decryption)?;
        let envelope = Envelope {
            version: VAULT_VERSION,
            nonce: STANDARD_NO_PAD.encode(nonce_bytes),
            ciphertext: STANDARD_NO_PAD.encode(ciphertext),
        };
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec(&envelope)?)?;
        set_private_permissions(&temporary)?;
        fs::rename(&temporary, &self.path)?;
        Ok(())
    }
}

impl SecretVault for EncryptedFileVault {
    fn get(&self, name: &str) -> Result<Option<String>, VaultError> {
        Ok(self
            .values
            .lock()
            .map_err(|_| VaultError::LockPoisoned)?
            .get(name)
            .cloned())
    }

    fn set(&self, name: &str, value: &str) -> Result<(), VaultError> {
        let mut values = self.values.lock().map_err(|_| VaultError::LockPoisoned)?;
        values.insert(name.to_owned(), value.to_owned());
        self.persist(&values)
    }

    fn delete(&self, name: &str) -> Result<(), VaultError> {
        let mut values = self.values.lock().map_err(|_| VaultError::LockPoisoned)?;
        values.remove(name);
        self.persist(&values)
    }
}

pub fn load_or_create_key(path: impl AsRef<Path>) -> Result<[u8; 32], VaultError> {
    let path = path.as_ref();
    if path.exists() {
        let bytes = fs::read(path)?;
        return bytes.try_into().map_err(|_| VaultError::InvalidKey);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let key = random::<[u8; 32]>();
    fs::write(path, key)?;
    set_private_permissions(path)?;
    Ok(key)
}

fn decrypt_file(
    path: &Path,
    cipher: &ChaCha20Poly1305,
) -> Result<BTreeMap<String, String>, VaultError> {
    let envelope: Envelope = serde_json::from_slice(&fs::read(path)?)?;
    if envelope.version != VAULT_VERSION {
        return Err(VaultError::InvalidEncoding);
    }
    let nonce = STANDARD_NO_PAD
        .decode(envelope.nonce)
        .map_err(|_| VaultError::InvalidEncoding)?;
    let ciphertext = STANDARD_NO_PAD
        .decode(envelope.ciphertext)
        .map_err(|_| VaultError::InvalidEncoding)?;
    if nonce.len() != 12 {
        return Err(VaultError::InvalidEncoding);
    }
    let nonce = Nonce::try_from(nonce.as_slice()).map_err(|_| VaultError::InvalidEncoding)?;
    let plaintext = cipher
        .decrypt(&nonce, ciphertext.as_ref())
        .map_err(|_| VaultError::Decryption)?;
    Ok(serde_json::from_slice(&plaintext)?)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_permissions(path: &Path) -> Result<(), std::io::Error> {
    // Non-Unix platforms inherit access control from the parent directory.
    // Still verify that the just-written file is accessible before returning.
    let _ = fs::metadata(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_file_round_trip_never_contains_plaintext() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("credentials.enc");
        let key = [7; 32];
        let vault = EncryptedFileVault::open(&path, &key).unwrap();
        vault.set("ytmusic_cookie", "very-secret-cookie").unwrap();

        let disk = fs::read_to_string(&path).unwrap();
        assert!(!disk.contains("very-secret-cookie"));
        drop(vault);

        let reopened = EncryptedFileVault::open(&path, &key).unwrap();
        assert_eq!(
            reopened.get("ytmusic_cookie").unwrap().as_deref(),
            Some("very-secret-cookie")
        );
    }

    #[test]
    fn incorrect_key_cannot_open_vault() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("credentials.enc");
        let vault = EncryptedFileVault::open(&path, &[1; 32]).unwrap();
        vault.set("session", "secret").unwrap();
        drop(vault);

        assert!(matches!(
            EncryptedFileVault::open(&path, &[2; 32]),
            Err(VaultError::Decryption)
        ));
    }

    #[test]
    fn debug_output_redacts_values_and_key() {
        let directory = tempfile::tempdir().unwrap();
        let vault = EncryptedFileVault::open(directory.path().join("vault"), &[3; 32]).unwrap();
        vault.set("name", "sensitive-value").unwrap();
        let debug = format!("{vault:?}");
        assert!(!debug.contains("sensitive-value"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn generated_key_is_reused() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("master.key");
        let first = load_or_create_key(&path).unwrap();
        let second = load_or_create_key(&path).unwrap();
        assert_eq!(first, second);
    }
}
