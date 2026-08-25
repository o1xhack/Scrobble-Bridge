use std::collections::{HashMap, HashSet};

use crate::{HistoryItem, HistorySnapshot, track_comparison_key};

const MIN_ALIGNMENT_COVERAGE_PERCENT: usize = 60;

/// Result of comparing the last accepted source window to the current one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// First observation: record a baseline but do not backfill it.
    Baseline { snapshot: HistorySnapshot },
    /// A trustworthy ordered overlap was found.
    Delta {
        overlap_len: usize,
        new_item_indices: Vec<usize>,
        snapshot: HistorySnapshot,
    },
    /// The source moved beyond the known window; automatic submission must stop.
    Gap {
        previous_len: usize,
        current_len: usize,
        best_overlap_len: usize,
    },
}

fn item_key(item: &HistoryItem) -> String {
    track_comparison_key(
        item.track.source_id.as_deref(),
        &item.track.title,
        &item.track.artist,
    )
}

fn lcs_alignment(previous_keys: &[String], current_keys: &[String]) -> Vec<(usize, usize)> {
    let mut lengths = vec![vec![0; current_keys.len() + 1]; previous_keys.len() + 1];

    for previous_index in (0..previous_keys.len()).rev() {
        for current_index in (0..current_keys.len()).rev() {
            lengths[previous_index][current_index] =
                if previous_keys[previous_index] == current_keys[current_index] {
                    1 + lengths[previous_index + 1][current_index + 1]
                } else {
                    lengths[previous_index + 1][current_index]
                        .max(lengths[previous_index][current_index + 1])
                };
        }
    }

    let mut alignment = Vec::with_capacity(lengths[0][0]);
    let (mut previous_index, mut current_index) = (0, 0);
    while previous_index < previous_keys.len() && current_index < current_keys.len() {
        if previous_keys[previous_index] == current_keys[current_index]
            && lengths[previous_index][current_index]
                == 1 + lengths[previous_index + 1][current_index + 1]
        {
            alignment.push((previous_index, current_index));
            previous_index += 1;
            current_index += 1;
        } else if lengths[previous_index + 1][current_index]
            >= lengths[previous_index][current_index + 1]
        {
            // On ties, discard an older previous item first. History windows
            // normally advance by dropping their oldest prefix.
            previous_index += 1;
        } else {
            current_index += 1;
        }
    }
    alignment
}

fn alignment_is_trustworthy(
    previous_len: usize,
    alignment: &[(usize, usize)],
    minimum_overlap: usize,
    maximum_overlap: usize,
) -> bool {
    let required_overlap = minimum_overlap.max(1).min(maximum_overlap.max(1));
    if alignment.len() < required_overlap {
        return false;
    }

    let Some(&(first_previous, _)) = alignment.first() else {
        return false;
    };
    let Some(&(_, last_current)) = alignment.last() else {
        return false;
    };
    let previous_span = previous_len - first_previous;
    let current_span = last_current + 1;
    let widest_span = previous_span.max(current_span);

    alignment.len() * 100 >= widest_span * MIN_ALIGNMENT_COVERAGE_PERCENT
}

fn new_item_indices(
    previous_keys: &[String],
    current_keys: &[String],
    alignment: &[(usize, usize)],
) -> Vec<usize> {
    let last_current = alignment[alignment.len() - 1].1;
    let matched_previous = alignment
        .iter()
        .map(|(previous_index, _)| *previous_index)
        .collect::<HashSet<_>>();
    let matched_current = alignment
        .iter()
        .map(|(_, current_index)| *current_index)
        .collect::<HashSet<_>>();

    // If the provider merely reordered an already-seen item inside the
    // overlap, pair the unmatched occurrences by identity instead of treating
    // the moved item as a new play. Counted matching preserves real repeats.
    let mut unmatched_previous = HashMap::<String, usize>::new();
    for (index, key) in previous_keys.iter().enumerate() {
        if !matched_previous.contains(&index) {
            *unmatched_previous.entry(key.clone()).or_default() += 1;
        }
    }

    let mut indices = Vec::new();
    for (index, key) in current_keys.iter().enumerate().take(last_current + 1) {
        if matched_current.contains(&index) {
            continue;
        }
        if let Some(count) = unmatched_previous.get_mut(key)
            && *count > 0
        {
            *count -= 1;
            continue;
        }
        indices.push(index);
    }

    indices.extend(last_current + 1..current_keys.len());
    indices
}

