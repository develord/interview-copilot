//! Scoring for the STT benchmark harness (`src/bin/stt-bench.rs`): normalized word error rate,
//! technical-term recall and latency percentiles. Pure functions, unit-tested.

/// Lowercases, unifies apostrophes, and turns punctuation into word breaks, so "Vue ?" and "vue"
/// compare equal while accents (meaningful in French) are kept.
pub fn normalize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace(['’', '`', 'ʼ'], "'")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' || c == '+' || c == '#' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(|w| w.trim_matches('\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word-level Levenshtein distance between two token sequences.
pub fn edit_distance(reference: &[String], hypothesis: &[String]) -> usize {
    let mut previous: Vec<usize> = (0..=hypothesis.len()).collect();
    for (i, r) in reference.iter().enumerate() {
        let mut current = vec![i + 1; hypothesis.len() + 1];
        for (j, h) in hypothesis.iter().enumerate() {
            let substitution = previous[j] + usize::from(r != h);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[hypothesis.len()]
}

/// Errors and reference length, to aggregate WER over a corpus (sum errors / sum words).
pub fn word_errors(reference: &str, hypothesis: &str) -> (usize, usize) {
    let reference = normalize(reference);
    let hypothesis = normalize(hypothesis);
    (edit_distance(&reference, &hypothesis), reference.len())
}

/// Whether a technical term appears in the hypothesis as a contiguous word sequence.
pub fn contains_term(hypothesis: &str, term: &str) -> bool {
    let words = normalize(hypothesis);
    let needle = normalize(term);
    !needle.is_empty()
        && words
            .windows(needle.len())
            .any(|window| window == needle.as_slice())
}

/// Nearest-rank percentile (`p` in 0–100) of an unsorted sample; `None` when empty.
pub fn percentile(values: &[u32], p: f64) -> Option<u32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    Some(sorted[rank.min(sorted.len()) - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        normalize(s)
    }

    #[test]
    fn normalization_ignores_case_and_punctuation_but_keeps_accents() {
        assert_eq!(
            words("Comment tu GÈRES le state-management, dans Vue ?"),
            words("comment tu gères le state management dans vue")
        );
        assert_eq!(words("C’est quoi ?"), vec!["c'est", "quoi"]);
        assert_ne!(
            words("gère"),
            words("gere"),
            "accents change meaning in French"
        );
        assert_eq!(words("C++ et C#"), vec!["c++", "et", "c#"]);
    }

    #[test]
    fn counts_substitutions_insertions_and_deletions() {
        assert_eq!(
            word_errors(
                "le state management dans vue",
                "le state management dans vue"
            ),
            (0, 5)
        );
        assert_eq!(
            word_errors("le state management dans vue", "la gestion d'état dans vue"),
            (3, 5)
        );
        assert_eq!(word_errors("a b c", "a c"), (1, 3));
        assert_eq!(word_errors("a b", "a b c d"), (2, 2));
        assert_eq!(word_errors("", "noise"), (1, 0));
    }

    #[test]
    fn finds_multi_word_terms_regardless_of_punctuation() {
        assert!(contains_term(
            "Comment tu gères le state management ?",
            "state management"
        ));
        assert!(contains_term(
            "on utilise Promise.allSettled ici",
            "Promise.allSettled"
        ));
        assert!(!contains_term("la gestion d'état", "state management"));
        assert!(!contains_term("Vue", ""));
    }

    #[test]
    fn nearest_rank_percentiles() {
        let latencies = [900, 100, 300, 500, 700];
        assert_eq!(percentile(&latencies, 50.0), Some(500));
        assert_eq!(percentile(&latencies, 95.0), Some(900));
        assert_eq!(percentile(&latencies, 0.0), Some(100));
        assert_eq!(percentile(&[], 50.0), None);
    }
}
