use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fmt::Write;

use crate::{Track, track_comparison_key};

pub fn candidate_fingerprint(
    account_id: &str,
    track: &Track,
    started_at: DateTime<Utc>,
    source_position: u32,
) -> String {
    let mut digest = Sha256::new();
    digest.update(account_id.as_bytes());
    digest.update([0]);
    digest.update(
        track_comparison_key(track.source_id.as_deref(), &track.title, &track.artist).as_bytes(),
    );
    digest.update([0]);
    digest.update(started_at.timestamp().to_be_bytes());
    digest.update(source_position.to_be_bytes());
    let output = digest.finalize();
    let mut encoded = String::with_capacity(output.len() * 2);
    for byte in output {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn fingerprint_is_stable_and_account_scoped() {
        let track = Track::new(Some("video".to_owned()), "Song", "Artist");
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

        let first = candidate_fingerprint("a", &track, timestamp, 1);
        let second = candidate_fingerprint("a", &track, timestamp, 1);
        let other_account = candidate_fingerprint("b", &track, timestamp, 1);

        assert_eq!(first, second);
        assert_ne!(first, other_account);
        assert_eq!(first.len(), 64);
    }
}
