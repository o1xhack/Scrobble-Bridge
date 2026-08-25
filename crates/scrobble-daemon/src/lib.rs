mod api;
mod config;
mod state;

use std::{fs, path::Path, sync::Arc};

pub use api::scheduler;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
pub use config::{ConfigError, DaemonConfig};
use rand::random;
use scrobble_storage::{EncryptedFileVault, load_or_create_key};
use sha2::{Digest, Sha256};
pub use state::{
    AppState, LASTFM_API_KEY, LASTFM_PENDING_TOKEN, LASTFM_SESSION_KEY, LASTFM_SHARED_SECRET,
    LASTFM_USERNAME, RuntimeError, RuntimePhase, RuntimeStatus, YTMUSIC_ACCOUNT_ID,
    YTMUSIC_ACCOUNT_NAME, YTMUSIC_AUTH_USER, YTMUSIC_CHANNEL_HANDLE, YTMUSIC_COOKIE,
    YTMUSIC_DELEGATED_SESSION_ID,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Vault(#[from] scrobble_storage::VaultError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

#[derive(Debug)]
pub struct Daemon {
    config: DaemonConfig,
    state: Arc<state::AppState>,
    generated_admin_token: bool,
}

impl Daemon {
    pub fn open(config: DaemonConfig) -> Result<Self, DaemonError> {
        fs::create_dir_all(&config.data_dir)?;
        let master_key = load_or_create_key(&config.master_key_file)?;
        let vault = Arc::new(EncryptedFileVault::open(
            config.data_dir.join("credentials.enc"),
            &master_key,
        )?);
        let (admin_token, generated_admin_token) =
            load_or_create_admin_token(&config.admin_token_file)?;
        let admin_token_hash = Sha256::digest(admin_token.as_bytes()).into();
        let state = Arc::new(state::AppState::new(
            &config.data_dir.join("state.sqlite3"),
            vault,
            admin_token_hash,
            config.sync_interval,
        )?);
        Ok(Self {
            config,
            state,
            generated_admin_token,
        })
    }

    pub async fn serve(self) -> Result<(), std::io::Error> {
        if self.generated_admin_token {
            tracing::warn!(
                token_file = %self.config.admin_token_file.display(),
                "generated first-run admin token; read it from the protected token file"
            );
        }
        let scheduler_state = Arc::clone(&self.state);
        tokio::spawn(api::scheduler(scheduler_state));
        tokio::spawn(backup_scheduler(
            Arc::clone(&self.state.storage),
            self.config.data_dir.join("backups"),
        ));
        let app = api::router(Arc::clone(&self.state), &self.config);
        let listener = tokio::net::TcpListener::bind(self.config.bind).await?;
        tracing::info!(address = %listener.local_addr()?, "Scrobble Bridge daemon listening");
        let result = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await;
        if let Err(error) = self.state.storage.checkpoint() {
            tracing::error!(error = %error, "SQLite checkpoint during shutdown failed");
        }
        result
    }
}

async fn backup_scheduler(storage: Arc<scrobble_storage::Storage>, directory: std::path::PathBuf) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(86_400));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let storage = Arc::clone(&storage);
        let directory = directory.clone();
        if let Err(error) = tokio::task::spawn_blocking(move || create_backup(&storage, &directory))
            .await
            .unwrap_or_else(|error| Err(error.to_string()))
        {
            tracing::warn!(error = %error, "daily SQLite backup failed");
        }
    }
}

fn create_backup(
    storage: &scrobble_storage::Storage,
    directory: &std::path::Path,
) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!(
        "state-{}.sqlite3",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ")
    ));
    storage
        .backup_to(&path)
        .map_err(|error| error.to_string())?;
    let mut backups = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sqlite3")
        })
        .collect::<Vec<_>>();
    backups.sort();
    let remove_count = backups.len().saturating_sub(7);
    for obsolete in backups.into_iter().take(remove_count) {
        fs::remove_file(obsolete).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn load_or_create_admin_token(path: &Path) -> Result<(String, bool), std::io::Error> {
    if path.exists() {
        let token = fs::read_to_string(path)?.trim().to_owned();
        if token.len() < 32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "admin token file is empty or invalid",
            ));
        }
        return Ok((token, false));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let token = URL_SAFE_NO_PAD.encode(random::<[u8; 32]>());
    fs::write(path, &token)?;
    set_private_permissions(path)?;
    Ok((token, true))
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

async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_token_is_strong_reused_and_empty_file_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("admin.token");
        let (created, generated) = load_or_create_admin_token(&path).unwrap();
        assert!(generated);
        assert!(created.len() >= 32);
        let (reused, generated) = load_or_create_admin_token(&path).unwrap();
        assert!(!generated);
        assert_eq!(created, reused);

        let invalid = directory.path().join("invalid.token");
        fs::write(&invalid, "\n").unwrap();
        assert_eq!(
            load_or_create_admin_token(&invalid).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }
}
