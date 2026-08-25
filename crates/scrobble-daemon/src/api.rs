use std::{fmt::Write, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use lastfm_client::{LastFmClient, LastFmCredentials};
use rand::random;
use scrobble_core::OutboxStatus;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tower_http::services::{ServeDir, ServeFile};
use ytmusic_client::BrowserCredentials;

use crate::{
    DaemonConfig,
    state::{
        AppState, DEVICE_INDEX, LASTFM_API_KEY, LASTFM_PENDING_TOKEN, LASTFM_SESSION_KEY,
        LASTFM_SHARED_SECRET, LASTFM_USERNAME, PairingRequest, RuntimeError, RuntimePhase,
        RuntimeStatus, SERVER_INSTANCE_ID, YTMUSIC_ACCOUNT_ID, YTMUSIC_AUTH_USER, YTMUSIC_COOKIE,
        YTMUSIC_DELEGATED_SESSION_ID,
    },
};

const PAIRING_TTL_MINUTES: i64 = 10;
const SIGNATURE_WINDOW_SECONDS: i64 = 300;
const MAX_LABEL_CHARS: usize = 128;
const MAX_CREDENTIAL_CHARS: usize = 512;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    configured: bool,
}

#[derive(Debug, Deserialize)]
struct YouTubeCredentialsPayload {
    account_id: String,
    #[serde(default)]
    auth_user: u8,
    #[serde(default)]
    delegated_session_id: Option<String>,
    cookie_header: String,
}

#[derive(Debug, Deserialize)]
struct LastFmApplicationPayload {
    api_key: String,
    shared_secret: String,
}

#[derive(Debug, Default, Deserialize)]
struct ActivityQuery {
    limit: Option<usize>,
    offset: Option<usize>,
    search: Option<String>,
    status: Option<OutboxStatus>,
}

#[derive(Debug, Serialize)]
struct LastFmAuthorizationResponse {
    authorization_url: String,
}

#[derive(Debug, Serialize)]
struct MessageResponse {
    message: &'static str,
}

#[derive(Debug, Deserialize)]
struct PairingStartPayload {
    label: String,
}

#[derive(Debug, Serialize)]
struct PairingStartResponse {
    code: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct PairingExchangePayload {
    code: String,
    device_name: String,
}

#[derive(Debug, Serialize)]
struct PairingExchangeResponse {
    device_id: String,
    device_token: String,
    server_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DeviceRecord {
    id: String,
    label: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ExtensionCredentialResponse {
    message: &'static str,
    server_id: String,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiErrorBody {
                code: self.code,
                message: &self.message,
            }),
        )
            .into_response()
    }
}

impl From<RuntimeError> for ApiError {
    fn from(error: RuntimeError) -> Self {
        let (status, code) = match error {
            RuntimeError::NotConfigured(_) => (StatusCode::CONFLICT, "not_configured"),
            RuntimeError::Sync(_) => (StatusCode::BAD_GATEWAY, "sync_failed"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        Self {
            status,
            code,
            message: error.to_string(),
        }
    }
}

pub fn router(state: Arc<AppState>, config: &DaemonConfig) -> Router {
    let api = Router::new()
        .route("/status", get(status))
        .route("/activity", get(activity))
        .route("/sync", post(sync_now))
        .route("/pause", post(pause))
        .route("/resume", post(resume))
        .route("/credentials/ytmusic", put(save_ytmusic_credentials))
        .route("/ytmusic/identity/refresh", post(refresh_ytmusic_identity))
        .route("/lastfm/application", put(save_lastfm_application))
        .route("/lastfm/auth/start", post(start_lastfm_authorization))
        .route("/lastfm/auth/finish", post(finish_lastfm_authorization))
        .route("/pairing/start", post(start_pairing))
        .route("/pairing/exchange", post(exchange_pairing))
        .route("/devices", get(list_devices))
        .route("/devices/{device_id}", delete(revoke_device))
        .route(
            "/extension/credentials/ytmusic",
            put(save_extension_ytmusic_credentials),
        );

    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .nest("/api/v1", api)
        .fallback_service(
            ServeDir::new(&config.web_dir)
                .not_found_service(ServeFile::new(config.web_dir.join("index.html"))),
        )
        .with_state(state)
}

async fn live(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        configured: state.is_configured(),
    })
}