/// Reconciles chronological windows while preserving repeated identical tracks.
///
/// YouTube Music can insert or omit a play inside a previously observed
/// history window. We therefore align the two windows as ordered sequences,
/// accept only a dense overlap, and treat unmatched current occurrences as new.
/// A configurable minimum and a coverage threshold prevent a few coincidental
/// matches from bridging a real history gap.
pub fn reconcile_history(
    previous: Option<&HistorySnapshot>,
    current: HistorySnapshot,
    minimum_overlap: usize,
) -> ReconcileOutcome {
    let Some(previous) = previous else {
        return ReconcileOutcome::Baseline { snapshot: current };
    };

    if previous.items.is_empty() {
        let new_item_indices = (0..current.items.len()).collect();
        return ReconcileOutcome::Delta {
            overlap_len: 0,
            new_item_indices,
            snapshot: current,
        };
    }

    let previous_keys = previous.items.iter().map(item_key).collect::<Vec<_>>();
    let current_keys = current.items.iter().map(item_key).collect::<Vec<_>>();
    let maximum = previous_keys.len().min(current_keys.len());
    let alignment = lcs_alignment(&previous_keys, &current_keys);
    let best_overlap_len = alignment.len();

    if !alignment_is_trustworthy(previous.items.len(), &alignment, minimum_overlap, maximum) {
        return ReconcileOutcome::Gap {
            previous_len: previous.items.len(),
            current_len: current.items.len(),
            best_overlap_len,
        };
    }

    let new_item_indices = new_item_indices(&previous_keys, &current_keys, &alignment);
    ReconcileOutcome::Delta {
        overlap_len: best_overlap_len,
        new_item_indices,
        snapshot: current,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::Track;

    fn item(id: &str, position: u32) -> HistoryItem {
        HistoryItem {
            track: Track::new(Some(id.to_owned()), id, "artist"),
            source_position: position,
            played_text: None,
        }
    }

    fn snapshot(ids: &[&str]) -> HistorySnapshot {
        HistorySnapshot {
            account_id: "account".to_owned(),
            observed_at: Utc::now(),
            items: ids
                .iter()
                .enumerate()
                .map(|(index, id)| item(id, u32::try_from(index).unwrap()))
                .collect(),
        }
    }

    #[test]
    fn first_window_becomes_baseline_without_backfill() {
        let outcome = reconcile_history(None, snapshot(&["a", "b"]), 2);
        assert!(matches!(outcome, ReconcileOutcome::Baseline { .. }));
    }

    #[test]
    fn first_play_after_an_explicit_empty_baseline_is_new() {
        let previous = snapshot(&[]);
        let outcome = reconcile_history(Some(&previous), snapshot(&["a"]), 2);

        let ReconcileOutcome::Delta {
            overlap_len,
            new_item_indices,
            ..
        } = outcome
        else {
            panic!("expected delta");
        };
        assert_eq!(overlap_len, 0);
        assert_eq!(new_item_indices, [0]);
    }

    #[test]
    fn finds_shifted_window_and_returns_only_new_items() {
        let previous = snapshot(&["a", "b", "c"]);
        let outcome = reconcile_history(Some(&previous), snapshot(&["b", "c", "d", "e"]), 2);

        let ReconcileOutcome::Delta {
            overlap_len,
            new_item_indices,
            snapshot,
            ..
        } = outcome
        else {
            panic!("expected delta");
        };

        assert_eq!(overlap_len, 2);
        assert_eq!(
            new_item_indices
                .iter()
                .map(|index| snapshot.items[*index]
                    .track
                    .source_id
                    .as_deref()
                    .unwrap_or_default())
                .collect::<Vec<_>>(),
            ["d", "e"]
        );
    }

    #[test]
    fn preserves_contiguous_repeats() {
        let previous = snapshot(&["a", "a"]);
        let outcome = reconcile_history(Some(&previous), snapshot(&["a", "a", "a"]), 2);

        let ReconcileOutcome::Delta {
            overlap_len,
            new_item_indices,
            ..
        } = outcome
        else {
            panic!("expected delta");
        };

        assert_eq!(overlap_len, 2);
        assert_eq!(new_item_indices, [2]);
    }

    #[test]
    fn refuses_weak_overlap_in_large_windows() {
        let previous = snapshot(&["a", "b", "c"]);
        let outcome = reconcile_history(Some(&previous), snapshot(&["c", "d", "e"]), 2);
        assert!(matches!(
            outcome,
            ReconcileOutcome::Gap {
                best_overlap_len: 1,
                ..
            }
        ));
    }

    #[test]
    fn allows_one_item_overlap_when_windows_are_only_one_item_long() {
        let previous = snapshot(&["a"]);
        let outcome = reconcile_history(Some(&previous), snapshot(&["a"]), 2);
        assert!(matches!(
            outcome,
            ReconcileOutcome::Delta { overlap_len: 1, .. }
        ));
    }

    #[test]
    fn tolerates_a_late_insert_inside_the_real_history_window() {
        let previous = snapshot(&["motto", "suddenly", "spaghetti", "gate", "my-pace-official"]);
        let outcome = reconcile_history(
            Some(&previous),
            snapshot(&[
                "suddenly",
                "spaghetti",
                "gate",
                "my-pace-video",
                "my-pace-official",
                "lightning",
                "sound-wave",
            ]),
            2,
        );

        let ReconcileOutcome::Delta {
            overlap_len,
            new_item_indices,
            snapshot,
        } = outcome
        else {
            panic!("expected a recoverable delta");
        };
        assert_eq!(overlap_len, 4);
        assert_eq!(
            new_item_indices
                .iter()
                .map(|index| snapshot.items[*index]
                    .track
                    .source_id
                    .as_deref()
                    .unwrap_or_default())
                .collect::<Vec<_>>(),
            ["my-pace-video", "lightning", "sound-wave"]
        );
    }

    #[test]
    fn tolerates_an_omission_inside_the_overlap() {
        let previous = snapshot(&["a", "b", "omitted", "c", "d"]);
        let outcome = reconcile_history(Some(&previous), snapshot(&["b", "c", "d", "e"]), 2);

        let ReconcileOutcome::Delta {
            overlap_len,
            new_item_indices,
            ..
        } = outcome
        else {
            panic!("expected a recoverable delta");
        };
        assert_eq!(overlap_len, 3);
        assert_eq!(new_item_indices, [3]);
    }

    #[test]
    fn does_not_resubmit_an_item_reordered_inside_the_overlap() {
        let previous = snapshot(&["a", "b", "c", "d"]);
        let outcome = reconcile_history(Some(&previous), snapshot(&["b", "a", "c", "d", "e"]), 2);

        let ReconcileOutcome::Delta {
            new_item_indices, ..
        } = outcome
        else {
            panic!("expected a recoverable delta");
        };
        assert_eq!(new_item_indices, [4]);
    }

    #[test]
    fn refuses_sparse_coincidental_matches() {
        let previous = snapshot(&["a", "b", "c", "d", "e", "f"]);
        let outcome = reconcile_history(
            Some(&previous),
            snapshot(&["b", "x", "y", "z", "f", "new"]),
            2,
        );
        assert!(matches!(
            outcome,
            ReconcileOutcome::Gap {
                best_overlap_len: 2,
                ..
            }
        ));
    }
}
