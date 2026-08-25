//! Last.fm Web Services client with deterministic signing and duplicate checks.

use std::{collections::BTreeMap, fmt::Write, time::Duration as StdDuration};

use chrono::{DateTime, Duration, TimeZone, Utc};
use md5::{Digest, Md5};
use reqwest::{Client, Method, StatusCode};
use scrobble_core::{ScrobbleCandidate, normalize_component};
use serde_json::Value;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

pub const DEFAULT_API_ENDPOINT: &str = "https://ws.audioscrobbler.com/2.0/";
pub const DEFAULT_AUTH_ENDPOINT: &str = "https://www.last.fm/api/auth/";
pub const MAX_SCROBBLES_PER_REQUEST: usize = 50;
const REQUEST_TIMEOUT: StdDuration = StdDuration::from_secs(30);
const CONNECT_TIMEOUT: StdDuration = StdDuration::from_secs(10);

#[derive(Clone, Default)]
pub struct LastFmCredentials {
    pub api_key: String,
    shared_secret: String,
}

impl LastFmCredentials {
    pub fn new(api_key: impl Into<String>, shared_secret: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            shared_secret: shared_secret.into(),
        }
    }

    pub fn expose_shared_secret(&self) -> &str {
        &self.shared_secret
    }
}

impl std::fmt::Debug for LastFmCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LastFmCredentials")
            .field("api_key", &"[REDACTED]")
            .field("shared_secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone)]
pub struct LastFmSession {
    pub username: String,
    key: String,
}

impl LastFmSession {
    pub fn new(username: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            key: key.into(),
        }
    }

    pub fn expose_key(&self) -> &str {
        &self.key
    }
}

impl std::fmt::Debug for LastFmSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LastFmSession")
            .field("username", &self.username)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub token: String,
    pub url: Url,
}