async fn ready(State(state): State<Arc<AppState>>) -> Response {
    let configured = state.is_configured();
    let status = if configured {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(HealthResponse {
            status: if configured { "ready" } else { "needs_setup" },
            configured,
        }),
    )
        .into_response()
}

async fn status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<crate::state::RuntimeStatus>, ApiError> {
    authorize(&headers, &state)?;
    Ok(Json(state.snapshot_status().await?))
}

async fn sync_now(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<scrobble_engine::SyncReport>, ApiError> {
    authorize(&headers, &state)?;
    Ok(Json(state.run_sync().await?))
}

async fn activity(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<scrobble_storage::ActivityPage>, ApiError> {
    authorize(&headers, &state)?;
    Ok(Json(state.activity_page(
        query.limit.unwrap_or(50),
        query.offset.unwrap_or(0),
        query.search.as_deref(),
        query.status,
    )?))
}

async fn refresh_ytmusic_identity(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ytmusic_client::AccountInfo>, ApiError> {
    authorize(&headers, &state)?;
    Ok(Json(state.refresh_ytmusic_identity().await?))
}

async fn pause(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<MessageResponse>, ApiError> {
    authorize(&headers, &state)?;
    state.set_paused(true).await?;
    Ok(Json(MessageResponse { message: "paused" }))
}

async fn resume(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<MessageResponse>, ApiError> {
    authorize(&headers, &state)?;
    state.set_paused(false).await?;
    state.trigger.notify_one();
    Ok(Json(MessageResponse { message: "resumed" }))
}

async fn save_ytmusic_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<YouTubeCredentialsPayload>,
) -> Result<Json<MessageResponse>, ApiError> {
    authorize(&headers, &state)?;
    BrowserCredentials::new(&payload.cookie_header, payload.auth_user)
        .with_delegated_session_id(payload.delegated_session_id.clone())
        .validate()
        .map_err(|error| ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_ytmusic_credentials",
            message: error.to_string(),
        })?;
    if invalid_label(&payload.account_id) {
        return Err(ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_account_id",
            message: "account_id must contain 1 to 128 characters".to_owned(),
        });
    }
    state.vault.set(YTMUSIC_COOKIE, &payload.cookie_header)?;
    state
        .vault
        .set(YTMUSIC_AUTH_USER, &payload.auth_user.to_string())?;
    state
        .vault
        .set(YTMUSIC_ACCOUNT_ID, payload.account_id.trim())?;
    save_delegated_session_id(&state, payload.delegated_session_id.as_deref())?;
    state.trigger.notify_one();
    Ok(Json(MessageResponse {
        message: "YouTube Music credentials saved",
    }))
}

async fn save_lastfm_application(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<LastFmApplicationPayload>,
) -> Result<Json<MessageResponse>, ApiError> {
    authorize(&headers, &state)?;
    if payload.api_key.trim().is_empty()
        || payload.shared_secret.trim().is_empty()
        || payload.api_key.len() > MAX_CREDENTIAL_CHARS
        || payload.shared_secret.len() > MAX_CREDENTIAL_CHARS
    {
        return Err(ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_lastfm_application",
            message: "Last.fm API key and shared secret are required".to_owned(),
        });
    }
    state.vault.set(LASTFM_API_KEY, payload.api_key.trim())?;
    state
        .vault
        .set(LASTFM_SHARED_SECRET, payload.shared_secret.trim())?;
    Ok(Json(MessageResponse {
        message: "Last.fm application credentials saved",
    }))
}

async fn start_lastfm_authorization(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<LastFmAuthorizationResponse>, ApiError> {
    authorize(&headers, &state)?;
    let client = lastfm_client(&state)?;
    let authorization = client
        .request_authorization()
        .await
        .map_err(|error| upstream_error("lastfm_auth_start", error.to_string()))?;
    state
        .vault
        .set(LASTFM_PENDING_TOKEN, &authorization.token)?;
    Ok(Json(LastFmAuthorizationResponse {
        authorization_url: authorization.url.to_string(),
    }))
}

async fn finish_lastfm_authorization(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<MessageResponse>, ApiError> {
    authorize(&headers, &state)?;
    let token = state
        .vault
        .get(LASTFM_PENDING_TOKEN)?
        .ok_or_else(|| ApiError {
            status: StatusCode::CONFLICT,
            code: "lastfm_auth_not_started",
            message: "Start Last.fm authorization first".to_owned(),
        })?;
    let session = lastfm_client(&state)?
        .exchange_token(&token)
        .await
        .map_err(|error| upstream_error("lastfm_auth_finish", error.to_string()))?;
    state.vault.set(LASTFM_USERNAME, &session.username)?;
    state.vault.set(LASTFM_SESSION_KEY, session.expose_key())?;
    state.vault.delete(LASTFM_PENDING_TOKEN)?;
    state
        .storage
        .expedite_retryable_failures("lastfm_auth", Utc::now())?;
    state.trigger.notify_one();
    Ok(Json(MessageResponse {
        message: "Last.fm account connected",
    }))
}

async fn start_pairing(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<PairingStartPayload>,
) -> Result<Json<PairingStartResponse>, ApiError> {
    authorize(&headers, &state)?;
    if invalid_label(&payload.label) {
        return Err(invalid_request(
            "Pairing label must contain 1 to 128 characters",
        ));
    }
    let code = URL_SAFE_NO_PAD.encode(random::<[u8; 9]>());
    let expires_at = Utc::now() + chrono::Duration::minutes(PAIRING_TTL_MINUTES);
    let mut requests = state.pairing_requests.lock().await;
    requests.retain(|_, request| request.expires_at > Utc::now());
    requests.insert(
        code.clone(),
        PairingRequest {
            label: payload.label.trim().to_owned(),
            expires_at,
        },
    );
    Ok(Json(PairingStartResponse { code, expires_at }))
}

async fn exchange_pairing(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<PairingExchangePayload>,
) -> Result<Json<PairingExchangeResponse>, ApiError> {
    require_https(&headers)?;
    if !payload.device_name.trim().is_empty()
        && payload.device_name.chars().count() > MAX_LABEL_CHARS
    {
        return Err(invalid_request("Device name cannot exceed 128 characters"));
    }
    let request = state
        .pairing_requests
        .lock()
        .await
        .remove(payload.code.trim())
        .filter(|request| request.expires_at > Utc::now())
        .ok_or_else(|| ApiError {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_pairing_code",
            message: "Pairing code is invalid or expired".to_owned(),
        })?;
    let device_id = uuid::Uuid::new_v4().to_string();
    let device_token = URL_SAFE_NO_PAD.encode(random::<[u8; 32]>());
    state
        .vault
        .set(&device_token_key(&device_id), &device_token)?;
    let mut devices = load_device_index(&state)?;
    devices.push(DeviceRecord {
        id: device_id.clone(),
        label: if payload.device_name.trim().is_empty() {
            request.label
        } else {
            payload.device_name.trim().to_owned()
        },
        created_at: Utc::now(),
    });
    save_device_index(&state, &devices)?;
    let server_id = server_instance_id(&state)?;
    Ok(Json(PairingExchangeResponse {
        device_id,
        device_token,
        server_id,
    }))
}

async fn list_devices(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<DeviceRecord>>, ApiError> {
    authorize(&headers, &state)?;
    Ok(Json(load_device_index(&state)?))
}

async fn revoke_device(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<MessageResponse>, ApiError> {
    authorize(&headers, &state)?;
    let device_id = uuid::Uuid::parse_str(&device_id)
        .map_err(|_| invalid_request("Device ID is invalid"))?
        .to_string();
    state.vault.delete(&device_token_key(&device_id))?;
    let mut devices = load_device_index(&state)?;
    devices.retain(|device| device.id != device_id);
    save_device_index(&state, &devices)?;
    Ok(Json(MessageResponse {
        message: "device revoked",
    }))
}

async fn save_extension_ytmusic_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ExtensionCredentialResponse>, ApiError> {
    require_https(&headers)?;
    verify_device_signature(&headers, &body, &state)?;
    let payload: YouTubeCredentialsPayload =
        serde_json::from_slice(&body).map_err(|_| invalid_request("Request body is invalid"))?;
    BrowserCredentials::new(&payload.cookie_header, payload.auth_user)
        .with_delegated_session_id(payload.delegated_session_id.clone())
        .validate()
        .map_err(|error| ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_ytmusic_credentials",
            message: error.to_string(),
        })?;
    if invalid_label(&payload.account_id) {
        return Err(invalid_request(
            "Account ID must contain 1 to 128 characters",
        ));
    }
    state.vault.set(YTMUSIC_COOKIE, &payload.cookie_header)?;
    state
        .vault
        .set(YTMUSIC_AUTH_USER, &payload.auth_user.to_string())?;
    state
        .vault
        .set(YTMUSIC_ACCOUNT_ID, payload.account_id.trim())?;
    save_delegated_session_id(&state, payload.delegated_session_id.as_deref())?;
    state.trigger.notify_one();
    Ok(Json(ExtensionCredentialResponse {
        message: "credentials saved",
        server_id: server_instance_id(&state)?,
    }))
}

fn authorize(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    let provided_hash: [u8; 32] = Sha256::digest(provided.as_bytes()).into();
    if provided_hash == state.admin_token_hash {
        Ok(())
    } else {
        Err(ApiError {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "A valid admin bearer token is required".to_owned(),
        })
    }
}

fn lastfm_client(state: &AppState) -> Result<LastFmClient, ApiError> {
    let api_key = state
        .vault
        .get(LASTFM_API_KEY)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing_setup("Last.fm API key"))?;
    let shared_secret = state
        .vault
        .get(LASTFM_SHARED_SECRET)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing_setup("Last.fm shared secret"))?;
    LastFmClient::new(LastFmCredentials::new(api_key, shared_secret))
        .map_err(|error| upstream_error("lastfm_client", error.to_string()))
}

fn missing_setup(label: &str) -> ApiError {
    ApiError {
        status: StatusCode::CONFLICT,
        code: "not_configured",
        message: format!("{label} is not configured"),
    }
}

fn invalid_request(message: &str) -> ApiError {
    ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "invalid_request",
        message: message.to_owned(),
    }
}

fn invalid_label(value: &str) -> bool {
    value.trim().is_empty() || value.chars().count() > MAX_LABEL_CHARS
}

fn save_delegated_session_id(
    state: &AppState,
    delegated_session_id: Option<&str>,
) -> Result<(), ApiError> {
    if let Some(value) = delegated_session_id {
        state.vault.set(YTMUSIC_DELEGATED_SESSION_ID, value)?;
    } else {
        state.vault.delete(YTMUSIC_DELEGATED_SESSION_ID)?;
    }
    Ok(())
}

fn require_https(headers: &HeaderMap) -> Result<(), ApiError> {
    let is_https = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("https"));
    if is_https {
        Ok(())
    } else {
        Err(ApiError {
            status: StatusCode::UPGRADE_REQUIRED,
            code: "https_required",
            message: "Extension pairing and credential delivery require HTTPS".to_owned(),
        })
    }
}

