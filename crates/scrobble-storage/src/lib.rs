//! SQLite persistence for source snapshots and the idempotent scrobble outbox.

mod vault;

pub use vault::{EncryptedFileVault, MemoryVault, SecretVault, VaultError, load_or_create_key};

use std::{
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use scrobble_core::{HistorySnapshot, OutboxEntry, OutboxStatus, ScrobbleCandidate};
use thiserror::Error;
use uuid::Uuid;

const MIGRATION_V1: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS history_snapshots (
    account_id TEXT PRIMARY KEY NOT NULL,
    observed_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS scrobble_outbox (
    id TEXT PRIMARY KEY NOT NULL,
    fingerprint TEXT UNIQUE NOT NULL,
    account_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN ('pending', 'submitting', 'accepted', 'retryable', 'rejected')
    ),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT NOT NULL,
    last_error_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_outbox_due
    ON scrobble_outbox (status, next_attempt_at, created_at);
CREATE INDEX IF NOT EXISTS idx_outbox_account
    ON scrobble_outbox (account_id, created_at);

CREATE TABLE IF NOT EXISTS sync_state (
    account_id TEXT PRIMARY KEY NOT NULL,
    paused_reason TEXT,
    last_attempt_at TEXT,
    last_success_at TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS device_nonces (
    device_id TEXT NOT NULL,
    nonce TEXT NOT NULL,
    seen_at TEXT NOT NULL,
    PRIMARY KEY (device_id, nonce)
);
CREATE INDEX IF NOT EXISTS idx_device_nonces_seen_at
    ON device_nonces (seen_at);

PRAGMA user_version = 1;
"#;
const MIGRATION_V2: &str = r#"
ALTER TABLE scrobble_outbox ADD COLUMN track_title TEXT;
ALTER TABLE scrobble_outbox ADD COLUMN track_artist TEXT;
ALTER TABLE scrobble_outbox ADD COLUMN track_album TEXT;
ALTER TABLE scrobble_outbox ADD COLUMN source_id TEXT;
ALTER TABLE scrobble_outbox ADD COLUMN started_at TEXT;

UPDATE scrobble_outbox
SET track_title = json_extract(payload, '$.track.title'),
    track_artist = json_extract(payload, '$.track.artist'),
    track_album = json_extract(payload, '$.track.album'),
    source_id = json_extract(payload, '$.track.source_id'),
    started_at = json_extract(payload, '$.started_at');

CREATE INDEX IF NOT EXISTS idx_outbox_activity
    ON scrobble_outbox (account_id, started_at DESC, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_outbox_activity_status
    ON scrobble_outbox (account_id, status, started_at DESC);

PRAGMA user_version = 2;
"#;
const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("another Scrobble Bridge runtime is already using this data directory")]
    InstanceInUse,
    #[error("database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("stored data is invalid: {0}")]
    InvalidData(#[from] serde_json::Error),
    #[error("invalid stored timestamp: {0}")]
    InvalidTimestamp(#[from] chrono::ParseError),
    #[error("storage lock is poisoned")]
    LockPoisoned,
}

#[derive(Debug)]
pub struct Storage {
    connection: Mutex<Connection>,
    _instance_lock: Option<File>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ActivityPage {
    pub items: Vec<OutboxEntry>,
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoredSyncState {
    pub paused: bool,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        let mut lock_name = path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock_path = PathBuf::from(lock_name);
        let instance_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        match instance_lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(StorageError::InstanceInUse),
            Err(TryLockError::Error(error)) => return Err(StorageError::Io(error)),
        }
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        Self::from_connection(connection, Some(instance_lock))
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?, None)
    }

    fn from_connection(
        connection: Connection,
        instance_lock: Option<File>,
    ) -> Result<Self, StorageError> {
        let schema_version: u32 =
            connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema_version > SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema {
                found: schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        if schema_version == 0 {
            connection.execute_batch(MIGRATION_V1)?;
        }
        let schema_version: u32 =
            connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema_version == 1 {
            connection.execute_batch(MIGRATION_V2)?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
            _instance_lock: instance_lock,
        })
    }

    pub fn load_snapshot(&self, account_id: &str) -> Result<Option<HistorySnapshot>, StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload FROM history_snapshots WHERE account_id = ?1",
                [account_id],
                |row| row.get(0),
            )
            .optional()?;

        payload
            .map(|payload| serde_json::from_str(&payload).map_err(StorageError::from))
            .transpose()
    }

    /// Atomically advances the source snapshot and inserts newly inferred plays.
    /// A unique fingerprint makes repeating the same transaction harmless.
    pub fn store_snapshot_and_enqueue(
        &self,
        snapshot: &HistorySnapshot,
        candidates: &[ScrobbleCandidate],
    ) -> Result<usize, StorageError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let now = Utc::now();
        let mut inserted = 0;

        for candidate in candidates {
            let changed = transaction.execute(
                r#"
                INSERT OR IGNORE INTO scrobble_outbox (
                    id, fingerprint, account_id, payload, status, attempt_count,
                    next_attempt_at, created_at, updated_at, track_title,
                    track_artist, track_album, source_id, started_at
                ) VALUES (
                    ?1, ?2, ?3, ?4, 'pending', 0, ?5, ?5, ?5,
                    ?6, ?7, ?8, ?9, ?10
                )
                "#,
                params![
                    candidate.id.to_string(),
                    candidate.fingerprint,
                    candidate.account_id,
                    serde_json::to_string(candidate)?,
                    timestamp(now),
                    candidate.track.title,
                    candidate.track.artist,
                    candidate.track.album,
                    candidate.track.source_id,
                    timestamp(candidate.started_at),
                ],
            )?;
            inserted += changed;
        }

        upsert_snapshot(&transaction, snapshot, now)?;
        transaction.commit()?;
        Ok(inserted)
    }

    pub fn due_outbox(
        &self,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<OutboxEntry>, StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut statement = connection.prepare(
            r#"
            SELECT payload, status, attempt_count, next_attempt_at,
                   last_error_code, created_at, updated_at
            FROM scrobble_outbox
            WHERE status IN ('pending', 'retryable') AND next_attempt_at <= ?1
            ORDER BY created_at ASC
            LIMIT ?2
            "#,
        )?;

        let sql_limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = statement.query_map(params![timestamp(now), sql_limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (
                payload,
                status,
                attempt_count,
                next_attempt_at,
                last_error_code,
                created_at,
                updated_at,
            ) = row?;
            entries.push(OutboxEntry {
                candidate: serde_json::from_str(&payload)?,
                status: parse_status(&status)?,
                attempt_count,
                next_attempt_at: parse_timestamp(&next_attempt_at)?,
                last_error_code,
                created_at: parse_timestamp(&created_at)?,
                updated_at: parse_timestamp(&updated_at)?,
            });
        }
        Ok(entries)
    }

    /// Returns a bounded activity page for UI history views. Search is applied
    /// to materialized title/artist columns so large libraries do not require
    /// deserializing every stored candidate in application memory.
    pub fn activity_page(
        &self,
        account_id: &str,
        limit: usize,
        offset: usize,
        search: Option<&str>,
        status: Option<OutboxStatus>,
    ) -> Result<ActivityPage, StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let limit = limit.clamp(1, 200);
        let sql_limit = i64::try_from(limit).unwrap_or(200);
        let sql_offset = i64::try_from(offset).unwrap_or(i64::MAX);
        let status = status.map(status_name);
        let search = search
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", escape_like(&value.to_lowercase())));

        let where_clause = r#"
            account_id = ?1
            AND (?2 IS NULL OR status = ?2)
            AND (
                ?3 IS NULL
                OR lower(COALESCE(track_title, '')) LIKE ?3 ESCAPE '\'
                OR lower(COALESCE(track_artist, '')) LIKE ?3 ESCAPE '\'
            )
        "#;
        let total: i64 = connection.query_row(
            &format!("SELECT COUNT(*) FROM scrobble_outbox WHERE {where_clause}"),
            params![account_id, status, search],
            |row| row.get(0),
        )?;

        let mut statement = connection.prepare(&format!(
            r#"
            SELECT payload, status, attempt_count, next_attempt_at,
                   last_error_code, created_at, updated_at
            FROM scrobble_outbox
            WHERE {where_clause}
            ORDER BY COALESCE(started_at, created_at) DESC, created_at DESC
            LIMIT ?4 OFFSET ?5
            "#
        ))?;
        let rows = statement.query_map(
            params![account_id, status, search, sql_limit, sql_offset],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )?;

        let mut items = Vec::new();
        for row in rows {
            let (
                payload,
                status,
                attempt_count,
                next_attempt_at,
                last_error_code,
                created_at,
                updated_at,
            ) = row?;
            items.push(OutboxEntry {
                candidate: serde_json::from_str(&payload)?,
                status: parse_status(&status)?,
                attempt_count,
                next_attempt_at: parse_timestamp(&next_attempt_at)?,
                last_error_code,
                created_at: parse_timestamp(&created_at)?,
                updated_at: parse_timestamp(&updated_at)?,
            });
        }

        Ok(ActivityPage {
            items,
            total: u64::try_from(total).unwrap_or_default(),
            limit,
            offset,
        })
    }

    pub fn mark_submitting(&self, ids: &[Uuid]) -> Result<usize, StorageError> {
        self.update_many(ids, |transaction, id, now| {
            transaction.execute(
                r#"
                UPDATE scrobble_outbox
                SET status = 'submitting', attempt_count = attempt_count + 1,
                    updated_at = ?2
                WHERE id = ?1 AND status IN ('pending', 'retryable')
                "#,
                params![id.to_string(), timestamp(now)],
            )
        })
    }

    pub fn mark_accepted(&self, id: Uuid) -> Result<bool, StorageError> {
        self.update_status(id, "accepted", None, Utc::now())
    }

    pub fn mark_accepted_existing(&self, id: Uuid) -> Result<bool, StorageError> {
        self.update_status(id, "accepted", Some("matched_recent_track"), Utc::now())
    }

    pub fn mark_retryable(
        &self,
        id: Uuid,
        error_code: &str,
        next_attempt_at: DateTime<Utc>,
    ) -> Result<bool, StorageError> {
        self.update_status(id, "retryable", Some(error_code), next_attempt_at)
    }

    pub fn mark_rejected(&self, id: Uuid, error_code: &str) -> Result<bool, StorageError> {
        self.update_status(id, "rejected", Some(error_code), Utc::now())
    }

    /// Makes only the matching recoverable provider failures eligible again.
    /// Reauthorization must not leave accepted, rejected, or unrelated retry
    /// entries in a different state.
    pub fn expedite_retryable_failures(
        &self,
        error_code: &str,
        now: DateTime<Utc>,
    ) -> Result<usize, StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        Ok(connection.execute(
            r#"
            UPDATE scrobble_outbox
            SET next_attempt_at = ?1, updated_at = ?1
            WHERE status = 'retryable' AND last_error_code = ?2
            "#,
            params![timestamp(now), error_code],
        )?)
    }

    /// Returns every in-flight item to the retry queue when a new runtime opens
    /// the database. A single data directory must never be used by two runtimes.
    pub fn recover_interrupted_submissions(
        &self,
        now: DateTime<Utc>,
    ) -> Result<usize, StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        Ok(connection.execute(
            r#"
            UPDATE scrobble_outbox
            SET status = 'retryable', next_attempt_at = ?1,
                last_error_code = 'interrupted_submission', updated_at = ?1
            WHERE status = 'submitting'
            "#,
            [timestamp(now)],
        )?)
    }

    pub fn outbox_count(&self, status: OutboxStatus) -> Result<u64, StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM scrobble_outbox WHERE status = ?1",
            [status_name(status)],
            |row| row.get(0),
        )?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    pub fn sync_state(&self, account_id: &str) -> Result<StoredSyncState, StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let stored: Option<(Option<String>, Option<String>, Option<String>)> = connection
            .query_row(
                r#"
                SELECT paused_reason, last_attempt_at, last_success_at
                FROM sync_state
                WHERE account_id = ?1
                "#,
                [account_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((paused_reason, last_attempt_at, last_success_at)) = stored else {
            return Ok(StoredSyncState::default());
        };
        Ok(StoredSyncState {
            paused: paused_reason.is_some(),
            last_attempt_at: last_attempt_at
                .as_deref()
                .map(parse_timestamp)
                .transpose()?,
            last_success_at: last_success_at
                .as_deref()
                .map(parse_timestamp)
                .transpose()?,
        })
    }

    pub fn is_paused(&self, account_id: &str) -> Result<bool, StorageError> {
        Ok(self.sync_state(account_id)?.paused)
    }

    pub fn set_paused(&self, account_id: &str, paused: bool) -> Result<(), StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let now = timestamp(Utc::now());
        connection.execute(
            r#"
            INSERT INTO sync_state (account_id, paused_reason, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(account_id) DO UPDATE SET
                paused_reason = excluded.paused_reason,
                updated_at = excluded.updated_at
            "#,
            params![account_id, paused.then_some("user"), now],
        )?;
        Ok(())
    }

    pub fn mark_sync_attempt(
        &self,
        account_id: &str,
        attempted_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let attempted_at = timestamp(attempted_at);
        connection.execute(
            r#"
            INSERT INTO sync_state (account_id, last_attempt_at, updated_at)
            VALUES (?1, ?2, ?2)
            ON CONFLICT(account_id) DO UPDATE SET
                last_attempt_at = excluded.last_attempt_at,
                updated_at = excluded.updated_at
            "#,
            params![account_id, attempted_at],
        )?;
        Ok(())
    }

    pub fn mark_sync_success(
        &self,
        account_id: &str,
        succeeded_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let succeeded_at = timestamp(succeeded_at);
        connection.execute(
            r#"
            INSERT INTO sync_state (
                account_id, last_attempt_at, last_success_at, updated_at
            ) VALUES (?1, ?2, ?2, ?2)
            ON CONFLICT(account_id) DO UPDATE SET
                last_attempt_at = excluded.last_attempt_at,
                last_success_at = excluded.last_success_at,
                updated_at = excluded.updated_at
            "#,
            params![account_id, succeeded_at],
        )?;
        Ok(())
    }

    /// Atomically accepts a device nonce once and removes expired replay records.
    pub fn claim_device_nonce(
        &self,
        device_id: &str,
        nonce: &str,
        now: DateTime<Utc>,
        cutoff: DateTime<Utc>,
    ) -> Result<bool, StorageError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM device_nonces WHERE seen_at < ?1",
            [timestamp(cutoff)],
        )?;
        let changed = transaction.execute(
            r#"
            INSERT OR IGNORE INTO device_nonces (device_id, nonce, seen_at)
            VALUES (?1, ?2, ?3)
            "#,
            params![device_id, nonce, timestamp(now)],
        )?;
        transaction.commit()?;
        Ok(changed == 1)
    }

    /// Creates a consistent SQLite backup without copying a live WAL file.
    pub fn backup_to(&self, path: impl AsRef<Path>) -> Result<(), StorageError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                StorageError::Database(rusqlite::Error::ToSqlConversionFailure(error.into()))
            })?;
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        connection.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;
        Ok(())
    }

    pub fn checkpoint(&self) -> Result<(), StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    fn update_status(
        &self,
        id: Uuid,
        status: &str,
        error_code: Option<&str>,
        next_attempt_at: DateTime<Utc>,
    ) -> Result<bool, StorageError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let changed = connection.execute(
            r#"
            UPDATE scrobble_outbox
            SET status = ?2, next_attempt_at = ?3, last_error_code = ?4,
                updated_at = ?5
            WHERE id = ?1
            "#,
            params![
                id.to_string(),
                status,
                timestamp(next_attempt_at),
                error_code,
                timestamp(Utc::now()),
            ],
        )?;
        Ok(changed == 1)
    }

    fn update_many(
        &self,
        ids: &[Uuid],
        mut update: impl FnMut(&Transaction<'_>, Uuid, DateTime<Utc>) -> rusqlite::Result<usize>,
    ) -> Result<usize, StorageError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let now = Utc::now();
        let mut changed = 0;
        for id in ids {
            changed += update(&transaction, *id, now)?;
        }
        transaction.commit()?;
        Ok(changed)
    }
}