impl std::fmt::Debug for AuthorizationRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthorizationRequest")
            .field("token", &"[REDACTED]")
            .field("url", &"[REDACTED AUTHORIZATION URL]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentTrack {
    pub title: String,
    pub artist: String,
    pub played_at: Option<DateTime<Utc>>,
    pub now_playing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrobbleResult {
    pub candidate_id: Uuid,
    pub accepted: bool,
    pub ignored_code: Option<i32>,
    pub ignored_message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LastFmClient {
    http: Client,
    credentials: LastFmCredentials,
    api_endpoint: String,
    auth_endpoint: String,
}

#[derive(Debug, Error)]
pub enum LastFmError {
    #[error("Last.fm request failed: {0}")]
    Transport(String),
    #[error("Last.fm returned HTTP {status}: {summary}")]
    HttpStatus { status: StatusCode, summary: String },
    #[error("Last.fm API error {code}: {message}")]
    Api { code: i32, message: String },
    #[error("Last.fm response is missing {0}")]
    InvalidResponse(&'static str),
    #[error("Last.fm scrobble batch cannot exceed 50 tracks")]
    BatchTooLarge,
    #[error("Last.fm authorization URL is invalid: {0}")]
    InvalidUrl(#[from] url::ParseError),
}

impl LastFmError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::HttpStatus { status, .. } => {
                *status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
            }
            Self::Api { code, .. } => matches!(code, 11 | 16 | 29),
            Self::InvalidResponse(_) | Self::BatchTooLarge | Self::InvalidUrl(_) => false,
        }
    }
}

impl LastFmClient {
    pub fn new(credentials: LastFmCredentials) -> Result<Self, LastFmError> {
        Self::with_endpoints(credentials, DEFAULT_API_ENDPOINT, DEFAULT_AUTH_ENDPOINT)
    }

    pub fn with_endpoints(
        credentials: LastFmCredentials,
        api_endpoint: impl Into<String>,
        auth_endpoint: impl Into<String>,
    ) -> Result<Self, LastFmError> {
        Ok(Self {
            http: Client::builder()
                .user_agent("ScrobbleBridge/1.0")
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .map_err(|error| LastFmError::Transport(error.without_url().to_string()))?,
            credentials,
            api_endpoint: api_endpoint.into(),
            auth_endpoint: auth_endpoint.into(),
        })
    }

    pub async fn request_authorization(&self) -> Result<AuthorizationRequest, LastFmError> {
        let mut parameters = BTreeMap::new();
        parameters.insert("api_key".to_owned(), self.credentials.api_key.clone());
        parameters.insert("method".to_owned(), "auth.getToken".to_owned());
        let payload = self.signed_request(Method::GET, parameters).await?;
        let token = payload
            .get("token")
            .and_then(Value::as_str)
            .ok_or(LastFmError::InvalidResponse("token"))?
            .to_owned();

        let mut url = Url::parse(&self.auth_endpoint)?;
        url.query_pairs_mut()
            .append_pair("api_key", &self.credentials.api_key)
            .append_pair("token", &token);
        Ok(AuthorizationRequest { token, url })
    }

    pub async fn exchange_token(&self, token: &str) -> Result<LastFmSession, LastFmError> {
        let mut parameters = BTreeMap::new();
        parameters.insert("api_key".to_owned(), self.credentials.api_key.clone());
        parameters.insert("method".to_owned(), "auth.getSession".to_owned());
        parameters.insert("token".to_owned(), token.to_owned());
        let payload = self.signed_request(Method::GET, parameters).await?;
        let session = payload
            .get("session")
            .ok_or(LastFmError::InvalidResponse("session"))?;
        let username = session
            .get("name")
            .and_then(Value::as_str)
            .ok_or(LastFmError::InvalidResponse("session.name"))?;
        let key = session
            .get("key")
            .and_then(Value::as_str)
            .ok_or(LastFmError::InvalidResponse("session.key"))?;
        Ok(LastFmSession::new(username, key))
    }

    pub async fn recent_tracks(
        &self,
        session: &LastFmSession,
        from: Option<DateTime<Utc>>,
        limit: u16,
    ) -> Result<Vec<RecentTrack>, LastFmError> {
        let mut parameters = BTreeMap::new();
        parameters.insert("api_key".to_owned(), self.credentials.api_key.clone());
        parameters.insert("method".to_owned(), "user.getRecentTracks".to_owned());
        parameters.insert("user".to_owned(), session.username.clone());
        parameters.insert("limit".to_owned(), limit.clamp(1, 200).to_string());
        parameters.insert("sk".to_owned(), session.expose_key().to_owned());
        if let Some(from) = from {
            parameters.insert("from".to_owned(), from.timestamp().to_string());
        }

        let payload = self.signed_request(Method::GET, parameters).await?;
        parse_recent_tracks(&payload)
    }

    pub async fn scrobble(
        &self,
        session: &LastFmSession,
        candidates: &[ScrobbleCandidate],
    ) -> Result<Vec<ScrobbleResult>, LastFmError> {
        if candidates.len() > MAX_SCROBBLES_PER_REQUEST {
            return Err(LastFmError::BatchTooLarge);
        }
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let mut parameters = BTreeMap::new();
        parameters.insert("api_key".to_owned(), self.credentials.api_key.clone());
        parameters.insert("method".to_owned(), "track.scrobble".to_owned());
        parameters.insert("sk".to_owned(), session.expose_key().to_owned());
        for (index, candidate) in candidates.iter().enumerate() {
            parameters.insert(format!("artist[{index}]"), candidate.track.artist.clone());
            parameters.insert(format!("track[{index}]"), candidate.track.title.clone());
            parameters.insert(
                format!("timestamp[{index}]"),
                candidate.started_at.timestamp().to_string(),
            );
            if let Some(album) = candidate
                .track
                .album
                .as_deref()
                .filter(|album| !album.is_empty())
            {
                parameters.insert(format!("album[{index}]"), album.to_owned());
            }
        }

        let payload = self.signed_request(Method::POST, parameters).await?;
        parse_scrobble_results(candidates, &payload)
    }

    async fn signed_request(
        &self,
        method: Method,
        mut parameters: BTreeMap<String, String>,
    ) -> Result<Value, LastFmError> {
        let signature = api_signature(&parameters, self.credentials.expose_shared_secret());
        parameters.insert("api_sig".to_owned(), signature);
        parameters.insert("format".to_owned(), "json".to_owned());

        let request = self.http.request(method.clone(), &self.api_endpoint);
        let response = if method == Method::GET {
            request
                .query(&parameters)
                .send()
                .await
                .map_err(|error| LastFmError::Transport(error.without_url().to_string()))?
        } else {
            request
                .form(&parameters)
                .send()
                .await
                .map_err(|error| LastFmError::Transport(error.without_url().to_string()))?
        };
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| LastFmError::Transport(error.without_url().to_string()))?;
        if !status.is_success() {
            return Err(LastFmError::HttpStatus {
                status,
                summary: status
                    .canonical_reason()
                    .unwrap_or("upstream request rejected")
                    .to_owned(),
            });
        }

        let payload: Value = serde_json::from_str(&body)
            .map_err(|_| LastFmError::InvalidResponse("valid JSON body"))?;
        if let Some(code) = payload.get("error").and_then(Value::as_i64) {
            let mut message = payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Unknown Last.fm error")
                .to_owned();
            for key in ["api_key", "api_sig", "sk", "token"] {
                if let Some(secret) = parameters.get(key).filter(|value| !value.is_empty()) {
                    message = message.replace(secret, "[REDACTED]");
                }
            }
            return Err(LastFmError::Api {
                code: i32::try_from(code).unwrap_or(i32::MAX),
                message,
            });
        }
        Ok(payload)
    }
}

