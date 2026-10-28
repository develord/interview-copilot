//! Turns final transcript segments into utterances. A fast endpoint can close a turn on a
//! hesitation ("Et du coup, sur la partie…"), so a final that looks cut waits briefly for the
//! next one and the two are merged. Partials prove the speaker is still talking and extend the
//! wait, up to a hard limit: an incomplete utterance is never held forever (finalization
//! timeout, PRD §4).
//!
//! Pure state machine over a millisecond clock, driven by `engine` and by tests.

use crate::audio::types::SourceKind;

use super::rules::looks_incomplete;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssemblerConfig {
    /// How long a cut turn waits for its continuation.
    pub continuation_wait_ms: u64,
    /// Hard limit from the first segment of an utterance.
    pub max_wait_ms: u64,
}

impl AssemblerConfig {
    pub const DEFAULT: Self = Self {
        continuation_wait_ms: 1_200,
        max_wait_ms: 6_000,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Utterance {
    pub source: SourceKind,
    pub segment_ids: Vec<String>,
    pub text: String,
}

#[derive(Debug)]
struct Pending {
    segment_ids: Vec<String>,
    text: String,
    first_at: u64,
    deadline: u64,
}

#[derive(Debug)]
pub struct Assembler {
    config: AssemblerConfig,
    /// One slot per source: index 0 = system, 1 = microphone.
    pending: [Option<Pending>; 2],
}

fn slot(source: SourceKind) -> usize {
    match source {
        SourceKind::System => 0,
        SourceKind::Microphone => 1,
    }
}

const SOURCES: [SourceKind; 2] = [SourceKind::System, SourceKind::Microphone];

impl Assembler {
    pub fn new(config: AssemblerConfig) -> Self {
        Self {
            config,
            pending: [None, None],
        }
    }

    /// Returns the utterance this final completes, if any.
    pub fn on_final(
        &mut self,
        source: SourceKind,
        segment_id: &str,
        text: &str,
        now: u64,
    ) -> Option<Utterance> {
        let text = text.trim();
        let config = self.config;
        let entry = &mut self.pending[slot(source)];
        if text.is_empty() {
            // An empty final closes nothing but proves the turn ended: flush if overdue.
            return None;
        }
        let pending = entry.get_or_insert_with(|| Pending {
            segment_ids: Vec::new(),
            text: String::new(),
            first_at: now,
            deadline: now,
        });
        pending.segment_ids.push(segment_id.to_string());
        if !pending.text.is_empty() {
            pending.text.push(' ');
        }
        pending.text.push_str(text);

        if looks_incomplete(&pending.text) && now < pending.first_at + config.max_wait_ms {
            pending.deadline = deadline(config, pending.first_at, now);
            None
        } else {
            entry.take().map(|p| into_utterance(source, p))
        }
    }

    /// The speaker of `source` is talking again: a cut utterance keeps waiting.
    pub fn on_partial(&mut self, source: SourceKind, now: u64) {
        let config = self.config;
        if let Some(p) = self.pending[slot(source)].as_mut() {
            p.deadline = p.deadline.max(deadline(config, p.first_at, now));
        }
    }

    /// Utterances whose wait is over.
    pub fn poll(&mut self, now: u64) -> Vec<Utterance> {
        SOURCES
            .into_iter()
            .filter_map(|source| {
                let entry = &mut self.pending[slot(source)];
                if entry.as_ref().is_some_and(|p| p.deadline <= now) {
                    entry.take().map(|p| into_utterance(source, p))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.pending.iter().flatten().map(|p| p.deadline).min()
    }

    /// Everything still waiting (end of session).
    pub fn flush(&mut self) -> Vec<Utterance> {
        SOURCES
            .into_iter()
            .filter_map(|source| {
                self.pending[slot(source)]
                    .take()
                    .map(|p| into_utterance(source, p))
            })
            .collect()
    }
}

fn deadline(config: AssemblerConfig, first_at: u64, now: u64) -> u64 {
    (now + config.continuation_wait_ms).min(first_at + config.max_wait_ms)
}

fn into_utterance(source: SourceKind, pending: Pending) -> Utterance {
    Utterance {
        source,
        segment_ids: pending.segment_ids,
        text: pending.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYS: SourceKind = SourceKind::System;
    const MIC: SourceKind = SourceKind::Microphone;

    fn assembler() -> Assembler {
        Assembler::new(AssemblerConfig::DEFAULT)
    }

    #[test]
    fn a_complete_final_is_an_utterance_at_once() {
        let mut a = assembler();
        let u = a
            .on_final(SYS, "s-1-0", "Comment tu gères le state ?", 0)
            .unwrap();
        assert_eq!(u.segment_ids, ["s-1-0"]);
        assert_eq!(u.text, "Comment tu gères le state ?");
        assert_eq!(a.next_deadline(), None);
    }

    #[test]
    fn a_cut_final_is_merged_with_its_continuation() {
        let mut a = assembler();
        assert_eq!(
            a.on_final(SYS, "s-1-0", "Et du coup, sur la partie", 0),
            None
        );
        assert_eq!(a.next_deadline(), Some(1_200));
        let u = a
            .on_final(SYS, "s-1-1", "base de données, tu choisirais quoi ?", 900)
            .unwrap();
        assert_eq!(u.segment_ids, ["s-1-0", "s-1-1"]);
        assert_eq!(
            u.text,
            "Et du coup, sur la partie base de données, tu choisirais quoi ?"
        );
    }

    #[test]
    fn a_cut_final_without_continuation_is_released_after_the_wait() {
        let mut a = assembler();
        a.on_final(SYS, "s-1-0", "Est-ce que tu peux me parler de la", 0);
        assert!(a.poll(1_199).is_empty());
        let released = a.poll(1_200);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].text, "Est-ce que tu peux me parler de la");
        assert!(a.poll(5_000).is_empty());
    }

    #[test]
    fn partials_extend_the_wait_but_never_past_the_hard_limit() {
        let mut a = assembler();
        a.on_final(SYS, "s-1-0", "Alors dans ton dernier projet,", 0);
        a.on_partial(SYS, 1_000);
        assert_eq!(a.next_deadline(), Some(2_200));
        a.on_partial(SYS, 5_500);
        assert_eq!(a.next_deadline(), Some(6_000));
        // A cut final that arrives past the hard limit is released, not held again.
        let u = a.on_final(SYS, "s-1-1", "et sur la partie", 6_100).unwrap();
        assert_eq!(u.segment_ids, ["s-1-0", "s-1-1"]);
    }

    #[test]
    fn sources_are_assembled_independently() {
        let mut a = assembler();
        a.on_final(SYS, "system-1-0", "Et pour la", 0);
        // The candidate's partial does not extend the interviewer's wait.
        a.on_partial(MIC, 1_000);
        let mic = a
            .on_final(MIC, "microphone-1-0", "Oui, bien sûr.", 1_000)
            .unwrap();
        assert_eq!(mic.source, MIC);
        assert_eq!(a.poll(1_200).len(), 1);
    }

    #[test]
    fn flush_releases_everything_waiting() {
        let mut a = assembler();
        a.on_final(SYS, "s-1-0", "Et pour la", 0);
        a.on_final(MIC, "m-1-0", "Je pense que", 0);
        assert_eq!(a.flush().len(), 2);
        assert_eq!(a.next_deadline(), None);
    }
}
