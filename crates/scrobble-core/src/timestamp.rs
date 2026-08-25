use chrono::{DateTime, Duration, Utc};
use std::collections::HashSet;
use uuid::Uuid;

use crate::{HistoryItem, ScrobbleCandidate, candidate_fingerprint};

pub const DEFAULT_TRACK_SECONDS: i64 = 240;
const MIN_TRACK_SECONDS: i64 = 30;
const MAX_TRACK_SECONDS: i64 = 3_600;

/// Assigns estimated start times to chronological history items by walking
/// backwards from the observation time.
pub fn infer_scrobble_candidates(
    account_id: &str,
    items: &[HistoryItem],
    observed_at: DateTime<Utc>,
) -> Vec<ScrobbleCandidate> {
    infer_scrobble_candidates_at_indices(account_id, items, observed_at, 0..items.len())
}

/// Assigns estimated start times using the full chronological source window,
/// then returns candidates only for the selected item indices.
///
/// This matters when YouTube Music inserts a newly visible play inside an
/// existing history window. Estimating only from the inserted items would
/// compress away the surrounding tracks and produce incorrect timestamps.
pub fn infer_scrobble_candidates_at_indices(
    account_id: &str,
    items: &[HistoryItem],
    observed_at: DateTime<Utc>,
    indices: impl IntoIterator<Item = usize>,
) -> Vec<ScrobbleCandidate> {
    let selected = indices
        .into_iter()
        .filter(|index| *index < items.len())
        .collect::<HashSet<_>>();
    let mut starts = Vec::with_capacity(selected.len());
    let mut cursor = observed_at;

    for (index, item) in items.iter().enumerate().rev() {
        let duration_seconds = item
            .track
            .duration_seconds
            .map_or(DEFAULT_TRACK_SECONDS, i64::from)
            .clamp(MIN_TRACK_SECONDS, MAX_TRACK_SECONDS);
        let started_at = cursor - Duration::seconds(duration_seconds);
        if selected.contains(&index) {
            starts.push((item, started_at));
        }
        cursor = started_at;
    }

    starts.reverse();
    starts
        .into_iter()
        .map(|(item, started_at)| ScrobbleCandidate {
            id: Uuid::new_v4(),
            account_id: account_id.to_owned(),
            track: item.track.clone(),
            started_at,
            timestamp_is_estimated: true,
            source_position: item.source_position,
            fingerprint: candidate_fingerprint(
                account_id,
                &item.track,
                started_at,
                item.source_position,
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::Track;

    #[test]
    fn inferred_times_are_chronological_and_use_track_duration() {
        let observed_at = Utc.timestamp_opt(1_700_001_000, 0).unwrap();
        let items = [
            HistoryItem {
                track: Track {
                    duration_seconds: Some(120),
                    ..Track::new(Some("a".to_owned()), "A", "Artist")
                },
                source_position: 0,
                played_text: None,
            },
            HistoryItem {
                track: Track {
                    duration_seconds: Some(180),
                    ..Track::new(Some("b".to_owned()), "B", "Artist")
                },
                source_position: 1,
                played_text: None,
            },
        ];

        let candidates = infer_scrobble_candidates("account", &items, observed_at);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].started_at.timestamp(), 1_700_000_700);
        assert_eq!(candidates[1].started_at.timestamp(), 1_700_000_820);
        assert!(candidates[0].started_at < candidates[1].started_at);
    }

    #[test]
    fn selected_candidates_keep_full_window_timestamps() {
        let observed_at = Utc.timestamp_opt(1_700_001_000, 0).unwrap();
        let items = [
            HistoryItem {
                track: Track {
                    duration_seconds: Some(120),
                    ..Track::new(Some("a".to_owned()), "A", "Artist")
                },
                source_position: 2,
                played_text: None,
            },
            HistoryItem {
                track: Track {
                    duration_seconds: Some(180),
                    ..Track::new(Some("inserted".to_owned()), "Inserted", "Artist")
                },
                source_position: 1,
                played_text: None,
            },
            HistoryItem {
                track: Track {
                    duration_seconds: Some(240),
                    ..Track::new(Some("c".to_owned()), "C", "Artist")
                },
                source_position: 0,
                played_text: None,
            },
        ];

        let candidates = infer_scrobble_candidates_at_indices("account", &items, observed_at, [1]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].track.source_id.as_deref(), Some("inserted"));
        assert_eq!(candidates[0].started_at.timestamp(), 1_700_000_580);
    }
}