fn upsert_snapshot(
    transaction: &Transaction<'_>,
    snapshot: &HistorySnapshot,
    now: DateTime<Utc>,
) -> Result<(), StorageError> {
    transaction.execute(
        r#"
        INSERT INTO history_snapshots (account_id, observed_at, payload, updated_at)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(account_id) DO UPDATE SET
            observed_at = excluded.observed_at,
            payload = excluded.payload,
            updated_at = excluded.updated_at
        "#,
        params![
            snapshot.account_id,
            timestamp(snapshot.observed_at),
            serde_json::to_string(snapshot)?,
            timestamp(now),
        ],
    )?;
    Ok(())
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, StorageError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn status_name(status: OutboxStatus) -> &'static str {
    match status {
        OutboxStatus::Pending => "pending",
        OutboxStatus::Submitting => "submitting",
        OutboxStatus::Accepted => "accepted",
        OutboxStatus::Retryable => "retryable",
        OutboxStatus::Rejected => "rejected",
    }
}

fn parse_status(value: &str) -> Result<OutboxStatus, StorageError> {
    match value {
        "pending" => Ok(OutboxStatus::Pending),
        "submitting" => Ok(OutboxStatus::Submitting),
        "accepted" => Ok(OutboxStatus::Accepted),
        "retryable" => Ok(OutboxStatus::Retryable),
        "rejected" => Ok(OutboxStatus::Rejected),
        other => Err(StorageError::InvalidData(serde_json::Error::io(
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown outbox status: {other}"),
            ),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use scrobble_core::{HistoryItem, Track, infer_scrobble_candidates};

    use super::*;

    fn snapshot() -> HistorySnapshot {
        HistorySnapshot {
            account_id: "account".to_owned(),
            observed_at: Utc.timestamp_opt(1_700_001_000, 0).unwrap(),
            items: vec![HistoryItem {
                track: Track {
                    duration_seconds: Some(180),
                    ..Track::new(Some("video".to_owned()), "Song", "Artist")
                },
                source_position: 0,
                played_text: Some("Just now".to_owned()),
            }],
        }
    }

    #[test]
    fn snapshot_and_outbox_are_committed_together() {
        let storage = Storage::open_in_memory().unwrap();
        let snapshot = snapshot();
        let candidates =
            infer_scrobble_candidates("account", &snapshot.items, snapshot.observed_at);

        assert_eq!(
            storage
                .store_snapshot_and_enqueue(&snapshot, &candidates)
                .unwrap(),
            1
        );
        assert_eq!(storage.load_snapshot("account").unwrap(), Some(snapshot));
        assert_eq!(storage.outbox_count(OutboxStatus::Pending).unwrap(), 1);
    }

    #[test]
    fn fingerprint_makes_repeated_enqueue_idempotent() {
        let storage = Storage::open_in_memory().unwrap();
        let snapshot = snapshot();
        let candidates =
            infer_scrobble_candidates("account", &snapshot.items, snapshot.observed_at);

        assert_eq!(
            storage
                .store_snapshot_and_enqueue(&snapshot, &candidates)
                .unwrap(),
            1
        );
        assert_eq!(
            storage
                .store_snapshot_and_enqueue(&snapshot, &candidates)
                .unwrap(),
            0
        );
    }

    #[test]
    fn retryable_item_is_hidden_until_due() {
        let storage = Storage::open_in_memory().unwrap();
        let snapshot = snapshot();
        let candidates =
            infer_scrobble_candidates("account", &snapshot.items, snapshot.observed_at);
        storage
            .store_snapshot_and_enqueue(&snapshot, &candidates)
            .unwrap();

        let id = candidates[0].id;
        storage.mark_submitting(&[id]).unwrap();
        storage
            .mark_retryable(id, "temporary", Utc::now() + Duration::minutes(10))
            .unwrap();

        assert!(storage.due_outbox(Utc::now(), 50).unwrap().is_empty());
        assert_eq!(
            storage
                .due_outbox(Utc::now() + Duration::minutes(11), 50)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn reauthorization_expedites_only_matching_provider_failures() {
        let storage = Storage::open_in_memory().unwrap();
        let snapshot = snapshot();
        let mut candidates =
            infer_scrobble_candidates("account", &snapshot.items, snapshot.observed_at);
        let mut unrelated = candidates[0].clone();
        unrelated.id = Uuid::new_v4();
        unrelated.fingerprint = "unrelated-temporary-failure".to_owned();
        candidates.push(unrelated);
        storage
            .store_snapshot_and_enqueue(&snapshot, &candidates)
            .unwrap();
        storage
            .mark_retryable(
                candidates[0].id,
                "lastfm_auth",
                Utc::now() + Duration::hours(24),
            )
            .unwrap();
        storage
            .mark_retryable(
                candidates[1].id,
                "lastfm_temporary",
                Utc::now() + Duration::hours(1),
            )
            .unwrap();

        let now = Utc::now();
        assert_eq!(
            storage
                .expedite_retryable_failures("lastfm_auth", now)
                .unwrap(),
            1
        );
        let due = storage.due_outbox(now, 50).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].candidate.id, candidates[0].id);
        assert_eq!(storage.outbox_count(OutboxStatus::Retryable).unwrap(), 2);
    }

    #[test]
    fn accepted_item_leaves_due_queue() {
        let storage = Storage::open_in_memory().unwrap();
        let snapshot = snapshot();
        let candidates =
            infer_scrobble_candidates("account", &snapshot.items, snapshot.observed_at);
        storage
            .store_snapshot_and_enqueue(&snapshot, &candidates)
            .unwrap();

        assert!(storage.mark_accepted(candidates[0].id).unwrap());
        assert_eq!(storage.outbox_count(OutboxStatus::Accepted).unwrap(), 1);
        assert!(
            storage
                .due_outbox(Utc::now() + Duration::days(1), 50)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn interrupted_submission_is_due_immediately_after_restart_recovery() {
        let storage = Storage::open_in_memory().unwrap();
        let snapshot = snapshot();
        let candidates =
            infer_scrobble_candidates("account", &snapshot.items, snapshot.observed_at);
        storage
            .store_snapshot_and_enqueue(&snapshot, &candidates)
            .unwrap();
        storage.mark_submitting(&[candidates[0].id]).unwrap();

        let now = Utc::now();
        assert_eq!(storage.recover_interrupted_submissions(now).unwrap(), 1);
        assert_eq!(storage.due_outbox(now, 50).unwrap().len(), 1);
    }

    #[test]
    fn activity_history_is_paged_searchable_and_status_filtered() {
        let storage = Storage::open_in_memory().unwrap();
        let snapshot = snapshot();
        let mut candidates =
            infer_scrobble_candidates("account", &snapshot.items, snapshot.observed_at);
        let mut newer = candidates[0].clone();
        newer.id = Uuid::new_v4();
        newer.fingerprint = "newer-fingerprint".to_owned();
        newer.track.title = "Midnight City".to_owned();
        newer.track.artist = "M83".to_owned();
        newer.started_at += Duration::minutes(5);
        candidates.push(newer.clone());

        storage
            .store_snapshot_and_enqueue(&snapshot, &candidates)
            .unwrap();
        storage.mark_accepted(newer.id).unwrap();

        let first_page = storage.activity_page("account", 1, 0, None, None).unwrap();
        assert_eq!(first_page.total, 2);
        assert_eq!(first_page.items.len(), 1);
        assert_eq!(first_page.items[0].candidate.track.title, "Midnight City");

        let search = storage
            .activity_page("account", 50, 0, Some("m83"), None)
            .unwrap();
        assert_eq!(search.total, 1);
        assert_eq!(search.items[0].candidate.track.artist, "M83");

        let accepted = storage
            .activity_page("account", 50, 0, None, Some(OutboxStatus::Accepted))
            .unwrap();
        assert_eq!(accepted.total, 1);
        assert_eq!(accepted.items[0].status, OutboxStatus::Accepted);
    }

    #[test]
    fn version_one_database_is_backfilled_for_activity_queries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(MIGRATION_V1).unwrap();
        let snapshot = snapshot();
        let candidate =
            infer_scrobble_candidates("account", &snapshot.items, snapshot.observed_at).remove(0);
        let now = timestamp(Utc::now());
        connection
            .execute(
                r#"
                INSERT INTO scrobble_outbox (
                    id, fingerprint, account_id, payload, status, attempt_count,
                    next_attempt_at, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, 'accepted', 1, ?5, ?5, ?5)
                "#,
                params![
                    candidate.id.to_string(),
                    candidate.fingerprint,
                    candidate.account_id,
                    serde_json::to_string(&candidate).unwrap(),
                    now,
                ],
            )
            .unwrap();
        drop(connection);

        let storage = Storage::open(&path).unwrap();
        let page = storage
            .activity_page("account", 50, 0, Some("song"), None)
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].candidate.track.title, "Song");
    }

    #[test]
    fn device_nonce_is_claimed_only_once() {
        let storage = Storage::open_in_memory().unwrap();
        let now = Utc::now();
        assert!(
            storage
                .claim_device_nonce("device", "nonce", now, now - Duration::minutes(5))
                .unwrap()
        );
        assert!(
            !storage
                .claim_device_nonce("device", "nonce", now, now - Duration::minutes(5))
                .unwrap()
        );
    }

    #[test]
    fn pause_setting_survives_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let attempted_at = Utc.timestamp_opt(1_700_002_000, 0).unwrap();
        let succeeded_at = Utc.timestamp_opt(1_700_002_100, 0).unwrap();
        {
            let storage = Storage::open(&path).unwrap();
            assert!(!storage.is_paused("default").unwrap());
            storage.mark_sync_attempt("default", attempted_at).unwrap();
            storage.mark_sync_success("default", succeeded_at).unwrap();
            storage.set_paused("default", true).unwrap();
            assert!(storage.is_paused("default").unwrap());
        }
        let reopened = Storage::open(&path).unwrap();
        assert_eq!(
            reopened.sync_state("default").unwrap(),
            StoredSyncState {
                paused: true,
                last_attempt_at: Some(succeeded_at),
                last_success_at: Some(succeeded_at),
            }
        );
        reopened.set_paused("default", false).unwrap();
        assert!(!reopened.is_paused("default").unwrap());
    }

    #[test]
    fn backup_is_a_readable_consistent_database() {
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("source.sqlite3");
        let backup_path = directory.path().join("backups/state.sqlite3");
        let storage = Storage::open(&source_path).unwrap();
        storage
            .store_snapshot_and_enqueue(&snapshot(), &[])
            .unwrap();
        storage.backup_to(&backup_path).unwrap();

        let backup = Storage::open(&backup_path).unwrap();
        assert!(backup.load_snapshot("account").unwrap().is_some());
    }

    #[test]
    fn database_directory_allows_only_one_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let first = Storage::open(&path).unwrap();
        assert!(matches!(
            Storage::open(&path),
            Err(StorageError::InstanceInUse)
        ));
        drop(first);
        Storage::open(&path).unwrap();
    }

    #[test]
    fn newer_database_schema_is_rejected_without_downgrading_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(connection);

        assert!(matches!(
            Storage::open(&path),
            Err(StorageError::UnsupportedSchema {
                found: 3,
                supported: 2
            })
        ));

        let connection = Connection::open(&path).unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION + 1);
    }
}