fn verify_device_signature(
    headers: &HeaderMap,
    body: &[u8],
    state: &AppState,
) -> Result<(), ApiError> {
    let device_id = signed_header(headers, "x-scrobble-device")?;
    uuid::Uuid::parse_str(device_id).map_err(|_| invalid_request("Device ID is invalid"))?;
    let timestamp = signed_header(headers, "x-scrobble-timestamp")?
        .parse::<i64>()
        .map_err(|_| invalid_request("Signature timestamp is invalid"))?;
    let nonce = signed_header(headers, "x-scrobble-nonce")?;
    if !(16..=128).contains(&nonce.len()) {
        return Err(invalid_request("Signature nonce is invalid"));
    }
    let now = Utc::now();
    if (now.timestamp() - timestamp).abs() > SIGNATURE_WINDOW_SECONDS {
        return Err(ApiError {
            status: StatusCode::UNAUTHORIZED,
            code: "signature_expired",
            message: "Request signature timestamp is outside the allowed window".to_owned(),
        });
    }
    let token = state
        .vault
        .get(&device_token_key(device_id))?
        .ok_or_else(|| ApiError {
            status: StatusCode::UNAUTHORIZED,
            code: "device_revoked",
            message: "Device is unknown or revoked".to_owned(),
        })?;
    let signature = URL_SAFE_NO_PAD
        .decode(signed_header(headers, "x-scrobble-signature")?)
        .map_err(|_| invalid_request("Request signature is invalid"))?;
    let mut body_hash = String::with_capacity(64);
    for byte in Sha256::digest(body) {
        write!(&mut body_hash, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let canonical = format!("{timestamp}\n{nonce}\n{body_hash}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(token.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(canonical.as_bytes());
    mac.verify_slice(&signature).map_err(|_| ApiError {
        status: StatusCode::UNAUTHORIZED,
        code: "invalid_signature",
        message: "Request signature is invalid".to_owned(),
    })?;
    if !state.storage.claim_device_nonce(
        device_id,
        nonce,
        now,
        now - chrono::Duration::seconds(SIGNATURE_WINDOW_SECONDS),
    )? {
        return Err(ApiError {
            status: StatusCode::CONFLICT,
            code: "replayed_request",
            message: "Request nonce has already been used".to_owned(),
        });
    }
    Ok(())
}

fn signed_header<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<&'a str, ApiError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_request(&format!("Missing {name} header")))
}

fn device_token_key(device_id: &str) -> String {
    format!("device.{device_id}.token")
}

fn load_device_index(state: &AppState) -> Result<Vec<DeviceRecord>, ApiError> {
    let Some(payload) = state.vault.get(DEVICE_INDEX)? else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&payload).map_err(|error| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "device_index_invalid",
        message: error.to_string(),
    })
}

