//! Text helpers for the first stage: accent folding, tokenization, phrase matching and a cheap
//! language guess. Everything works on folded tokens, so "Qu’est-ce que" and "qu'est ce que"
//! match the same pattern `["qu", "est", "ce", "que"]`.

use super::types::Language;

/// Lowercases and removes French diacritics.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' => out.push('a'),
            'ç' => out.push('c'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' | 'í' => out.push('i'),
            'ô' | 'ö' | 'ó' => out.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => out.push('u'),
            'ÿ' => out.push('y'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            other => out.push(other),
        }
    }
    out
}

/// Folded words; apostrophes, hyphens and punctuation separate words.
pub fn tokens(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Lowercased words that keep their accents, for the few checks where folding is ambiguous
/// ("sûr" versus "sur", "à" versus "a").
pub fn raw_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('’', "'")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// A pattern written in plain text, tokenized the same way as utterances.
pub fn pattern(text: &str) -> Vec<String> {
    tokens(text)
}

pub fn starts_with(words: &[String], pattern: &[String]) -> bool {
    !pattern.is_empty() && words.len() >= pattern.len() && words[..pattern.len()] == *pattern
}

pub fn contains(words: &[String], pattern: &[String]) -> bool {
    !pattern.is_empty() && words.windows(pattern.len()).any(|w| w == pattern)
}

/// The last non-space character, ignoring closing quotes and brackets.
pub fn last_mark(text: &str) -> Option<char> {
    text.trim_end()
        .chars()
        .rev()
        .find(|c| !matches!(c, '"' | '»' | '”' | ')' | ']' | ' ' | '\u{a0}'))
}

// Words that exist in both languages ("a", "me", "on", "an") are left out on purpose, and so
// are the single letters English contractions produce ("what's" → "s", "don't" → "t").
const FRENCH: &[&str] = &[
    "le", "la", "les", "un", "une", "des", "de", "en", "est", "et", "tu", "vous", "je", "nous",
    "il", "elle", "comment", "pourquoi", "quoi", "quel", "quelle", "avec", "dans", "pour", "sur",
    "pas", "ce", "que", "qui", "ton", "ta", "tes", "votre", "vos", "mon", "ma", "du", "au", "aux",
    "ou", "mais", "donc", "alors", "peux", "pouvez", "fais", "faire", "c", "qu", "d", "l", "j",
];
const ENGLISH: &[&str] = &[
    "the", "is", "are", "was", "how", "why", "what", "when", "where", "which", "you", "your",
    "with", "would", "could", "can", "do", "does", "did", "have", "has", "of", "to", "and", "for",
    "in", "that", "this", "it", "tell", "about", "i",
];

/// French / English / mixed from function words. Technical English nouns inside a French
/// sentence do not make it "mixed"; English function words do.
pub fn guess_language(words: &[String]) -> Language {
    let fr = words
        .iter()
        .filter(|w| FRENCH.contains(&w.as_str()))
        .count();
    let en = words
        .iter()
        .filter(|w| ENGLISH.contains(&w.as_str()))
        .count();
    match (fr, en) {
        (0, 0) => Language::Fr,
        (f, 0) if f > 0 => Language::Fr,
        (0, _) => Language::En,
        (f, e) if f >= 3 * e => Language::Fr,
        (f, e) if e >= 3 * f => Language::En,
        _ => Language::Mixed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str) -> Vec<String> {
        tokens(text)
    }

    #[test]
    fn folding_and_tokens_ignore_accents_apostrophes_and_hyphens() {
        assert_eq!(
            w("Qu’est-ce qu'un Hook ?"),
            ["qu", "est", "ce", "qu", "un", "hook"]
        );
        assert_eq!(fold("Éléments À ÇA œuvre"), "elements a ca oeuvre");
        assert!(starts_with(
            &w("Est-ce que tu connais Vue ?"),
            &pattern("est-ce que")
        ));
        assert!(contains(
            &w("Alors, comment ça va ?"),
            &pattern("comment ca va")
        ));
        assert!(!contains(&w("Alors"), &pattern("")));
    }

    #[test]
    fn last_mark_skips_closing_quotes() {
        assert_eq!(last_mark("Tu as dit « Vue » ?  "), Some('?'));
        assert_eq!(last_mark("Il a dit \"oui\"."), Some('.'));
        assert_eq!(last_mark("   "), None);
    }

    #[test]
    fn language_guess_keeps_technical_nouns_out_of_mixed() {
        assert_eq!(
            guess_language(&w("Comment tu gères le state management dans Vue ?")),
            Language::Fr
        );
        assert_eq!(
            guess_language(&w("How would you design a rate limiter?")),
            Language::En
        );
        assert_eq!(
            guess_language(&w(
                "Tu peux me dire what is the difference avec le virtual DOM ?"
            )),
            Language::Mixed
        );
    }
}
