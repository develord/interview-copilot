//! Splits the streamed answer into its two card sections at `DETAILS_MARKER`, without ever
//! showing a partial marker: text that could be the start of the marker (and whitespace
//! before it) is held back until the next piece tells.

use super::prompt::DETAILS_MARKER;
use crate::events::AnswerSection;

#[derive(Debug)]
pub struct Splitter {
    section: AnswerSection,
    pending: String,
    details_started: bool,
}

impl Default for Splitter {
    fn default() -> Self {
        Self {
            section: AnswerSection::Primary,
            pending: String::new(),
            details_started: false,
        }
    }
}

/// Length of the longest end of `text` that is a beginning of the marker.
fn partial_marker(text: &str) -> usize {
    (1..DETAILS_MARKER.len())
        .rev()
        .find(|&k| text.ends_with(&DETAILS_MARKER[..k]))
        .unwrap_or(0)
}

impl Splitter {
    pub fn push(&mut self, piece: &str) -> Vec<(AnswerSection, String)> {
        self.pending.push_str(piece);
        let mut out = Vec::new();
        loop {
            match self.section {
                AnswerSection::Primary => {
                    if let Some(at) = self.pending.find(DETAILS_MARKER) {
                        let before = self.pending[..at].trim_end().to_string();
                        if !before.is_empty() {
                            out.push((AnswerSection::Primary, before));
                        }
                        self.pending = self.pending[at + DETAILS_MARKER.len()..].to_string();
                        self.section = AnswerSection::Details;
                        continue;
                    }
                    // Trailing whitespace is held back too: it may precede the marker, and
                    // the main answer must not end with the line break before it.
                    let possible_marker = self.pending.len() - partial_marker(&self.pending);
                    let ready = self.pending[..possible_marker].trim_end().len();
                    if ready > 0 {
                        out.push((AnswerSection::Primary, self.pending[..ready].to_string()));
                        self.pending.drain(..ready);
                    }
                }
                AnswerSection::Details => {
                    if !self.details_started {
                        let trimmed = self.pending.trim_start().to_string();
                        self.pending = trimmed;
                        self.details_started = !self.pending.is_empty();
                    }
                    if !self.pending.is_empty() {
                        out.push((AnswerSection::Details, std::mem::take(&mut self.pending)));
                    }
                }
            }
            return out;
        }
    }

    /// The text held back at the end of the stream.
    pub fn finish(&mut self) -> Vec<(AnswerSection, String)> {
        let rest = std::mem::take(&mut self.pending);
        if rest.trim().is_empty() {
            Vec::new()
        } else {
            vec![(self.section, rest)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pieces: &[&str]) -> (String, String) {
        let mut splitter = Splitter::default();
        let mut out: Vec<(AnswerSection, String)> =
            pieces.iter().flat_map(|p| splitter.push(p)).collect();
        out.extend(splitter.finish());
        let join = |section| {
            out.iter()
                .filter(|(s, _)| *s == section)
                .map(|(_, t)| t.as_str())
                .collect::<String>()
        };
        (join(AnswerSection::Primary), join(AnswerSection::Details))
    }

    #[test]
    fn the_marker_splits_sections_even_when_cut_across_pieces() {
        let text = "J'utilise Pinia.\n<<<DETAILS>>>\n```ts\nconst store = defineStore()\n```";
        let expected = (
            "J'utilise Pinia.".to_string(),
            "```ts\nconst store = defineStore()\n```".to_string(),
        );
        // Every way of cutting the text in two, and one character at a time.
        for cut in 0..=text.len() {
            if text.is_char_boundary(cut) {
                assert_eq!(run(&[&text[..cut], &text[cut..]]), expected, "cut at {cut}");
            }
        }
        let chars: Vec<String> = text.chars().map(String::from).collect();
        let refs: Vec<&str> = chars.iter().map(String::as_str).collect();
        assert_eq!(run(&refs), expected);
    }

    #[test]
    fn text_that_only_looks_like_the_marker_is_not_lost() {
        assert_eq!(run(&["a <<", "< b"]), ("a <<< b".into(), String::new()));
        assert_eq!(run(&["fin <<<DET"]), ("fin <<<DET".into(), String::new()));
    }

    #[test]
    fn answers_without_details_or_with_an_empty_details_section() {
        assert_eq!(
            run(&["Bonjour, ", "merci !"]),
            ("Bonjour, merci !".into(), String::new())
        );
        assert_eq!(
            run(&["Oui.\n<<<DETAILS>>>\n  "]),
            ("Oui.".into(), String::new())
        );
    }
}