fn save_device_index(state: &AppState, devices: &[DeviceRecord]) -> Result<(), ApiError> {
    let payload = serde_json::to_string(devices).map_err(|error| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "device_index_invalid",
        message: error.to_string(),
    })?;
    state.vault.set(DEVICE_INDEX, &payload)?;
    Ok(())
}

fn server_instance_id(state: &AppState) -> Result<String, ApiError> {
    if let Some(id) = state.vault.get(SERVER_INSTANCE_ID)? {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    state.vault.set(SERVER_INSTANCE_ID, &id)?;
    Ok(id)
}

fn upstream_error(code: &'static str, message: String) -> ApiError {
    ApiError {
        status: StatusCode::BAD_GATEWAY,
        code,
        message,
    }
}

impl From<scrobble_storage::VaultError> for ApiError {
    fn from(error: scrobble_storage::VaultError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "vault_error",
            message: error.to_string(),
        }
    }
}

impl From<scrobble_storage::StorageError> for ApiError {
    fn from(error: scrobble_storage::StorageError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "storage_error",
            message: error.to_string(),
        }
    }
}

pub async fn scheduler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(state.sync_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let explicitly_triggered = tokio::select! {
            _ = interval.tick() => false,
            () = state.trigger.notified() => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                true
            }
        };
        let status = state.status.read().await;
        let should_run =
            should_run_scheduled_sync(&status, state.is_configured(), explicitly_triggered);
        drop(status);
        if should_run && let Err(error) = state.run_sync().await {
            tracing::warn!(error = %error, "scheduled synchronization failed");
        }
    }
}

