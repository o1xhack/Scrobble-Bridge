use unicode_normalization::UnicodeNormalization;

/// Normalizes only for identity comparison; display metadata remains untouched.
pub fn normalize_component(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn track_comparison_key(source_id: Option<&str>, title: &str, artist: &str) -> String {
    match source_id.filter(|value| !value.trim().is_empty()) {
        Some(source_id) => format!("id:{source_id}"),
        None => format!(
            "meta:{}\u{1f}{}",
            normalize_component(artist),
            normalize_component(title)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_unicode_case_and_punctuation() {
        assert_eq!(normalize_component("  Beyoncé — HALO! "), "beyoncé halo");
        assert_eq!(normalize_component("ＡＢＣ"), "abc");
    }

    #[test]
    fn source_id_takes_precedence() {
        assert_eq!(
            track_comparison_key(Some("video-1"), "Different", "Metadata"),
            "id:video-1"
        );
    }
}
