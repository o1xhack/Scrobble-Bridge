//! Minimal authenticated YouTube Music web client.
//!
//! YouTube Music does not expose this history through the public YouTube Data
//! API. The parser therefore fails closed when the internal response no longer
//! contains recognizable track renderers.

use std::{
    collections::BTreeMap,
    fmt::Write,
    time::{Duration as StdDuration, SystemTime},
};

use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, header};
use scrobble_core::{HistoryItem, HistorySnapshot, Track};
use serde::Serialize;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use thiserror::Error;

pub const MUSIC_ORIGIN: &str = "https://music.youtube.com";
pub const DEFAULT_HISTORY_ENDPOINT: &str =
    "https://music.youtube.com/youtubei/v1/browse?prettyPrint=false";
pub const DEFAULT_ACCOUNT_ENDPOINT: &str =
    "https://music.youtube.com/youtubei/v1/account/account_menu?prettyPrint=false";
pub const DEFAULT_CLIENT_VERSION: &str = "1.20260806.01.00";
pub const MAX_COOKIE_HEADER_BYTES: usize = 256 * 1024;
const REQUEST_TIMEOUT: StdDuration = StdDuration::from_secs(30);
const CONNECT_TIMEOUT: StdDuration = StdDuration::from_secs(10);

#[derive(Clone, Default)]
pub struct BrowserCredentials {
    cookie_header: String,
    pub auth_user: u8,
    delegated_session_id: Option<String>,
}

impl BrowserCredentials {
    pub fn new(cookie_header: impl Into<String>, auth_user: u8) -> Self {
        Self {
            cookie_header: cookie_header.into(),
            auth_user,
            delegated_session_id: None,
        }
    }

    #[must_use]
    pub fn with_delegated_session_id(mut self, delegated_session_id: Option<String>) -> Self {
        self.delegated_session_id = delegated_session_id;
        self
    }

    pub fn expose_cookie_header(&self) -> &str {
        &self.cookie_header
    }

    pub fn validate(&self) -> Result<(), YtMusicError> {
        if self.cookie_header.len() > MAX_COOKIE_HEADER_BYTES {
            return Err(YtMusicError::CookieTooLarge);
        }
        if self.delegated_session_id.as_ref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 128
                || !value.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            return Err(YtMusicError::InvalidDelegatedSession);
        }
        sapisid_from_cookie(&self.cookie_header).map(|_| ())
    }

    fn delegated_session_id(&self) -> Option<&str> {
        self.delegated_session_id.as_deref()
    }
}