pub fn api_signature(parameters: &BTreeMap<String, String>, shared_secret: &str) -> String {
    let mut digest = Md5::new();
    for (key, value) in parameters {
        if !matches!(key.as_str(), "format" | "callback" | "api_sig") {
            digest.update(key.as_bytes());
            digest.update(value.as_bytes());
        }
    }
    digest.update(shared_secret.as_bytes());
    let output = digest.finalize();
    let mut encoded = String::with_capacity(output.len() * 2);
    for byte in output {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

pub fn matches_recent_track(
    candidate: &ScrobbleCandidate,
    recent: &RecentTrack,
    time_window: Duration,
) -> bool {
    if recent.now_playing {
        return false;
    }
    let Some(played_at) = recent.played_at else {
        return false;
    };
    if normalize_component(&candidate.track.title) != normalize_component(&recent.title)
        || normalize_component(&candidate.track.artist) != normalize_component(&recent.artist)
    {
        return false;
    }

    (candidate.started_at - played_at).abs() <= time_window
}

fn parse_recent_tracks(payload: &Value) -> Result<Vec<RecentTrack>, LastFmError> {
    let tracks = payload
        .pointer("/recenttracks/track")
        .ok_or(LastFmError::InvalidResponse("recenttracks.track"))?;
    let values = value_as_list(tracks);

    values
        .into_iter()
        .map(|track| {
            let title = track
                .get("name")
                .and_then(Value::as_str)
                .ok_or(LastFmError::InvalidResponse("recent track name"))?
                .to_owned();
            let artist = text_field(track.get("artist"))
                .ok_or(LastFmError::InvalidResponse("recent track artist"))?;
            let now_playing = track
                .pointer("/@attr/nowplaying")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));
            let played_at = track
                .pointer("/date/uts")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<i64>().ok())
                .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single());
            Ok(RecentTrack {
                title,
                artist,
                played_at,
                now_playing,
            })
        })
        .collect()
}

fn parse_scrobble_results(
    candidates: &[ScrobbleCandidate],
    payload: &Value,
) -> Result<Vec<ScrobbleResult>, LastFmError> {
    let scrobbles = payload
        .pointer("/scrobbles/scrobble")
        .ok_or(LastFmError::InvalidResponse("scrobbles.scrobble"))?;
    let values = value_as_list(scrobbles);
    if values.len() != candidates.len() {
        return Err(LastFmError::InvalidResponse("one result per scrobble"));
    }

    candidates
        .iter()
        .zip(values)
        .map(|(candidate, result)| {
            let ignored = result
                .get("ignoredMessage")
                .ok_or(LastFmError::InvalidResponse("ignoredMessage"))?;
            let code = ignored
                .get("code")
                .and_then(|value| value.as_str().and_then(|value| value.parse::<i32>().ok()))
                .or_else(|| {
                    ignored
                        .get("code")
                        .and_then(Value::as_i64)
                        .and_then(|value| i32::try_from(value).ok())
                })
                .unwrap_or(0);
            let message = ignored
                .get("#text")
                .and_then(Value::as_str)
                .filter(|message| !message.is_empty())
                .map(ToOwned::to_owned);
            Ok(ScrobbleResult {
                candidate_id: candidate.id,
                accepted: code == 0,
                ignored_code: (code != 0).then_some(code),
                ignored_message: message,
            })
        })
        .collect()
}

fn value_as_list(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(values) => values.iter().collect(),
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    }
}

