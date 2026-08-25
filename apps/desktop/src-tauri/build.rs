fn main() {
    println!("cargo:rerun-if-env-changed=SCROBBLE_LASTFM_API_KEY");
    println!("cargo:rerun-if-env-changed=SCROBBLE_LASTFM_SHARED_SECRET");
    println!("cargo:rerun-if-env-changed=SCROBBLE_PRODUCTION_EXTENSION_ID");

    let has_api_key = std::env::var("SCROBBLE_LASTFM_API_KEY")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let has_shared_secret = std::env::var("SCROBBLE_LASTFM_SHARED_SECRET")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    assert_eq!(
        has_api_key, has_shared_secret,
        "SCROBBLE_LASTFM_API_KEY and SCROBBLE_LASTFM_SHARED_SECRET must both be configured or both be absent"
    );

    if let Some(extension_id) = std::env::var("SCROBBLE_PRODUCTION_EXTENSION_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        assert!(
            extension_id.len() == 32
                && extension_id.bytes().all(|byte| matches!(byte, b'a'..=b'p')),
            "SCROBBLE_PRODUCTION_EXTENSION_ID must be a 32-character Chrome extension ID"
        );
    }

    tauri_build::build();
}