impl std::fmt::Debug for BrowserCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserCredentials")
            .field("cookie_header", &"[REDACTED]")
            .field("auth_user", &self.auth_user)
            .field(
                "delegated_session_id",
                &self.delegated_session_id.as_ref().map(|_| "[PRESENT]"),
            )
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct YtMusicClient {
    http: Client,
    history_endpoint: String,
    account_endpoint: String,
    client_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AccountInfo {
    pub account_name: String,
    pub channel_handle: Option<String>,
    pub photo_url: Option<String>,
}

#[derive(Debug, Error)]
pub enum YtMusicError {
    #[error("YouTube Music cookie is missing __Secure-3PAPISID/SAPISID/__Secure-1PAPISID")]
    MissingSapisid,
    #[error("YouTube Music cookie header exceeds the 256 KiB limit")]
    CookieTooLarge,
    #[error("YouTube Music delegated account identifier is invalid")]
    InvalidDelegatedSession,
    #[error("system clock is before the Unix epoch")]
    InvalidSystemClock,
    #[error("failed to construct request headers: {0}")]
    InvalidHeader(#[from] header::InvalidHeaderValue),
    #[error("YouTube Music request failed: {0}")]
    Transport(String),
    #[error("YouTube Music returned HTTP {status}: {summary}")]
    ApiStatus { status: StatusCode, summary: String },
    #[error("YouTube Music history response has no recognizable tracks ({summary})")]
    UnrecognizedHistory { summary: String },
    #[error("YouTube Music account response has no recognizable active account")]
    UnrecognizedAccount,
}

impl YtMusicClient {
    pub fn new() -> Result<Self, YtMusicError> {
        Ok(Self {
            http: Client::builder()
                .user_agent("ScrobbleBridge/1.0")
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .map_err(|error| YtMusicError::Transport(error.without_url().to_string()))?,
            history_endpoint: DEFAULT_HISTORY_ENDPOINT.to_owned(),
            account_endpoint: DEFAULT_ACCOUNT_ENDPOINT.to_owned(),
            client_version: DEFAULT_CLIENT_VERSION.to_owned(),
        })
    }

    pub fn with_endpoint(
        history_endpoint: impl Into<String>,
        client_version: impl Into<String>,
    ) -> Result<Self, YtMusicError> {
        Ok(Self {
            http: Client::builder()
                .user_agent("ScrobbleBridge/1.0")
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .map_err(|error| YtMusicError::Transport(error.without_url().to_string()))?,
            history_endpoint: history_endpoint.into(),
            account_endpoint: DEFAULT_ACCOUNT_ENDPOINT.to_owned(),
            client_version: client_version.into(),
        })
    }

    pub fn with_endpoints(
        history_endpoint: impl Into<String>,
        account_endpoint: impl Into<String>,
        client_version: impl Into<String>,
    ) -> Result<Self, YtMusicError> {
        Ok(Self {
            http: Client::builder()
                .user_agent("ScrobbleBridge/1.0")
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .map_err(|error| YtMusicError::Transport(error.without_url().to_string()))?,
            history_endpoint: history_endpoint.into(),
            account_endpoint: account_endpoint.into(),
            client_version: client_version.into(),
        })
    }

    pub async fn fetch_history(
        &self,
        account_id: &str,
        credentials: &BrowserCredentials,
    ) -> Result<HistorySnapshot, YtMusicError> {
        credentials.validate()?;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| YtMusicError::InvalidSystemClock)?
            .as_secs();
        let authorization =
            authorization_header(credentials.expose_cookie_header(), timestamp, MUSIC_ORIGIN)?;
        let mut body = json!({
            "context": {
                "client": {
                    "clientName": "WEB_REMIX",
                    "clientVersion": self.client_version,
                    "hl": "en",
                    "gl": "US"
                }
            },
            "browseId": "FEmusic_history"
        });
        apply_delegated_context(&mut body, credentials.delegated_session_id());

        let mut request = self
            .http
            .post(&self.history_endpoint)
            .header(header::AUTHORIZATION, authorization)
            .header(header::COOKIE, credentials.expose_cookie_header())
            .header(header::ORIGIN, MUSIC_ORIGIN)
            .header("x-origin", MUSIC_ORIGIN)
            .header("x-goog-authuser", credentials.auth_user.to_string())
            .header("x-youtube-client-name", "67")
            .header("x-youtube-client-version", &self.client_version)
            .json(&body);
        if let Some(delegated_session_id) = credentials.delegated_session_id() {
            request = request.header("x-goog-pageid", delegated_session_id);
        }
        let response = request
            .send()
            .await
            .map_err(|error| YtMusicError::Transport(error.without_url().to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(YtMusicError::ApiStatus {
                status,
                summary: status
                    .canonical_reason()
                    .unwrap_or("upstream request rejected")
                    .to_owned(),
            });
        }

        let observed_at = Utc::now();
        let payload: Value = response
            .json()
            .await
            .map_err(|error| YtMusicError::Transport(error.without_url().to_string()))?;
        parse_history_response(account_id, observed_at, &payload)
    }

    pub async fn fetch_account_info(
        &self,
        credentials: &BrowserCredentials,
    ) -> Result<AccountInfo, YtMusicError> {
        credentials.validate()?;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| YtMusicError::InvalidSystemClock)?
            .as_secs();
        let authorization =
            authorization_header(credentials.expose_cookie_header(), timestamp, MUSIC_ORIGIN)?;
        let mut body = json!({
            "context": {
                "client": {
                    "clientName": "WEB_REMIX",
                    "clientVersion": self.client_version,
                    "hl": "en",
                    "gl": "US"
                }
            }
        });
        apply_delegated_context(&mut body, credentials.delegated_session_id());
        let mut request = self
            .http
            .post(&self.account_endpoint)
            .header(header::AUTHORIZATION, authorization)
            .header(header::COOKIE, credentials.expose_cookie_header())
            .header(header::ORIGIN, MUSIC_ORIGIN)
            .header("x-origin", MUSIC_ORIGIN)
            .header("x-goog-authuser", credentials.auth_user.to_string())
            .header("x-youtube-client-name", "67")
            .header("x-youtube-client-version", &self.client_version)
            .json(&body);
        if let Some(delegated_session_id) = credentials.delegated_session_id() {
            request = request.header("x-goog-pageid", delegated_session_id);
        }
        let response = request
            .send()
            .await
            .map_err(|error| YtMusicError::Transport(error.without_url().to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(YtMusicError::ApiStatus {
                status,
                summary: status
                    .canonical_reason()
                    .unwrap_or("upstream request rejected")
                    .to_owned(),
            });
        }
        let payload: Value = response
            .json()
            .await
            .map_err(|error| YtMusicError::Transport(error.without_url().to_string()))?;
        parse_account_info(&payload)
    }
}

fn apply_delegated_context(body: &mut Value, delegated_session_id: Option<&str>) {
    if let Some(delegated_session_id) = delegated_session_id {
        body["context"]["user"] = json!({ "onBehalfOfUser": delegated_session_id });
    }
}

pub fn parse_account_info(payload: &Value) -> Result<AccountInfo, YtMusicError> {
    const ROOT: &str =
        "/actions/0/openPopupAction/popup/multiPageMenuRenderer/header/activeAccountHeaderRenderer";
    let account_name = payload
        .pointer(&format!("{ROOT}/accountName/runs/0/text"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(YtMusicError::UnrecognizedAccount)?
        .to_owned();
    let channel_handle = payload
        .pointer(&format!("{ROOT}/channelHandle/runs/0/text"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let photo_url = payload
        .pointer(&format!("{ROOT}/accountPhoto/thumbnails/0/url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok(AccountInfo {
        account_name,
        channel_handle,
        photo_url,
    })
}

pub fn authorization_header(
    cookie_header: &str,
    timestamp: u64,
    origin: &str,
) -> Result<String, YtMusicError> {
    let sapisid = sapisid_from_cookie(cookie_header)?;
    let mut digest = Sha1::new();
    digest.update(format!("{timestamp} {sapisid} {origin}").as_bytes());
    let output = digest.finalize();
    let mut hash = String::with_capacity(output.len() * 2);
    for byte in output {
        write!(&mut hash, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(format!("SAPISIDHASH {timestamp}_{hash}"))
}

pub fn parse_history_response(
    account_id: &str,
    observed_at: DateTime<Utc>,
    payload: &Value,
) -> Result<HistorySnapshot, YtMusicError> {
    let mut renderers = Vec::new();
    collect_track_renderers(payload, &mut renderers);

    let mut newest_first = renderers
        .into_iter()
        .enumerate()
        .filter_map(|(position, renderer)| {
            let position = u32::try_from(position).ok()?;
            parse_renderer(renderer, position)
        })
        .collect::<Vec<_>>();

    if newest_first.is_empty() {
        let mut messages = Vec::new();
        collect_message_renderer_text(payload, &mut messages);
        if messages
            .iter()
            .any(|message| message == "Once you listen to something it will show up here.")
        {
            return Ok(HistorySnapshot {
                account_id: account_id.to_owned(),
                observed_at,
                items: Vec::new(),
            });
        }
        return Err(YtMusicError::UnrecognizedHistory {
            summary: history_shape_summary(payload),
        });
    }

    newest_first.reverse();
    Ok(HistorySnapshot {
        account_id: account_id.to_owned(),
        observed_at,
        items: newest_first,
    })
}

fn history_shape_summary(payload: &Value) -> String {
    let mut renderers = BTreeMap::<String, usize>::new();
    collect_renderer_names(payload, &mut renderers);
    if renderers.is_empty() {
        let top_level = payload
            .as_object()
            .map(|object| {
                object
                    .keys()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .filter(|keys| !keys.is_empty())
            .unwrap_or_else(|| "non-object payload".to_owned());
        return format!("top-level keys: {top_level}");
    }
    let summary = renderers
        .into_iter()
        .take(20)
        .map(|(name, count)| format!("{name}={count}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut messages = Vec::new();
    collect_message_renderer_text(payload, &mut messages);
    messages.sort();
    messages.dedup();
    messages.truncate(4);
    if messages.is_empty() {
        format!("renderer keys: {summary}")
    } else {
        format!(
            "renderer keys: {summary}; messages: {}",
            messages.join(" | ")
        )
    }
}

fn collect_renderer_names(value: &Value, output: &mut BTreeMap<String, usize>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if key.ends_with("Renderer") {
                    *output.entry(key.clone()).or_default() += 1;
                }
                collect_renderer_names(child, output);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_renderer_names(child, output);
            }
        }
        _ => {}
    }
}

fn collect_message_renderer_text(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(renderer) = object.get("messageRenderer") {
                collect_text_values(renderer, output);
                return;
            }
            for child in object.values() {
                collect_message_renderer_text(child, output);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_message_renderer_text(child, output);
            }
        }
        _ => {}
    }
}

fn collect_text_values(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if key == "text"
                    && let Some(text) = child
                        .as_str()
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                {
                    output.push(text.chars().take(160).collect());
                } else {
                    collect_text_values(child, output);
                }
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_text_values(child, output);
            }
        }
        _ => {}
    }
}

fn sapisid_from_cookie(cookie_header: &str) -> Result<&str, YtMusicError> {
    let cookies = cookie_header
        .split(';')
        .filter_map(|component| component.trim().split_once('='))
        .map(|(name, value)| (name.trim(), value.trim()))
        .collect::<BTreeMap<_, _>>();

    ["__Secure-3PAPISID", "SAPISID", "__Secure-1PAPISID"]
        .into_iter()
        .find_map(|name| cookies.get(name).copied())
        .filter(|value| !value.is_empty())
        .ok_or(YtMusicError::MissingSapisid)
}

fn collect_track_renderers<'a>(value: &'a Value, output: &mut Vec<&'a Value>) {
    match value {
        Value::Object(object) => {
            if let Some(renderer) = object.get("musicResponsiveListItemRenderer") {
                output.push(renderer);
                return;
            }
            for child in object.values() {
                collect_track_renderers(child, output);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_track_renderers(child, output);
            }
        }
        _ => {}
    }
}

fn parse_renderer(renderer: &Value, source_position: u32) -> Option<HistoryItem> {
    let columns = renderer.get("flexColumns")?.as_array()?;
    let title_runs = column_runs(columns.first()?)?;
    let title = joined_text(title_runs);
    if title.is_empty() {
        return None;
    }

    let metadata_runs = columns.get(1).and_then(column_runs).unwrap_or(&[]);
    let mut artists = Vec::new();
    let mut album = None;
    let mut fallback_text = Vec::new();

    for run in metadata_runs {
        let Some(text) = run.get("text").and_then(Value::as_str) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() || matches!(text, "•" | "·") {
            continue;
        }

        let page_type = run
            .pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
            .and_then(Value::as_str);
        match page_type {
            Some("MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL") => {
                artists.push(text.to_owned());
            }
            Some("MUSIC_PAGE_TYPE_ALBUM") => album = Some(text.to_owned()),
            _ => fallback_text.push(text.to_owned()),
        }
    }

    if artists.is_empty()
        && let Some(first) = fallback_text.first()
    {
        artists.push(first.clone());
    }
    if album.is_none() {
        album = fallback_text.get(1).cloned();
    }
    let artist = artists.join(", ");
    if artist.is_empty() {
        return None;
    }

    let source_id = renderer
        .pointer("/playlistItemData/videoId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let duration_seconds = renderer
        .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text")
        .and_then(Value::as_str)
        .and_then(parse_duration);

    Some(HistoryItem {
        track: Track {
            source_id,
            title,
            artist,
            album,
            duration_seconds,
        },
        source_position,
        played_text: None,
    })
}

fn column_runs(column: &Value) -> Option<&[Value]> {
    column
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs")?
        .as_array()
        .map(Vec::as_slice)
}

fn joined_text(runs: &[Value]) -> String {
    runs.iter()
        .filter_map(|run| run.get("text").and_then(Value::as_str))
        .collect::<String>()
        .trim()
        .to_owned()
}

fn parse_duration(value: &str) -> Option<u32> {
    let mut total = 0_u32;
    for component in value.split(':') {
        total = total.checked_mul(60)?;
        total = total.checked_add(component.parse().ok()?)?;
    }
    (total > 0).then_some(total)
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, http::HeaderMap, routing::post};
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn builds_known_sapisid_hash() {
        let header = authorization_header(
            "PREF=x; __Secure-3PAPISID=secret-value; SID=y",
            1_700_000_000,
            MUSIC_ORIGIN,
        )
        .unwrap();
        assert_eq!(
            header,
            "SAPISIDHASH 1700000000_293a12b25a169b039dea5288347a5b0c3e236cc3"
        );
    }

    #[test]
    fn credentials_never_print_cookie() {
        let credentials = BrowserCredentials::new("__Secure-3PAPISID=secret", 0)
            .with_delegated_session_id(Some("123456789012345678901".to_owned()));
        let output = format!("{credentials:?}");
        assert!(!output.contains("secret"));
        assert!(!output.contains("123456789012345678901"));
        assert!(output.contains("REDACTED"));
    }

    #[test]
    fn missing_sapisid_fails_before_network_use() {
        let credentials = BrowserCredentials::new("SID=not-enough", 0);
        assert!(matches!(
            credentials.validate(),
            Err(YtMusicError::MissingSapisid)
        ));
    }

    #[test]
    fn oversized_cookie_header_fails_before_network_use() {
        let credentials = BrowserCredentials::new(
            format!("__Secure-3PAPISID={}", "x".repeat(MAX_COOKIE_HEADER_BYTES)),
            0,
        );
        assert!(matches!(
            credentials.validate(),
            Err(YtMusicError::CookieTooLarge)
        ));
    }

    #[test]
    fn invalid_delegated_session_fails_before_network_use() {
        let credentials = BrowserCredentials::new("__Secure-3PAPISID=secret", 0)
            .with_delegated_session_id(Some("not-a-channel-id".to_owned()));
        assert!(matches!(
            credentials.validate(),
            Err(YtMusicError::InvalidDelegatedSession)
        ));
    }

    #[test]
    fn parses_fixture_and_returns_chronological_items() {
        let payload: Value =
            serde_json::from_str(include_str!("../../../fixtures/ytmusic/history.en.json"))
                .unwrap();
        let observed_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let snapshot = parse_history_response("account", observed_at, &payload).unwrap();

        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.items[0].track.title, "Older Song");
        assert_eq!(snapshot.items[0].track.artist, "Second Artist");
        assert_eq!(snapshot.items[1].track.title, "Newest Song");
        assert_eq!(snapshot.items[1].track.artist, "First Artist, Guest Artist");
        assert_eq!(
            snapshot.items[1].track.album.as_deref(),
            Some("First Album")
        );
        assert_eq!(snapshot.items[1].track.duration_seconds, Some(245));
        assert_eq!(snapshot.items[1].source_position, 0);
    }

    #[test]
    fn unknown_payload_fails_closed() {
        let payload = json!({"contents": []});
        assert!(matches!(
            parse_history_response("account", Utc::now(), &payload),
            Err(YtMusicError::UnrecognizedHistory { .. })
        ));
    }

    #[test]
    fn explicit_empty_history_message_is_a_valid_empty_snapshot() {
        let payload = json!({
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "itemSectionRenderer": {
                                            "contents": [{
                                                "messageRenderer": {
                                                    "text": {
                                                        "runs": [{
                                                            "text": "Once you listen to something it will show up here."
                                                        }]
                                                    }
                                                }
                                            }]
                                        }
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        });
        let snapshot = parse_history_response("empty-account", Utc::now(), &payload).unwrap();
        assert_eq!(snapshot.account_id, "empty-account");
        assert!(snapshot.items.is_empty());
    }

    #[test]
    fn parses_active_account_identity() {
        let payload = json!({
            "actions": [{
                "openPopupAction": {
                    "popup": {
                        "multiPageMenuRenderer": {
                            "header": {
                                "activeAccountHeaderRenderer": {
                                    "accountName": {"runs": [{"text": "MS-113"}]},
                                    "channelHandle": {"runs": [{"text": "@ms113"}]},
                                    "accountPhoto": {"thumbnails": [{"url": "https://example.test/avatar.jpg"}]}
                                }
                            }
                        }
                    }
                }
            }]
        });
        assert_eq!(
            parse_account_info(&payload).unwrap(),
            AccountInfo {
                account_name: "MS-113".to_owned(),
                channel_handle: Some("@ms113".to_owned()),
                photo_url: Some("https://example.test/avatar.jpg".to_owned()),
            }
        );
    }

    #[test]
    fn parses_cjk_branded_artist_and_preserves_repeated_plays() {
        let payload: Value =
            serde_json::from_str(include_str!("../../../fixtures/ytmusic/history.zh.json"))
                .unwrap();
        let snapshot = parse_history_response("中文账号", Utc::now(), &payload).unwrap();

        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.items[0].track.title, "夜空中最亮的星");
        assert_eq!(snapshot.items[1].track.title, "夜空中最亮的星");
        assert_eq!(snapshot.items[0].track.artist, "逃跑计划官方频道");
        assert_ne!(
            snapshot.items[0].source_position,
            snapshot.items[1].source_position
        );
    }

    #[tokio::test]
    async fn fetches_authenticated_history_from_mock_server() {
        async fn history(headers: HeaderMap) -> Json<Value> {
            assert!(
                headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.starts_with("SAPISIDHASH "))
            );
            assert_eq!(
                headers
                    .get("x-goog-authuser")
                    .and_then(|value| value.to_str().ok()),
                Some("2")
            );
            Json(
                serde_json::from_str(include_str!("../../../fixtures/ytmusic/history.en.json"))
                    .unwrap(),
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/history", post(history)))
                .await
                .unwrap();
        });
        let client =
            YtMusicClient::with_endpoint(format!("http://{address}/history"), "test-client")
                .unwrap();
        let credentials = BrowserCredentials::new("__Secure-3PAPISID=integration-secret", 2);

        let snapshot = client.fetch_history("account", &credentials).await.unwrap();
        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.items[1].track.title, "Newest Song");
    }

    #[tokio::test]
    async fn sends_delegated_channel_context_for_brand_accounts() {
        async fn history(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(
                headers
                    .get("x-goog-pageid")
                    .and_then(|value| value.to_str().ok()),
                Some("111111111111111111111")
            );
            assert_eq!(
                body.pointer("/context/user/onBehalfOfUser")
                    .and_then(Value::as_str),
                Some("111111111111111111111")
            );
            Json(
                serde_json::from_str(include_str!("../../../fixtures/ytmusic/history.en.json"))
                    .unwrap(),
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/history", post(history)))
                .await
                .unwrap();
        });
        let client =
            YtMusicClient::with_endpoint(format!("http://{address}/history"), "test-client")
                .unwrap();
        let credentials = BrowserCredentials::new("__Secure-3PAPISID=integration-secret", 2)
            .with_delegated_session_id(Some("111111111111111111111".to_owned()));

        let snapshot = client.fetch_history("brand", &credentials).await.unwrap();
        assert_eq!(snapshot.items.len(), 2);
    }

    #[tokio::test]
    async fn fetches_authenticated_account_info_from_mock_server() {
        async fn account(headers: HeaderMap) -> Json<Value> {
            assert!(
                headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.starts_with("SAPISIDHASH "))
            );
            Json(json!({
                "actions": [{
                    "openPopupAction": {
                        "popup": {
                            "multiPageMenuRenderer": {
                                "header": {
                                    "activeAccountHeaderRenderer": {
                                        "accountName": {"runs": [{"text": "MS-113"}]},
                                        "channelHandle": {"runs": [{"text": "@ms113"}]},
                                        "accountPhoto": {"thumbnails": [{"url": "https://example.test/avatar.jpg"}]}
                                    }
                                }
                            }
                        }
                    }
                }]
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/account", post(account)))
                .await
                .unwrap();
        });
        let client = YtMusicClient::with_endpoints(
            format!("http://{address}/history"),
            format!("http://{address}/account"),
            "test-client",
        )
        .unwrap();
        let credentials = BrowserCredentials::new("__Secure-3PAPISID=integration-secret", 2);

        let account = client.fetch_account_info(&credentials).await.unwrap();
        assert_eq!(account.account_name, "MS-113");
        assert_eq!(account.channel_handle.as_deref(), Some("@ms113"));
    }
}
