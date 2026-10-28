//! Technical vocabulary (PRD §3.3), shared with the frontend through
//! `contracts/vocabulary/technical-terms.json`. Terms bias the provider's recognition; they are
//! never used to blindly rewrite transcripts.

use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Term {
    pub canonical: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub category: String,
}

#[derive(Deserialize)]
struct Vocabulary {
    terms: Vec<Term>,
}

static BUILTIN: LazyLock<Vec<Term>> = LazyLock::new(|| {
    let json = include_str!("../../../contracts/vocabulary/technical-terms.json");
    serde_json::from_str::<Vocabulary>(json)
        .expect("contracts/vocabulary/technical-terms.json is invalid")
        .terms
});

pub fn builtin() -> &'static [Term] {
    &BUILTIN
}

/// Bias terms for a session: user terms first (most specific to this interview), then the
/// built-in canonical terms; trimmed, deduplicated case-insensitively, capped at `limit`.
pub fn bias_terms(extra: &[String], include_builtin: bool, limit: usize) -> Vec<String> {
    let builtin = include_builtin
        .then(|| builtin().iter().map(|t| t.canonical.as_str()))
        .into_iter()
        .flatten();
    let mut seen = std::collections::HashSet::new();
    extra
        .iter()
        .map(|s| s.trim())
        .chain(builtin)
        .filter(|term| !term.is_empty() && term.chars().count() <= 50)
        .filter(|term| seen.insert(term.to_lowercase()))
        .take(limit)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_dictionary_parses_and_has_unique_canonical_terms() {
        let terms = builtin();
        assert!(terms.len() >= 80, "{} terms", terms.len());
        let mut seen = std::collections::HashSet::new();
        for term in terms {
            assert!(
                seen.insert(term.canonical.to_lowercase()),
                "duplicate {}",
                term.canonical
            );
            assert!(!term.category.is_empty());
        }
        assert!(
            terms
                .iter()
                .any(|t| t.canonical == "Vue.js" && t.aliases.contains(&"Vue".to_string()))
        );
    }

    #[test]
    fn user_terms_come_first_and_duplicates_are_removed() {
        let extra = vec![
            "  Kafka ".to_string(),
            "Spring Boot".to_string(),
            "kafka".to_string(),
            String::new(),
        ];
        let terms = bias_terms(&extra, true, 100);
        assert_eq!(&terms[..2], &["Kafka", "Spring Boot"]);
        assert_eq!(
            terms
                .iter()
                .filter(|t| t.eq_ignore_ascii_case("kafka"))
                .count(),
            1
        );
        assert_eq!(
            terms.len(),
            builtin().len() + 1,
            "every built-in term plus Spring Boot"
        );
        assert_eq!(
            bias_terms(&extra, true, 10).len(),
            10,
            "capped at the provider limit"
        );
    }

    #[test]
    fn builtin_terms_can_be_left_out() {
        assert_eq!(bias_terms(&["Rust".into()], false, 100), vec!["Rust"]);
        assert!(bias_terms(&[], false, 100).is_empty());
    }
}
