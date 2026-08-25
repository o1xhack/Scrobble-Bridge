//! Shared synchronization engine used by both desktop and headless runtimes.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use lastfm_client::{
    LastFmClient, LastFmError, LastFmSession, RecentTrack, ScrobbleResult, matches_recent_track,
};
use scrobble_core::{
    HistorySnapshot, OutboxEntry, ReconcileOutcome, ScrobbleCandidate,
    infer_scrobble_candidates_at_indices, reconcile_history,
};
use scrobble_storage::{Storage, StorageError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use ytmusic_client::{BrowserCredentials, YtMusicClient, YtMusicError};

#[derive(Clone, Debug)]
pub struct SyncEngineConfig {
    pub minimum_overlap: usize,
    pub duplicate_time_window: Duration,
    pub recent_lookback: Duration,
    pub batch_size: usize,
}

impl Default for SyncEngineConfig {
    fn default() -> Self {
        Self {
            minimum_overlap: 2,
            duplicate_time_window: Duration::minutes(5),
            recent_lookback: Duration::hours(2),
            batch_size: 50,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureDisposition {
    Retry,
    Pause,
    Reject,
}

#[derive(Clone, Debug, Error)]
#[error("{code}: {message}")]
pub struct ProviderFailure {
    pub code: String,
    pub message: String,
    pub disposition: FailureDisposition,
}

impl ProviderFailure {
    pub fn retry(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            disposition: FailureDisposition::Retry,
        }
    }

    pub fn pause(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            disposition: FailureDisposition::Pause,
        }
    }

    pub fn reject(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            disposition: FailureDisposition::Reject,
        }
    }
}

#[async_trait]
pub trait HistorySource: Send + Sync {
    async fn fetch_history(&self) -> Result<HistorySnapshot, ProviderFailure>;
}

#[async_trait]
pub trait ScrobbleTarget: Send + Sync {
    async fn recent_tracks(
        &self,
        from: Option<DateTime<Utc>>,
        limit: u16,
    ) -> Result<Vec<RecentTrack>, ProviderFailure>;

    async fn submit(
        &self,
        candidates: &[ScrobbleCandidate],
    ) -> Result<Vec<ScrobbleResult>, ProviderFailure>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceOutcome {
    Baseline,
    Delta,
    Gap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncReport {
    pub source_outcome: SourceOutcome,
    pub overlap_matches: usize,
    pub discovered: usize,
    pub enqueued: usize,
    pub matched_existing: usize,
    pub submitted: usize,
    pub accepted: usize,
    pub retryable: usize,
    pub rejected: usize,
    pub gap_best_overlap: Option<usize>,
}

impl SyncReport {
    fn new(source_outcome: SourceOutcome) -> Self {
        Self {
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
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("history provider failed: {0}")]
    History(ProviderFailure),
    #[error("scrobble provider failed before submission: {0}")]
    Target(ProviderFailure),
}

#[derive(Debug)]
pub struct SyncEngine<H, T> {
    storage: Arc<Storage>,
    history: H,
    target: T,
    config: SyncEngineConfig,
}

impl<H, T> SyncEngine<H, T>
where
    H: HistorySource,
    T: ScrobbleTarget,
{
    pub fn new(storage: Arc<Storage>, history: H, target: T, config: SyncEngineConfig) -> Self {
        Self {
            storage,
            history,
            target,
            config,
        }
    }

    pub async fn run_once(&self) -> Result<SyncReport, SyncError> {
        let current = self
            .history
            .fetch_history()
            .await
            .map_err(SyncError::History)?;
        let previous = self.storage.load_snapshot(&current.account_id)?;
        let reconciliation =
            reconcile_history(previous.as_ref(), current, self.config.minimum_overlap);

        let mut report = match reconciliation {
            ReconcileOutcome::Baseline { snapshot } => {
                self.storage.store_snapshot_and_enqueue(&snapshot, &[])?;
                SyncReport::new(SourceOutcome::Baseline)
            }
            ReconcileOutcome::Delta {
                overlap_len,
                new_item_indices,
                snapshot,
            } => {
                let candidates = infer_scrobble_candidates_at_indices(
                    &snapshot.account_id,
                    &snapshot.items,
                    snapshot.observed_at,
                    new_item_indices,
                );
                let mut report = SyncReport::new(SourceOutcome::Delta);
                report.overlap_matches = overlap_len;
                report.discovered = candidates.len();
                report.enqueued = self
                    .storage
                    .store_snapshot_and_enqueue(&snapshot, &candidates)?;
                report
            }
            ReconcileOutcome::Gap {
                best_overlap_len, ..
            } => {
                let mut report = SyncReport::new(SourceOutcome::Gap);
                report.overlap_matches = best_overlap_len;
                report.gap_best_overlap = Some(best_overlap_len);
                report
            }
        };

        self.dispatch_due(&mut report).await?;
        Ok(report)
    }

    async fn dispatch_due(&self, report: &mut SyncReport) -> Result<(), SyncError> {
        let now = Utc::now();
        let due = self
            .storage
            .due_outbox(now, self.config.batch_size.min(50))?;
        if due.is_empty() {
            return Ok(());
        }

        let earliest = due
            .iter()
            .map(|entry| entry.candidate.started_at)
            .min()
            .map(|time| time - self.config.recent_lookback);
        let recent = self
            .target
            .recent_tracks(earliest, 200)
            .await
            .map_err(SyncError::Target)?;
        let (to_submit, attempt_counts) = self.reconcile_due_with_recent(due, &recent, report)?;

        if to_submit.is_empty() {
            return Ok(());
        }

        self.submit_candidates(&to_submit, &attempt_counts, now, report)
            .await
    }

    fn reconcile_due_with_recent(
        &self,
        due: Vec<OutboxEntry>,
        recent: &[RecentTrack],
        report: &mut SyncReport,
    ) -> Result<(Vec<ScrobbleCandidate>, HashMap<Uuid, u32>), StorageError> {
        let mut used_recent = vec![false; recent.len()];
        let mut to_submit = Vec::new();
        let mut attempt_counts = HashMap::new();

        for entry in due {
            let existing_match = recent.iter().enumerate().find(|(index, track)| {
                !used_recent[*index]
                    && matches_recent_track(
                        &entry.candidate,
                        track,
                        self.config.duplicate_time_window,
                    )
            });
            if let Some((index, _)) = existing_match {
                used_recent[index] = true;
                self.storage.mark_accepted_existing(entry.candidate.id)?;
                report.matched_existing += 1;
                report.accepted += 1;
            } else {
                attempt_counts.insert(entry.candidate.id, entry.attempt_count + 1);
                to_submit.push(entry.candidate);
            }
        }

        Ok((to_submit, attempt_counts))
    }

    async fn submit_candidates(
        &self,
        to_submit: &[ScrobbleCandidate],
        attempt_counts: &HashMap<Uuid, u32>,
        now: DateTime<Utc>,
        report: &mut SyncReport,
    ) -> Result<(), SyncError> {
        self.storage.mark_submitting(
            &to_submit
                .iter()
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>(),
        )?;
        report.submitted = to_submit.len();

        match self.target.submit(to_submit).await {
            Ok(results) => {
                let by_id = results
                    .into_iter()
                    .map(|result| (result.candidate_id, result))
                    .collect::<HashMap<_, _>>();
                for candidate in to_submit {
                    match by_id.get(&candidate.id) {
                        Some(result) if result.accepted => {
                            self.storage.mark_accepted(candidate.id)?;
                            report.accepted += 1;
                        }
                        Some(result) => {
                            let code = result.ignored_code.map_or_else(
                                || "ignored".to_owned(),
                                |code| format!("ignored_{code}"),
                            );
                            self.storage.mark_rejected(candidate.id, &code)?;
                            report.rejected += 1;
                        }
                        None => {
                            let attempt = attempt_counts[&candidate.id];
                            self.storage.mark_retryable(
                                candidate.id,
                                "missing_submission_result",
                                now + retry_delay(attempt, candidate.id),
                            )?;
                            report.retryable += 1;
                        }
                    }
                }
            }
            Err(failure) => {
                for candidate in to_submit {
                    match failure.disposition {
                        FailureDisposition::Retry | FailureDisposition::Pause => {
                            let attempt = attempt_counts[&candidate.id];
                            let delay = if failure.disposition == FailureDisposition::Pause {
                                Duration::hours(24)
                            } else {
                                retry_delay(attempt, candidate.id)
                            };
                            self.storage.mark_retryable(
                                candidate.id,
                                &failure.code,
                                now + delay,
                            )?;
                            report.retryable += 1;
                        }
                        FailureDisposition::Reject => {
                            self.storage.mark_rejected(candidate.id, &failure.code)?;
                            report.rejected += 1;
                        }
                    }
                }
                return Err(SyncError::Target(failure));
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct YtMusicHistorySource {
    pub client: YtMusicClient,
    pub account_id: String,
    pub credentials: BrowserCredentials,
}

#[async_trait]
impl HistorySource for YtMusicHistorySource {
    async fn fetch_history(&self) -> Result<HistorySnapshot, ProviderFailure> {
        self.client
            .fetch_history(&self.account_id, &self.credentials)
            .await
            .map_err(|error| history_failure(&error))
    }
}

#[derive(Clone, Debug)]
pub struct LastFmScrobbleTarget {
    pub client: LastFmClient,
    pub session: LastFmSession,
}

#[async_trait]
impl ScrobbleTarget for LastFmScrobbleTarget {
    async fn recent_tracks(
        &self,
        from: Option<DateTime<Utc>>,
        limit: u16,
    ) -> Result<Vec<RecentTrack>, ProviderFailure> {
        self.client
            .recent_tracks(&self.session, from, limit)
            .await
            .map_err(|error| lastfm_failure(&error))
    }

    async fn submit(
        &self,
        candidates: &[ScrobbleCandidate],
    ) -> Result<Vec<ScrobbleResult>, ProviderFailure> {
        self.client
            .scrobble(&self.session, candidates)
            .await
            .map_err(|error| lastfm_failure(&error))
    }
}

fn history_failure(error: &YtMusicError) -> ProviderFailure {
    match error {
        YtMusicError::MissingSapisid
        | YtMusicError::CookieTooLarge
        | YtMusicError::InvalidDelegatedSession => {
            ProviderFailure::pause("ytmusic_auth", error.to_string())
        }
        YtMusicError::ApiStatus { status, .. } if matches!(status.as_u16(), 401 | 403) => {
            ProviderFailure::pause("ytmusic_auth", error.to_string())
        }
        YtMusicError::UnrecognizedHistory { .. } => {
            ProviderFailure::pause("ytmusic_schema", error.to_string())
        }
        _ => ProviderFailure::retry("ytmusic_unavailable", error.to_string()),
    }
}

fn lastfm_failure(error: &LastFmError) -> ProviderFailure {
    match error {
        LastFmError::Api {
            code: 4 | 9 | 10 | 13,
            ..
        } => ProviderFailure::pause("lastfm_auth", error.to_string()),
        _ if error.is_retryable() => ProviderFailure::retry("lastfm_temporary", error.to_string()),
        _ => ProviderFailure::reject("lastfm_permanent", error.to_string()),
    }
}

fn retry_delay(attempt: u32, id: Uuid) -> Duration {
    let exponent = attempt.saturating_sub(1).min(10);
    let base_seconds = 30_i64.saturating_mul(1_i64 << exponent).min(21_600);
    let jitter = i64::from(id.as_bytes()[0] % 31);
    Duration::seconds(base_seconds + jitter)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::TimeZone;
    use scrobble_core::{HistoryItem, OutboxStatus, Track};

    use super::*;

    #[derive(Debug)]
    struct FakeHistory {
        snapshots: Mutex<Vec<HistorySnapshot>>,
    }

    #[derive(Debug)]
    struct RecoveringHistory {
        snapshots: Mutex<Vec<Result<HistorySnapshot, ProviderFailure>>>,
    }

    #[async_trait]
    impl HistorySource for FakeHistory {
        async fn fetch_history(&self) -> Result<HistorySnapshot, ProviderFailure> {
            Ok(self.snapshots.lock().unwrap().remove(0))
        }
    }

    #[async_trait]
    impl HistorySource for RecoveringHistory {
        async fn fetch_history(&self) -> Result<HistorySnapshot, ProviderFailure> {
            self.snapshots.lock().unwrap().remove(0)
        }
    }

    #[derive(Debug, Default)]
    struct FakeTarget {
        recent: Mutex<Vec<RecentTrack>>,
        submissions: Mutex<Vec<Vec<ScrobbleCandidate>>>,
        failure: Mutex<Option<ProviderFailure>>,
    }

    #[async_trait]
    impl ScrobbleTarget for FakeTarget {
        async fn recent_tracks(
            &self,
            _from: Option<DateTime<Utc>>,
            _limit: u16,
        ) -> Result<Vec<RecentTrack>, ProviderFailure> {
            Ok(self.recent.lock().unwrap().clone())
        }

        async fn submit(
            &self,
            candidates: &[ScrobbleCandidate],
        ) -> Result<Vec<ScrobbleResult>, ProviderFailure> {
            self.submissions.lock().unwrap().push(candidates.to_vec());
            if let Some(failure) = self.failure.lock().unwrap().take() {
                return Err(failure);
            }
            Ok(candidates
                .iter()
                .map(|candidate| ScrobbleResult {
                    candidate_id: candidate.id,
                    accepted: true,
                    ignored_code: None,
                    ignored_message: None,
                })
                .collect())
        }
    }

    fn snapshot(ids: &[&str], observed_at: DateTime<Utc>) -> HistorySnapshot {
        HistorySnapshot {
            account_id: "account".to_owned(),
            observed_at,
            items: ids
                .iter()
                .enumerate()
                .map(|(index, id)| HistoryItem {
                    track: Track {
                        duration_seconds: Some(180),
                        ..Track::new(Some((*id).to_owned()), *id, "Artist")
                    },
                    source_position: u32::try_from(index).unwrap(),
                    played_text: None,
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn first_run_is_baseline_and_does_not_submit() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let history = FakeHistory {
            snapshots: Mutex::new(vec![snapshot(&["a", "b"], Utc::now())]),
        };
        let target = FakeTarget::default();
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            target,
            SyncEngineConfig::default(),
        );

        let report = engine.run_once().await.unwrap();
        assert_eq!(report.source_outcome, SourceOutcome::Baseline);
        assert_eq!(report.submitted, 0);
        assert_eq!(storage.outbox_count(OutboxStatus::Pending).unwrap(), 0);
    }

    #[tokio::test]
    async fn delta_is_enqueued_and_submitted_once() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let base_time = Utc.timestamp_opt(1_700_001_000, 0).unwrap();
        let baseline = snapshot(&["a", "b"], base_time);
        storage.store_snapshot_and_enqueue(&baseline, &[]).unwrap();
        let history = FakeHistory {
            snapshots: Mutex::new(vec![snapshot(
                &["a", "b", "c"],
                base_time + Duration::minutes(3),
            )]),
        };
        let target = FakeTarget::default();
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            target,
            SyncEngineConfig::default(),
        );

        let report = engine.run_once().await.unwrap();
        assert_eq!(report.discovered, 1);
        assert_eq!(report.enqueued, 1);
        assert_eq!(report.submitted, 1);
        assert_eq!(report.accepted, 1);
        assert_eq!(storage.outbox_count(OutboxStatus::Accepted).unwrap(), 1);
    }

    #[tokio::test]
    async fn late_insert_and_new_suffix_are_recovered_with_full_window_times() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let base_time = Utc.timestamp_opt(1_700_001_000, 0).unwrap();
        let baseline = snapshot(
            &["motto", "suddenly", "gate", "my-pace-official"],
            base_time,
        );
        storage.store_snapshot_and_enqueue(&baseline, &[]).unwrap();
        let current = snapshot(
            &[
                "suddenly",
                "gate",
                "my-pace-video",
                "my-pace-official",
                "lightning",
                "sound-wave",
            ],
            base_time + Duration::minutes(9),
        );
        let history = FakeHistory {
            snapshots: Mutex::new(vec![current.clone()]),
        };
        let target = FakeTarget::default();
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            target,
            SyncEngineConfig::default(),
        );

        let report = engine.run_once().await.unwrap();
        assert_eq!(report.source_outcome, SourceOutcome::Delta);
        assert_eq!(report.overlap_matches, 3);
        assert_eq!(report.discovered, 3);
        assert_eq!(report.accepted, 3);

        let activity = storage.activity_page("account", 10, 0, None, None).unwrap();
        assert_eq!(activity.total, 3);
        let by_title = activity
            .items
            .iter()
            .map(|entry| {
                (
                    entry.candidate.track.title.as_str(),
                    entry.candidate.started_at,
                )
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(
            by_title["my-pace-video"],
            current.observed_at - Duration::minutes(12)
        );
        assert_eq!(storage.load_snapshot("account").unwrap().unwrap(), current);
    }

    #[tokio::test]
    async fn recent_track_is_consumed_instead_of_resubmitted() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let base_time = Utc.timestamp_opt(1_700_001_000, 0).unwrap();
        let baseline = snapshot(&["a", "b"], base_time);
        storage.store_snapshot_and_enqueue(&baseline, &[]).unwrap();
        let current = snapshot(&["a", "b", "c"], base_time + Duration::minutes(3));
        let expected_start = current.observed_at - Duration::seconds(180);
        let history = FakeHistory {
            snapshots: Mutex::new(vec![current]),
        };
        let target = FakeTarget {
            recent: Mutex::new(vec![RecentTrack {
                title: "c".to_owned(),
                artist: "Artist".to_owned(),
                played_at: Some(expected_start),
                now_playing: false,
            }]),
            ..FakeTarget::default()
        };
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            target,
            SyncEngineConfig::default(),
        );

        let report = engine.run_once().await.unwrap();
        assert_eq!(report.matched_existing, 1);
        assert_eq!(report.submitted, 0);
        assert_eq!(storage.outbox_count(OutboxStatus::Accepted).unwrap(), 1);
    }

    #[tokio::test]
    async fn retryable_failure_returns_item_to_due_later() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let base_time = Utc::now();
        let baseline = snapshot(&["a", "b"], base_time);
        storage.store_snapshot_and_enqueue(&baseline, &[]).unwrap();
        let history = FakeHistory {
            snapshots: Mutex::new(vec![snapshot(
                &["a", "b", "c"],
                base_time + Duration::minutes(3),
            )]),
        };
        let target = FakeTarget {
            failure: Mutex::new(Some(ProviderFailure::retry("offline", "temporary"))),
            ..FakeTarget::default()
        };
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            target,
            SyncEngineConfig::default(),
        );

        assert!(matches!(
            engine.run_once().await,
            Err(SyncError::Target(ProviderFailure {
                disposition: FailureDisposition::Retry,
                ..
            }))
        ));
        assert_eq!(storage.outbox_count(OutboxStatus::Retryable).unwrap(), 1);
        assert!(storage.due_outbox(Utc::now(), 50).unwrap().is_empty());
    }

    #[test]
    fn backoff_is_bounded_and_has_deterministic_jitter() {
        let id = Uuid::from_bytes([255; 16]);
        assert_eq!(retry_delay(1, id), Duration::seconds(37));
        assert_eq!(retry_delay(30, id), Duration::seconds(21_607));
    }

    #[test]
    fn expired_or_invalid_youtube_credentials_require_reauthorization() {
        for error in [
            YtMusicError::MissingSapisid,
            YtMusicError::CookieTooLarge,
            YtMusicError::InvalidDelegatedSession,
            YtMusicError::ApiStatus {
                status: reqwest::StatusCode::UNAUTHORIZED,
                summary: "expired browser session".to_owned(),
            },
            YtMusicError::ApiStatus {
                status: reqwest::StatusCode::FORBIDDEN,
                summary: "invalid account".to_owned(),
            },
        ] {
            let failure = history_failure(&error);
            assert_eq!(failure.code, "ytmusic_auth");
            assert_eq!(failure.disposition, FailureDisposition::Pause);
        }
    }

    #[test]
    fn youtube_network_failures_retry_but_schema_changes_fail_closed() {
        let offline = history_failure(&YtMusicError::Transport("offline".to_owned()));
        assert_eq!(offline.code, "ytmusic_unavailable");
        assert_eq!(offline.disposition, FailureDisposition::Retry);

        let schema = history_failure(&YtMusicError::UnrecognizedHistory {
            summary: "no recognized track renderers".to_owned(),
        });
        assert_eq!(schema.code, "ytmusic_schema");
        assert_eq!(schema.disposition, FailureDisposition::Pause);
    }

    #[test]
    fn lastfm_invalid_sessions_require_reauthorization() {
        for code in [4, 9, 10, 13] {
            let failure = lastfm_failure(&LastFmError::Api {
                code,
                message: "session is invalid".to_owned(),
            });

            assert_eq!(failure.code, "lastfm_auth");
            assert_eq!(failure.disposition, FailureDisposition::Pause);
        }
    }

    #[test]
    fn lastfm_rate_limits_and_server_errors_retry_without_rejection() {
        for status in [
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
        ] {
            let failure = lastfm_failure(&LastFmError::HttpStatus {
                status,
                summary: "temporarily unavailable".to_owned(),
            });

            assert_eq!(failure.code, "lastfm_temporary");
            assert_eq!(failure.disposition, FailureDisposition::Retry);
        }
    }

    #[tokio::test]
    async fn interrupted_submission_is_reconciled_after_restart_without_duplicate() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let base_time = Utc::now() - Duration::minutes(6);
        let baseline = snapshot(&["a", "b"], base_time);
        storage.store_snapshot_and_enqueue(&baseline, &[]).unwrap();
        let current = snapshot(&["a", "b", "c"], base_time + Duration::minutes(3));
        let candidates = infer_scrobble_candidates_at_indices(
            &current.account_id,
            &current.items,
            current.observed_at,
            vec![2],
        );
        storage
            .store_snapshot_and_enqueue(&current, &candidates)
            .unwrap();
        storage.mark_submitting(&[candidates[0].id]).unwrap();
        assert_eq!(
            storage.recover_interrupted_submissions(Utc::now()).unwrap(),
            1
        );

        let history = FakeHistory {
            snapshots: Mutex::new(vec![current]),
        };
        let target = FakeTarget {
            recent: Mutex::new(vec![RecentTrack {
                title: "c".to_owned(),
                artist: "Artist".to_owned(),
                played_at: Some(candidates[0].started_at),
                now_playing: false,
            }]),
            ..FakeTarget::default()
        };
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            target,
            SyncEngineConfig::default(),
        );

        let report = engine.run_once().await.unwrap();
        assert_eq!(report.matched_existing, 1);
        assert_eq!(report.submitted, 0);
        assert_eq!(storage.outbox_count(OutboxStatus::Accepted).unwrap(), 1);
        assert_eq!(storage.outbox_count(OutboxStatus::Retryable).unwrap(), 0);
    }

    #[tokio::test]
    async fn unaligned_history_keeps_the_last_safe_snapshot() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let base_time = Utc::now();
        let baseline = snapshot(&["a", "b", "c"], base_time);
        storage.store_snapshot_and_enqueue(&baseline, &[]).unwrap();
        let history = FakeHistory {
            snapshots: Mutex::new(vec![snapshot(
                &["unrelated", "history", "window"],
                base_time + Duration::minutes(20),
            )]),
        };
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            FakeTarget::default(),
            SyncEngineConfig::default(),
        );

        let report = engine.run_once().await.unwrap();
        assert_eq!(report.source_outcome, SourceOutcome::Gap);
        assert_eq!(report.submitted, 0);
        assert_eq!(storage.load_snapshot("account").unwrap(), Some(baseline));
    }

    #[tokio::test]
    async fn refreshed_cookie_recovers_from_expired_history_without_losing_baseline() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let base_time = Utc::now() - Duration::minutes(6);
        let baseline = snapshot(&["a", "b"], base_time);
        storage.store_snapshot_and_enqueue(&baseline, &[]).unwrap();
        let current = snapshot(&["a", "b", "c"], base_time + Duration::minutes(3));
        let history = RecoveringHistory {
            snapshots: Mutex::new(vec![
                Err(ProviderFailure::pause(
                    "ytmusic_auth",
                    "expired browser cookie",
                )),
                Ok(current),
            ]),
        };
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            FakeTarget::default(),
            SyncEngineConfig::default(),
        );

        assert!(matches!(
            engine.run_once().await,
            Err(SyncError::History(ProviderFailure {
                disposition: FailureDisposition::Pause,
                ..
            }))
        ));
        assert_eq!(storage.load_snapshot("account").unwrap(), Some(baseline));

        let recovered = engine.run_once().await.unwrap();
        assert_eq!(recovered.accepted, 1);
        assert_eq!(storage.outbox_count(OutboxStatus::Accepted).unwrap(), 1);
    }

    #[tokio::test]
    async fn renewed_lastfm_authorization_immediately_retries_the_preserved_song() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let base_time = Utc::now() - Duration::minutes(6);
        let baseline = snapshot(&["a", "b"], base_time);
        storage.store_snapshot_and_enqueue(&baseline, &[]).unwrap();
        let current = snapshot(&["a", "b", "c"], base_time + Duration::minutes(3));
        let history = FakeHistory {
            snapshots: Mutex::new(vec![current.clone(), current]),
        };
        let target = FakeTarget {
            failure: Mutex::new(Some(ProviderFailure::pause(
                "lastfm_auth",
                "expired Last.fm session",
            ))),
            ..FakeTarget::default()
        };
        let engine = SyncEngine::new(
            storage.clone(),
            history,
            target,
            SyncEngineConfig::default(),
        );

        assert!(matches!(
            engine.run_once().await,
            Err(SyncError::Target(ProviderFailure {
                disposition: FailureDisposition::Pause,
                ..
            }))
        ));
        assert_eq!(storage.outbox_count(OutboxStatus::Retryable).unwrap(), 1);
        assert!(storage.due_outbox(Utc::now(), 50).unwrap().is_empty());

        assert_eq!(
            storage
                .expedite_retryable_failures("lastfm_auth", Utc::now())
                .unwrap(),
            1
        );
        let recovered = engine.run_once().await.unwrap();

        assert_eq!(recovered.submitted, 1);
        assert_eq!(recovered.accepted, 1);
        assert_eq!(storage.outbox_count(OutboxStatus::Accepted).unwrap(), 1);
        assert_eq!(storage.outbox_count(OutboxStatus::Retryable).unwrap(), 0);
    }
}
