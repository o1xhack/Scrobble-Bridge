use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A track as reported by `YouTube Music`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub source_id: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_seconds: Option<u32>,
}

impl Track {
    pub fn new(
        source_id: impl Into<Option<String>>,
        title: impl Into<String>,
        artist: impl Into<String>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            title: title.into(),
            artist: artist.into(),
            album: None,
            duration_seconds: None,
        }
    }
}

/// One position in a chronological history window (oldest to newest).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub track: Track,
    pub source_position: u32,
    pub played_text: Option<String>,
}

/// A source history window captured at one point in time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistorySnapshot {
    pub account_id: String,
    pub observed_at: DateTime<Utc>,
    pub items: Vec<HistoryItem>,
}

/// A play ready to enter the persistent submission outbox.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrobbleCandidate {
    pub id: Uuid,
    pub account_id: String,
    pub track: Track,
    pub started_at: DateTime<Utc>,
    pub timestamp_is_estimated: bool,
    pub source_position: u32,
    pub fingerprint: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    Submitting,
    Accepted,
    Retryable,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub candidate: ScrobbleCandidate,
    pub status: OutboxStatus,
    pub attempt_count: u32,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
