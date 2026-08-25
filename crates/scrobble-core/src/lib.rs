//! Domain types and correctness-critical reconciliation for Scrobble Bridge.

mod fingerprint;
mod model;
mod normalize;
mod reconcile;
mod timestamp;

pub use fingerprint::candidate_fingerprint;
pub use model::{
    HistoryItem, HistorySnapshot, OutboxEntry, OutboxStatus, ScrobbleCandidate, Track,
};
pub use normalize::{normalize_component, track_comparison_key};
pub use reconcile::{ReconcileOutcome, reconcile_history};
pub use timestamp::{
    DEFAULT_TRACK_SECONDS, infer_scrobble_candidates, infer_scrobble_candidates_at_indices,
};
