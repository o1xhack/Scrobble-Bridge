use std::{collections::HashMap, path::Path, sync::Arc};

use chrono::{DateTime, Utc};
use lastfm_client::{LastFmClient, LastFmCredentials, LastFmSession};
use scrobble_core::OutboxStatus;
use scrobble_engine::{
    LastFmScrobbleTarget, SourceOutcome, SyncEngine, SyncEngineConfig, SyncError, SyncReport,
    YtMusicHistorySource,
};
use scrobble_storage::{ActivityPage, SecretVault, Storage, StorageError, VaultError};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{Mutex, Notify, RwLock};
use ytmusic_client::{BrowserCredentials, YtMusicClient};

pub const YTMUSIC_COOKIE: &str = "ytmusic.cookie";
pub const YTMUSIC_AUTH_USER: &str = "ytmusic.auth_user";
pub const YTMUSIC_DELEGATED_SESSION_ID: &str = "ytmusic.delegated_session_id";
pub const YTMUSIC_ACCOUNT_ID: &str = "ytmusic.account_id";
pub const YTMUSIC_ACCOUNT_NAME: &str = "ytmusic.account_name";
pub const YTMUSIC_CHANNEL_HANDLE: &str = "ytmusic.channel_handle";
pub const LASTFM_API_KEY: &str = "lastfm.api_key";
pub const LASTFM_SHARED_SECRET: &str = "lastfm.shared_secret";
pub const LASTFM_USERNAME: &str = "lastfm.username";
pub const LASTFM_SESSION_KEY: &str = "lastfm.session_key";
pub const LASTFM_PENDING_TOKEN: &str = "lastfm.pending_token";
pub const DEVICE_INDEX: &str = "device.index";
pub const SERVER_INSTANCE_ID: &str = "server.instance_id";
const RUNTIME_STATE_ID: &str = "default";