fn text_field(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .as_str()
        .or_else(|| value.get("#text").and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::{Json, Router, extract::Query, routing::get};
    use chrono::TimeZone;
    use scrobble_core::{Track, candidate_fingerprint};

    use super::*;

    fn candidate() -> ScrobbleCandidate {
        let started_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let track = Track::new(Some("video".to_owned()), "Believe", "Cher");
        ScrobbleCandidate {
            id: Uuid::nil(),
            account_id: "account".to_owned(),
            fingerprint: candidate_fingerprint("account", &track, started_at, 0),
            track,
            started_at,
            timestamp_is_estimated: true,
            source_position: 0,
        }
    }

    #[test]
    fn signature_is_sorted_and_excludes_format() {
        let mut parameters = BTreeMap::new();
        parameters.insert("token".to_owned(), "token".to_owned());
        parameters.insert("format".to_owned(), "json".to_owned());
        parameters.insert("method".to_owned(), "auth.getSession".to_owned());
        parameters.insert("api_key".to_owned(), "key".to_owned());
        assert_eq!(
            api_signature(&parameters, "secret"),
            "9ac306496295a8866c4a8673395540eb"
        );
    }

    #[test]
    fn secrets_are_redacted_from_debug_output() {
        let credentials = LastFmCredentials::new("public-key", "shared-secret");
        let session = LastFmSession::new("listener", "session-secret");
        assert!(!format!("{credentials:?}").contains("public-key"));
        assert!(!format!("{credentials:?}").contains("shared-secret"));
        assert!(!format!("{session:?}").contains("session-secret"));
        let authorization = AuthorizationRequest {
            token: "temporary-secret".to_owned(),
            url: Url::parse("https://example.invalid/authorize?token=temporary-secret").unwrap(),
        };
        assert!(!format!("{authorization:?}").contains("temporary-secret"));
    }

    #[test]
    fn parses_recent_tracks_array_and_now_playing_item() {
        let payload = serde_json::json!({
            "recenttracks": {
                "track": [
                    {"name": "Believe", "artist": {"#text": "Cher"}, "date": {"uts": "1700000000"}},
                    {"name": "Current", "artist": {"#text": "Artist"}, "@attr": {"nowplaying": "true"}}
                ]
            }
        });
        let tracks = parse_recent_tracks(&payload).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].played_at.unwrap().timestamp(), 1_700_000_000);
        assert!(tracks[1].now_playing);
    }

    #[test]
    fn recent_match_requires_identity_time_and_completed_play() {
        let candidate = candidate();
        let matching = RecentTrack {
            title: " BELIEVE! ".to_owned(),
            artist: "cher".to_owned(),
            played_at: Some(candidate.started_at + Duration::seconds(45)),
            now_playing: false,
        };
        assert!(matches_recent_track(
            &candidate,
            &matching,
            Duration::minutes(2)
        ));

        let now_playing = RecentTrack {
            now_playing: true,
            ..matching
        };
        assert!(!matches_recent_track(
            &candidate,
            &now_playing,
            Duration::minutes(2)
        ));
    }

    #[test]
    fn parses_ignored_scrobble_result() {
        let candidate = candidate();
        let payload = serde_json::json!({
            "scrobbles": {
                "scrobble": {
                    "ignoredMessage": {"code": "1", "#text": "Artist was ignored"}
                }
            }
        });
        let results = parse_scrobble_results(&[candidate], &payload).unwrap();
        assert!(!results[0].accepted);
        assert_eq!(results[0].ignored_code, Some(1));
    }

    #[test]
    fn retryability_matches_lastfm_temporary_errors() {
        assert!(
            LastFmError::Api {
                code: 29,
                message: "Rate limit".to_owned()
            }
            .is_retryable()
        );
        assert!(
            !LastFmError::Api {
                code: 9,
                message: "Invalid session".to_owned()
            }
            .is_retryable()
        );
    }

    #[tokio::test]
    async fn authorization_uses_signed_mock_http_request() {
        async fn authorize(Query(query): Query<HashMap<String, String>>) -> Json<Value> {
            assert_eq!(
                query.get("method").map(String::as_str),
                Some("auth.getToken")
            );
            assert_eq!(query.get("api_key").map(String::as_str), Some("public-key"));
            assert!(query.get("api_sig").is_some_and(|value| value.len() == 32));
            Json(serde_json::json!({"token": "temporary-secret"}))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/2.0/", get(authorize)))
                .await
                .unwrap();
        });
        let client = LastFmClient::with_endpoints(
            LastFmCredentials::new("public-key", "shared-secret"),
            format!("http://{address}/2.0/"),
            "https://www.last.fm/api/auth/",
        )
        .unwrap();

        let authorization = client.request_authorization().await.unwrap();
        assert_eq!(authorization.token, "temporary-secret");
        assert_eq!(
            authorization
                .url
                .query_pairs()
                .find(|(key, _)| key == "api_key")
                .map(|(_, value)| value.into_owned()),
            Some("public-key".to_owned())
        );
    }
}