fn should_run_scheduled_sync(
    status: &RuntimeStatus,
    configured: bool,
    explicitly_triggered: bool,
) -> bool {
    if !configured || status.paused {
        return false;
    }
    if explicitly_triggered || !matches!(status.phase, RuntimePhase::NeedsAttention) {
        return true;
    }

    !matches!(
        status.last_error_code.as_deref(),
        Some(
            "ytmusic_auth"
                | "lastfm_auth"
                | "ytmusic_schema"
                | "lastfm_permanent"
                | "setup_incomplete"
                | "storage"
        )
    )
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use scrobble_storage::MemoryVault;
    use tower::ServiceExt;

    use super::*;

    fn test_app() -> (Router, String, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let token = "test-admin-token".to_owned();
        let state = Arc::new(
            AppState::new(
                &directory.path().join("state.sqlite3"),
                Arc::new(MemoryVault::default()),
                Sha256::digest(token.as_bytes()).into(),
                Duration::from_secs(600),
            )
            .unwrap(),
        );
        let config = DaemonConfig {
            data_dir: directory.path().to_path_buf(),
            web_dir: directory.path().join("web"),
            bind: "127.0.0.1:0".parse().unwrap(),
            sync_interval: Duration::from_secs(600),
            master_key_file: directory.path().join("master.key"),
            admin_token_file: directory.path().join("admin.token"),
        };
        (router(state, &config), token, directory)
    }

    #[test]
    fn recoverable_history_gaps_keep_the_background_scheduler_alive() {
        let status = RuntimeStatus {
            phase: RuntimePhase::NeedsAttention,
            last_error_code: Some("history_gap".to_owned()),
            ..RuntimeStatus::default()
        };

        assert!(should_run_scheduled_sync(&status, true, false));
    }

    #[test]
    fn a_rejected_song_does_not_block_later_listening_history() {
        let status = RuntimeStatus {
            phase: RuntimePhase::NeedsAttention,
            last_error_code: Some("submission_incomplete".to_owned()),
            ..RuntimeStatus::default()
        };

        assert!(should_run_scheduled_sync(&status, true, false));
    }

    #[test]
    fn expired_authorization_waits_for_an_explicit_credential_refresh() {
        for code in ["ytmusic_auth", "lastfm_auth", "ytmusic_schema"] {
            let status = RuntimeStatus {
                phase: RuntimePhase::NeedsAttention,
                last_error_code: Some(code.to_owned()),
                ..RuntimeStatus::default()
            };

            assert!(!should_run_scheduled_sync(&status, true, false));
            assert!(should_run_scheduled_sync(&status, true, true));
        }
    }

    #[test]
    fn paused_or_incomplete_runtime_never_starts_a_background_sync() {
        let paused = RuntimeStatus {
            phase: RuntimePhase::Paused,
            paused: true,
            ..RuntimeStatus::default()
        };

        assert!(!should_run_scheduled_sync(&paused, true, true));
        assert!(!should_run_scheduled_sync(
            &RuntimeStatus::default(),
            false,
            true
        ));
    }

    #[tokio::test]
    async fn liveness_is_public_but_status_is_protected() {
        let (app, _, _directory) = test_app();
        let live = app
            .clone()
            .oneshot(Request::get("/health/live").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(live.status(), StatusCode::OK);

        let status = app
            .oneshot(Request::get("/api/v1/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(status.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn credential_endpoint_validates_and_never_echoes_cookie() {
        let (app, token, _directory) = test_app();
        let cookie = "__Secure-3PAPISID=secret-cookie-value";
        let body = serde_json::json!({
            "account_id": "account-1",
            "auth_user": 0,
            "cookie_header": cookie,
        });
        let response = app
            .clone()
            .oneshot(
                Request::put("/api/v1/credentials/ytmusic")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(cookie));

        let status = app
            .oneshot(
                Request::get("/api/v1/status")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(status.status(), StatusCode::OK);
        let status_body = axum::body::to_bytes(status.into_body(), usize::MAX)
            .await
            .unwrap();
        let status: serde_json::Value = serde_json::from_slice(&status_body).unwrap();
        assert_eq!(status["ytmusic_configured"], true);
        assert_eq!(status["lastfm_application_configured"], false);
        assert_eq!(status["lastfm_authorized"], false);
        assert_eq!(status["configured"], false);
    }

    #[tokio::test]
    async fn readiness_requires_complete_setup() {
        let (app, _, _directory) = test_app();
        let response = app
            .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn extension_pairing_requires_https_and_signed_nonce_cannot_replay() {
        let (app, admin_token, _directory) = test_app();
        let start = app
            .clone()
            .oneshot(
                Request::post("/api/v1/pairing/start")
                    .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"label":"Chrome on Mac"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(start.status(), StatusCode::OK);
        let start_body = axum::body::to_bytes(start.into_body(), usize::MAX)
            .await
            .unwrap();
        let code = serde_json::from_slice::<serde_json::Value>(&start_body).unwrap()["code"]
            .as_str()
            .unwrap()
            .to_owned();
        let exchange_body = serde_json::json!({
            "code": code,
            "device_name": "Test extension",
        })
        .to_string();

        let insecure = app
            .clone()
            .oneshot(
                Request::post("/api/v1/pairing/exchange")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(exchange_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(insecure.status(), StatusCode::UPGRADE_REQUIRED);

        let exchange = app
            .clone()
            .oneshot(
                Request::post("/api/v1/pairing/exchange")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-forwarded-proto", "https")
                    .body(Body::from(exchange_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(exchange.status(), StatusCode::OK);
        let exchange_body = axum::body::to_bytes(exchange.into_body(), usize::MAX)
            .await
            .unwrap();
        let exchange: serde_json::Value = serde_json::from_slice(&exchange_body).unwrap();
        let device_id = exchange["device_id"].as_str().unwrap();
        let device_token = exchange["device_token"].as_str().unwrap();

        let credential_body = serde_json::json!({
            "account_id": "account-1",
            "auth_user": 0,
            "cookie_header": "__Secure-3PAPISID=extension-secret",
        })
        .to_string();
        let timestamp = Utc::now().timestamp();
        let nonce = "0123456789abcdef0123456789abcdef";
        let mut body_hash = String::with_capacity(64);
        for byte in Sha256::digest(credential_body.as_bytes()) {
            write!(&mut body_hash, "{byte:02x}").unwrap();
        }
        let canonical = format!("{timestamp}\n{nonce}\n{body_hash}");
        let mut mac = Hmac::<Sha256>::new_from_slice(device_token.as_bytes()).unwrap();
        mac.update(canonical.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        let signed_request = || {
            Request::put("/api/v1/extension/credentials/ytmusic")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-proto", "https")
                .header("x-scrobble-device", device_id)
                .header("x-scrobble-timestamp", timestamp)
                .header("x-scrobble-nonce", nonce)
                .header("x-scrobble-signature", &signature)
                .body(Body::from(credential_body.clone()))
                .unwrap()
        };
        let accepted = app.clone().oneshot(signed_request()).await.unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);

        let replayed = app.oneshot(signed_request()).await.unwrap();
        assert_eq!(replayed.status(), StatusCode::CONFLICT);
    }
}