#[derive(Clone, Debug)]
pub struct PairingRequest {
    pub label: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePhase {
    NeedsSetup,
    Idle,
    Syncing,
    Paused,
    RetryWaiting,
    NeedsAttention,
}

#[derive(Clone, Debug, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct RuntimeStatus {
    pub phase: RuntimePhase,
    pub configured: bool,
    pub ytmusic_configured: bool,
    pub ytmusic_account_name: Option<String>,
    pub ytmusic_channel_handle: Option<String>,
    pub lastfm_application_configured: bool,
    pub lastfm_authorized: bool,
    pub lastfm_username: Option<String>,
    pub paused: bool,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub next_scheduled_at: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub last_report: Option<SyncReport>,
    pub pending: u64,
    pub retryable: u64,
    pub rejected: u64,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            phase: RuntimePhase::NeedsSetup,
            configured: false,
            ytmusic_configured: false,
            ytmusic_account_name: None,
            ytmusic_channel_handle: None,
            lastfm_application_configured: false,
            lastfm_authorized: false,
            lastfm_username: None,
            paused: false,
            last_attempt_at: None,
            last_success_at: None,
            next_scheduled_at: None,
            last_error_code: None,
            last_error_message: None,
            last_report: None,
            pending: 0,
            retryable: 0,
            rejected: 0,
        }
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Vault(#[from] VaultError),
    #[error("setup is incomplete: {0}")]
    NotConfigured(&'static str),
    #[error("could not create YouTube Music client: {0}")]
    YtMusicClient(String),
    #[error("could not create Last.fm client: {0}")]
    LastFmClient(String),
    #[error(transparent)]
    Sync(#[from] SyncError),
}

pub struct AppState {
    pub storage: Arc<Storage>,
    pub vault: Arc<dyn SecretVault>,
    pub admin_token_hash: [u8; 32],
    pub sync_interval: std::time::Duration,
    pub status: RwLock<RuntimeStatus>,
    pub sync_lock: Mutex<()>,
    pub trigger: Notify,
    pub pairing_requests: Mutex<HashMap<String, PairingRequest>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("storage", &self.storage)
            .field("vault", &"[REDACTED]")
            .field("admin_token_hash", &"[REDACTED]")
            .field("sync_interval", &self.sync_interval)
            .finish_non_exhaustive()
    }
}

impl AppState {
    pub fn new(
        database_path: &Path,
        vault: Arc<dyn SecretVault>,
        admin_token_hash: [u8; 32],
        sync_interval: std::time::Duration,
    ) -> Result<Self, RuntimeError> {
        let storage = Arc::new(Storage::open(database_path)?);
        storage.recover_interrupted_submissions(Utc::now())?;
        let stored = storage.sync_state(RUNTIME_STATE_ID)?;
        let next_scheduled_at = if stored.paused {
            None
        } else {
            stored.last_attempt_at.and_then(|attempted_at| {
                chrono::Duration::from_std(sync_interval)
                    .ok()
                    .map(|interval| attempted_at + interval)
            })
        };
        let initial_status = RuntimeStatus {
            paused: stored.paused,
            phase: if stored.paused {
                RuntimePhase::Paused
            } else {
                RuntimePhase::NeedsSetup
            },
            last_attempt_at: stored.last_attempt_at,
            last_success_at: stored.last_success_at,
            next_scheduled_at,
            ..RuntimeStatus::default()
        };
        Ok(Self {
            storage,
            vault,
            admin_token_hash,
            sync_interval,
            status: RwLock::new(initial_status),
            sync_lock: Mutex::new(()),
            trigger: Notify::new(),
            pairing_requests: Mutex::new(HashMap::new()),
        })
    }

    pub fn is_configured(&self) -> bool {
        self.is_ytmusic_configured()
            && self.is_lastfm_application_configured()
            && self.is_lastfm_authorized()
    }

    pub fn is_ytmusic_configured(&self) -> bool {
        self.secrets_present(&[YTMUSIC_COOKIE, YTMUSIC_ACCOUNT_ID])
    }

    pub fn is_lastfm_application_configured(&self) -> bool {
        self.secrets_present(&[LASTFM_API_KEY, LASTFM_SHARED_SECRET])
    }

    pub fn is_lastfm_authorized(&self) -> bool {
        self.secrets_present(&[LASTFM_USERNAME, LASTFM_SESSION_KEY])
    }

    /// Installs bundled application credentials without replacing a user's
    /// existing Last.fm application or account authorization.
    pub fn bootstrap_lastfm_application(
        &self,
        api_key: &str,
        shared_secret: &str,
    ) -> Result<bool, RuntimeError> {
        let api_key = api_key.trim();
        let shared_secret = shared_secret.trim();
        if api_key.is_empty() || shared_secret.is_empty() {
            return Err(RuntimeError::NotConfigured(
                "bundled Last.fm application credentials are incomplete",
            ));
        }
        if self.is_lastfm_application_configured() {
            return Ok(false);
        }

        let previous_api_key = self.vault.get(LASTFM_API_KEY)?;
        self.vault.set(LASTFM_API_KEY, api_key)?;
        if let Err(error) = self.vault.set(LASTFM_SHARED_SECRET, shared_secret) {
            match previous_api_key {
                Some(previous) => self.vault.set(LASTFM_API_KEY, &previous)?,
                None => self.vault.delete(LASTFM_API_KEY)?,
            }
            return Err(error.into());
        }
        Ok(true)
    }

    pub async fn snapshot_status(&self) -> Result<RuntimeStatus, RuntimeError> {
        let mut status = self.status.read().await.clone();
        status.ytmusic_configured = self.is_ytmusic_configured();
        status.ytmusic_account_name = self.secret(YTMUSIC_ACCOUNT_NAME)?;
        status.ytmusic_channel_handle = self.secret(YTMUSIC_CHANNEL_HANDLE)?;
        status.lastfm_application_configured = self.is_lastfm_application_configured();
        status.lastfm_authorized = self.is_lastfm_authorized();
        status.lastfm_username = self.secret(LASTFM_USERNAME)?;
        status.configured = status.ytmusic_configured
            && status.lastfm_application_configured
            && status.lastfm_authorized;
        status.pending = self.storage.outbox_count(OutboxStatus::Pending)?;
        status.retryable = self.storage.outbox_count(OutboxStatus::Retryable)?;
        status.rejected = self.storage.outbox_count(OutboxStatus::Rejected)?;
        if !status.configured && !status.paused {
            status.phase = RuntimePhase::NeedsSetup;
        } else if status.configured
            && !status.paused
            && matches!(&status.phase, RuntimePhase::NeedsSetup)
        {
            status.phase = RuntimePhase::Idle;
        }
        Ok(status)
    }

    pub fn activity_page(
        &self,
        limit: usize,
        offset: usize,
        search: Option<&str>,
        status: Option<OutboxStatus>,
    ) -> Result<ActivityPage, RuntimeError> {
        let Some(account_id) = self.secret(YTMUSIC_ACCOUNT_ID)? else {
            return Ok(ActivityPage {
                items: Vec::new(),
                total: 0,
                limit: limit.clamp(1, 200),
                offset,
            });
        };
        Ok(self
            .storage
            .activity_page(&account_id, limit, offset, search, status)?)
    }

    pub async fn refresh_ytmusic_identity(
        &self,
    ) -> Result<ytmusic_client::AccountInfo, RuntimeError> {
        let cookie = self.required_secret(YTMUSIC_COOKIE, "YouTube Music credentials")?;
        let auth_user = self
            .vault
            .get(YTMUSIC_AUTH_USER)?
            .unwrap_or_else(|| "0".to_owned())
            .parse::<u8>()
            .map_err(|_| RuntimeError::NotConfigured("YouTube Music auth user is invalid"))?;
        let client =
            YtMusicClient::new().map_err(|error| RuntimeError::YtMusicClient(error.to_string()))?;
        let delegated_session_id = self.secret(YTMUSIC_DELEGATED_SESSION_ID)?;
        let account = client
            .fetch_account_info(
                &BrowserCredentials::new(cookie, auth_user)
                    .with_delegated_session_id(delegated_session_id),
            )
            .await
            .map_err(|error| RuntimeError::YtMusicClient(error.to_string()))?;
        self.vault
            .set(YTMUSIC_ACCOUNT_NAME, &account.account_name)?;
        if let Some(handle) = &account.channel_handle {
            self.vault.set(YTMUSIC_CHANNEL_HANDLE, handle)?;
        } else {
            self.vault.delete(YTMUSIC_CHANNEL_HANDLE)?;
        }
        Ok(account)
    }

    pub async fn run_sync(&self) -> Result<SyncReport, RuntimeError> {
        let _guard = self.sync_lock.lock().await;
        if self.status.read().await.paused {
            return Err(RuntimeError::NotConfigured("synchronization is paused"));
        }

        let engine = match self.build_engine() {
            Ok(engine) => engine,
            Err(error) => {
                self.record_setup_error(&error).await;
                return Err(error);
            }
        };
        self.set_syncing().await?;
        let result = engine.run_once().await;
        match result {
            Ok(report) => {
                let mut status = self.status.write().await;
                let now = Utc::now();
                status.phase = report_phase(&report);
                status.configured = true;
                if report_is_success(&report) {
                    status.last_success_at = Some(now);
                    self.storage.mark_sync_success(RUNTIME_STATE_ID, now)?;
                }
                status.next_scheduled_at = chrono::Duration::from_std(self.sync_interval)
                    .ok()
                    .map(|interval| now + interval);
                if matches!(report.source_outcome, SourceOutcome::Gap) {
                    status.last_error_code = Some("history_gap".to_owned());
                    status.last_error_message = Some(
                        "YouTube Music history could not be safely aligned; no newly observed plays were submitted"
                            .to_owned(),
                    );
                } else if report.retryable > 0 || report.rejected > 0 {
                    status.last_error_code = Some("submission_incomplete".to_owned());
                    status.last_error_message =
                        Some("One or more scrobbles remain retryable or were rejected".to_owned());
                } else {
                    status.last_error_code = None;
                    status.last_error_message = None;
                }
                status.last_report = Some(report.clone());
                Ok(report)
            }
            Err(error) => {
                self.record_error(&error).await;
                Err(error.into())
            }
        }
    }

    pub async fn set_paused(&self, paused: bool) -> Result<(), RuntimeError> {
        self.storage.set_paused(RUNTIME_STATE_ID, paused)?;
        let mut status = self.status.write().await;
        status.paused = paused;
        status.phase = if paused {
            RuntimePhase::Paused
        } else if self.is_configured() {
            RuntimePhase::Idle
        } else {
            RuntimePhase::NeedsSetup
        };
        status.next_scheduled_at = None;
        Ok(())
    }

    async fn set_syncing(&self) -> Result<(), RuntimeError> {
        let mut status = self.status.write().await;
        let now = Utc::now();
        self.storage.mark_sync_attempt(RUNTIME_STATE_ID, now)?;
        status.phase = RuntimePhase::Syncing;
        status.last_attempt_at = Some(now);
        status.last_error_code = None;
        status.last_error_message = None;
        Ok(())
    }

    async fn record_error(&self, error: &SyncError) {
        let mut status = self.status.write().await;
        let (phase, code) = match error {
            SyncError::History(failure) | SyncError::Target(failure) => match failure.disposition {
                scrobble_engine::FailureDisposition::Retry => {
                    (RuntimePhase::RetryWaiting, failure.code.clone())
                }
                scrobble_engine::FailureDisposition::Pause
                | scrobble_engine::FailureDisposition::Reject => {
                    (RuntimePhase::NeedsAttention, failure.code.clone())
                }
            },
            SyncError::Storage(_) => (RuntimePhase::NeedsAttention, "storage".to_owned()),
        };
        status.phase = phase;
        status.last_error_code = Some(code);
        status.last_error_message = Some(error.to_string());
        status.next_scheduled_at = if matches!(status.phase, RuntimePhase::RetryWaiting) {
            chrono::Duration::from_std(self.sync_interval)
                .ok()
                .map(|interval| Utc::now() + interval)
        } else {
            None
        };
    }

    async fn record_setup_error(&self, error: &RuntimeError) {
        let mut status = self.status.write().await;
        status.phase = if self.is_configured() {
            RuntimePhase::NeedsAttention
        } else {
            RuntimePhase::NeedsSetup
        };
        status.last_error_code = Some("setup_incomplete".to_owned());
        status.last_error_message = Some(error.to_string());
        status.next_scheduled_at = None;
    }

    fn build_engine(
        &self,
    ) -> Result<SyncEngine<YtMusicHistorySource, LastFmScrobbleTarget>, RuntimeError> {
        let cookie = self.required_secret(YTMUSIC_COOKIE, "YouTube Music credentials")?;
        let account_id = self.required_secret(YTMUSIC_ACCOUNT_ID, "YouTube Music account")?;
        let delegated_session_id = self.secret(YTMUSIC_DELEGATED_SESSION_ID)?;
        let auth_user = self
            .vault
            .get(YTMUSIC_AUTH_USER)?
            .unwrap_or_else(|| "0".to_owned())
            .parse::<u8>()
            .map_err(|_| RuntimeError::NotConfigured("YouTube Music auth user is invalid"))?;
        let api_key = self.required_secret(LASTFM_API_KEY, "Last.fm API key")?;
        let shared_secret = self.required_secret(LASTFM_SHARED_SECRET, "Last.fm shared secret")?;
        let username = self.required_secret(LASTFM_USERNAME, "Last.fm session")?;
        let session_key = self.required_secret(LASTFM_SESSION_KEY, "Last.fm session")?;
        let history = YtMusicHistorySource {
            client: YtMusicClient::new()
                .map_err(|error| RuntimeError::YtMusicClient(error.to_string()))?,
            account_id,
            credentials: BrowserCredentials::new(cookie, auth_user)
                .with_delegated_session_id(delegated_session_id),
        };
        let target = LastFmScrobbleTarget {
            client: LastFmClient::new(LastFmCredentials::new(api_key, shared_secret))
                .map_err(|error| RuntimeError::LastFmClient(error.to_string()))?,
            session: LastFmSession::new(username, session_key),
        };
        Ok(SyncEngine::new(
            Arc::clone(&self.storage),
            history,
            target,
            SyncEngineConfig::default(),
        ))
    }

    fn required_secret(&self, name: &str, label: &'static str) -> Result<String, RuntimeError> {
        self.vault
            .get(name)?
            .filter(|value| !value.is_empty())
            .ok_or(RuntimeError::NotConfigured(label))
    }

    fn secret(&self, name: &str) -> Result<Option<String>, RuntimeError> {
        Ok(self
            .vault
            .get(name)?
            .filter(|value| !value.trim().is_empty()))
    }

    fn secrets_present(&self, names: &[&str]) -> bool {
        names.iter().all(|name| {
            self.vault
                .get(name)
                .ok()
                .flatten()
                .is_some_and(|value| !value.is_empty())
        })
    }
}

fn report_phase(report: &SyncReport) -> RuntimePhase {
    if matches!(report.source_outcome, SourceOutcome::Gap) || report.rejected > 0 {
        RuntimePhase::NeedsAttention
    } else if report.retryable > 0 {
        RuntimePhase::RetryWaiting
    } else {
        RuntimePhase::Idle
    }
}

fn report_is_success(report: &SyncReport) -> bool {
    !matches!(report.source_outcome, SourceOutcome::Gap)
        && report.retryable == 0
        && report.rejected == 0
}

#[cfg(test)]
mod tests {
    use scrobble_storage::{MemoryVault, SecretVault};

    use super::*;

    fn test_state() -> (AppState, Arc<MemoryVault>, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let vault = Arc::new(MemoryVault::default());
        let state = AppState::new(
            &directory.path().join("state.sqlite3"),
            Arc::clone(&vault) as Arc<dyn SecretVault>,
            [0; 32],
            std::time::Duration::from_secs(600),
        )
        .unwrap();
        (state, vault, directory)
    }

    #[derive(Debug, Default)]
    struct SharedSecretFailingVault {
        inner: MemoryVault,
    }

    impl SecretVault for SharedSecretFailingVault {
        fn get(&self, name: &str) -> Result<Option<String>, VaultError> {
            self.inner.get(name)
        }

        fn set(&self, name: &str, value: &str) -> Result<(), VaultError> {
            if name == LASTFM_SHARED_SECRET {
                return Err(VaultError::Backend("simulated write failure".to_owned()));
            }
            self.inner.set(name, value)
        }

        fn delete(&self, name: &str) -> Result<(), VaultError> {
            self.inner.delete(name)
        }
    }

    fn report(source_outcome: SourceOutcome) -> SyncReport {
        SyncReport {
            source_outcome,
            overlap_matches: 0,
            discovered: 0,
            enqueued: 0,
            matched_existing: 0,
            submitted: 0,
            accepted: 0,
            retryable: 0,
            rejected: 0,
            gap_best_overlap: None,
        }
    }

    #[test]
    fn history_gap_requires_attention_and_never_marks_success() {
        let report = report(SourceOutcome::Gap);
        assert_eq!(report_phase(&report), RuntimePhase::NeedsAttention);
        assert!(!report_is_success(&report));
    }

    #[test]
    fn clean_delta_is_a_success() {
        let report = report(SourceOutcome::Delta);
        assert_eq!(report_phase(&report), RuntimePhase::Idle);
        assert!(report_is_success(&report));
    }

    #[test]
    fn bundled_lastfm_application_is_saved_only_when_missing() {
        let (state, vault, _directory) = test_state();

        assert!(
            state
                .bootstrap_lastfm_application("  bundled-key  ", " bundled-secret ")
                .unwrap()
        );
        assert_eq!(
            vault.get(LASTFM_API_KEY).unwrap().as_deref(),
            Some("bundled-key")
        );
        assert_eq!(
            vault.get(LASTFM_SHARED_SECRET).unwrap().as_deref(),
            Some("bundled-secret")
        );
        assert!(state.is_lastfm_application_configured());
    }

    #[test]
    fn bundled_lastfm_application_never_replaces_custom_credentials() {
        let (state, vault, _directory) = test_state();
        vault.set(LASTFM_API_KEY, "custom-key").unwrap();
        vault.set(LASTFM_SHARED_SECRET, "custom-secret").unwrap();

        assert!(
            !state
                .bootstrap_lastfm_application("bundled-key", "bundled-secret")
                .unwrap()
        );
        assert_eq!(
            vault.get(LASTFM_API_KEY).unwrap().as_deref(),
            Some("custom-key")
        );
        assert_eq!(
            vault.get(LASTFM_SHARED_SECRET).unwrap().as_deref(),
            Some("custom-secret")
        );
    }

    #[test]
    fn bundled_lastfm_application_rejects_partial_credentials() {
        let (state, vault, _directory) = test_state();

        assert!(
            state
                .bootstrap_lastfm_application("bundled-key", "  ")
                .is_err()
        );
        assert!(vault.get(LASTFM_API_KEY).unwrap().is_none());
        assert!(vault.get(LASTFM_SHARED_SECRET).unwrap().is_none());
    }

    #[test]
    fn failed_secret_write_restores_the_previous_application_key() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Arc::new(SharedSecretFailingVault::default());
        vault.set(LASTFM_API_KEY, "existing-partial-key").unwrap();
        let state = AppState::new(
            &directory.path().join("state.sqlite3"),
            Arc::clone(&vault) as Arc<dyn SecretVault>,
            [0; 32],
            std::time::Duration::from_secs(600),
        )
        .unwrap();

        assert!(
            state
                .bootstrap_lastfm_application("bundled-key", "bundled-secret")
                .is_err()
        );
        assert_eq!(
            vault.get(LASTFM_API_KEY).unwrap().as_deref(),
            Some("existing-partial-key")
        );
        assert!(vault.get(LASTFM_SHARED_SECRET).unwrap().is_none());
    }

    #[tokio::test]
    async fn incomplete_setup_never_leaves_runtime_stuck_syncing() {
        let (state, _vault, _directory) = test_state();

        assert!(matches!(
            state.run_sync().await,
            Err(RuntimeError::NotConfigured("YouTube Music credentials"))
        ));

        let status = state.snapshot_status().await.unwrap();
        assert_eq!(status.phase, RuntimePhase::NeedsSetup);
        assert_eq!(status.last_error_code.as_deref(), Some("setup_incomplete"));
        assert!(status.last_attempt_at.is_none());
        assert!(status.next_scheduled_at.is_none());
    }

    #[tokio::test]
    async fn invalid_saved_account_context_requires_attention_without_syncing() {
        let (state, vault, _directory) = test_state();
        for (name, value) in [
            (YTMUSIC_COOKIE, "SAPISID=cookie"),
            (YTMUSIC_ACCOUNT_ID, "account"),
            (YTMUSIC_AUTH_USER, "invalid"),
            (LASTFM_API_KEY, "api-key"),
            (LASTFM_SHARED_SECRET, "shared-secret"),
            (LASTFM_USERNAME, "listener"),
            (LASTFM_SESSION_KEY, "session"),
        ] {
            vault.set(name, value).unwrap();
        }

        assert!(matches!(
            state.run_sync().await,
            Err(RuntimeError::NotConfigured(
                "YouTube Music auth user is invalid"
            ))
        ));

        let status = state.snapshot_status().await.unwrap();
        assert_eq!(status.phase, RuntimePhase::NeedsAttention);
        assert!(status.last_attempt_at.is_none());
        assert!(status.next_scheduled_at.is_none());
    }

    #[tokio::test]
    async fn paused_state_survives_a_process_restart() {
        let (state, vault, directory) = test_state();
        state.set_paused(true).await.unwrap();
        drop(state);

        let restored = AppState::new(
            &directory.path().join("state.sqlite3"),
            vault,
            [0; 32],
            std::time::Duration::from_secs(600),
        )
        .unwrap();
        let status = restored.snapshot_status().await.unwrap();

        assert!(status.paused);
        assert_eq!(status.phase, RuntimePhase::Paused);
        assert!(status.next_scheduled_at.is_none());
    }

    #[tokio::test]
    async fn retryable_provider_failure_schedules_another_attempt() {
        let (state, _vault, _directory) = test_state();
        state
            .record_error(&SyncError::History(
                scrobble_engine::ProviderFailure::retry(
                    "ytmusic_unavailable",
                    "temporary network failure",
                ),
            ))
            .await;

        let status = state.status.read().await;
        assert_eq!(status.phase, RuntimePhase::RetryWaiting);
        assert_eq!(
            status.last_error_code.as_deref(),
            Some("ytmusic_unavailable")
        );
        assert!(
            status
                .next_scheduled_at
                .is_some_and(|scheduled| scheduled > Utc::now())
        );
    }

    #[tokio::test]
    async fn expired_credentials_require_attention_without_false_retry_time() {
        let (state, _vault, _directory) = test_state();
        state
            .record_error(&SyncError::History(
                scrobble_engine::ProviderFailure::pause(
                    "ytmusic_auth",
                    "YouTube Music returned HTTP 401",
                ),
            ))
            .await;

        let status = state.status.read().await;
        assert_eq!(status.phase, RuntimePhase::NeedsAttention);
        assert_eq!(status.last_error_code.as_deref(), Some("ytmusic_auth"));
        assert!(status.next_scheduled_at.is_none());
    }
}
