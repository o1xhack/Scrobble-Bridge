//! Versioned local IPC protocol shared by the Chrome host and desktop App.

use std::io;

#[cfg(not(windows))]
use std::{fs, path::PathBuf};

#[cfg(not(windows))]
use directories::ProjectDirs;
use interprocess::local_socket::Name;
#[cfg(not(windows))]
use interprocess::local_socket::{GenericFilePath, ToFsName};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
pub struct CredentialSnapshot {
    pub account_id: String,
    pub auth_user: u8,
    #[serde(default)]
    pub delegated_session_id: Option<String>,
    pub cookie_header: String,
}

impl std::fmt::Debug for CredentialSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialSnapshot")
            .field("account_id", &self.account_id)
            .field("auth_user", &self.auth_user)
            .field(
                "delegated_session_id",
                &self.delegated_session_id.as_ref().map(|_| "[PRESENT]"),
            )
            .field("cookie_header", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    CredentialSnapshot {
        version: u8,
        payload: CredentialSnapshot,
    },
}

impl IpcRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::CredentialSnapshot { version, payload } => {
                if *version != PROTOCOL_VERSION {
                    return Err(ProtocolError::UnsupportedVersion(*version));
                }
                if payload.account_id.trim().is_empty() {
                    return Err(ProtocolError::InvalidAccount);
                }
                if payload.delegated_session_id.as_ref().is_some_and(|value| {
                    value.is_empty()
                        || value.len() > 128
                        || !value.bytes().all(|byte| byte.is_ascii_digit())
                }) {
                    return Err(ProtocolError::InvalidDelegatedSession);
                }
                if payload.cookie_header.len() > MAX_MESSAGE_BYTES / 2 {
                    return Err(ProtocolError::TooLarge);
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl IpcResponse {
    pub fn success() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
        }
    }
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("unsupported IPC protocol version {0}")]
    UnsupportedVersion(u8),
    #[error("account label cannot be empty")]
    InvalidAccount,
    #[error("delegated YouTube account identifier is invalid")]
    InvalidDelegatedSession,
    #[error("IPC message exceeds the size limit")]
    TooLarge,
    #[error("could not determine the local IPC directory")]
    MissingDirectory,
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub fn local_socket_name() -> Result<Name<'static>, ProtocolError> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        return Ok("com.scrobblebridge.desktop.v1".to_ns_name::<GenericNamespaced>()?);
    }

    #[cfg(not(windows))]
    {
        let directory = ipc_directory()?;
        fs::create_dir_all(&directory)?;
        set_private_directory_permissions(&directory)?;
        Ok(directory
            .join("desktop-v1.sock")
            .to_fs_name::<GenericFilePath>()?)
    }
}

#[cfg(not(windows))]
fn ipc_directory() -> Result<PathBuf, ProtocolError> {
    ProjectDirs::from("com", "Scrobble Bridge", "Scrobble Bridge")
        .map(|directories| directories.cache_dir().join("ipc"))
        .ok_or(ProtocolError::MissingDirectory)
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(all(not(unix), not(windows)))]
fn set_private_directory_permissions(_path: &std::path::Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_debug_is_redacted() {
        let snapshot = CredentialSnapshot {
            account_id: "account".to_owned(),
            auth_user: 0,
            delegated_session_id: Some("123456789012345678901".to_owned()),
            cookie_header: "sensitive-cookie".to_owned(),
        };
        let debug = format!("{snapshot:?}");
        assert!(!debug.contains("sensitive-cookie"));
        assert!(!debug.contains("123456789012345678901"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn protocol_rejects_wrong_version() {
        let request = IpcRequest::CredentialSnapshot {
            version: 2,
            payload: CredentialSnapshot {
                account_id: "account".to_owned(),
                auth_user: 0,
                delegated_session_id: None,
                cookie_header: "cookie".to_owned(),
            },
        };
        assert!(matches!(
            request.validate(),
            Err(ProtocolError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn protocol_rejects_invalid_delegated_account_identifier() {
        let request = IpcRequest::CredentialSnapshot {
            version: PROTOCOL_VERSION,
            payload: CredentialSnapshot {
                account_id: "account".to_owned(),
                auth_user: 0,
                delegated_session_id: Some("not-a-channel-id".to_owned()),
                cookie_header: "cookie".to_owned(),
            },
        };
        assert!(matches!(
            request.validate(),
            Err(ProtocolError::InvalidDelegatedSession)
        ));
    }
}
