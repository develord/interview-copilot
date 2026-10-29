//! Turns assembled utterances into `question.*` and `utterance.classified` events.
//!
//! The detector keeps the bounded conversation context (recent lines of both speakers and the
//! recent questions), runs the first stage, and either decides at once or prepares a model
//! request. Model results come back through `resolve`. When the model is unavailable or fails,
//! the first-stage hints decide alone: explicit questions still become cards, statements do
//! not, so a provider outage never loses an obvious question (PRD §10).
//!
//! Synchronous and clock-free (times are passed in), so every rule is unit-tested.

use std::collections::VecDeque;

use super::assembler::Utterance;
use super::classifier::{
    Classification, ClassifyRequest, ContextLine, RecentQuestion, Relation, Speaker,
};
use super::engine::Escalated;
use super::rules::{self, Hints, Verdict};
use super::text;
use super::types::{
    ClassifyError, Decision, DetectedQuestion, IgnoreReason, Intent, Language, Stage,
    UtteranceReport,
};
use crate::audio::types::SourceKind;
use crate::events::PipelineEvent;

const MAX_TEXT_CHARS: usize = 4_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetectorConfig {
    /// Microphone speech is the candidate's: context only, unless this is set (testing).
    pub include_microphone: bool,
    /// Transcript lines sent with each classification.
    pub context_lines: usize,
    /// Recent questions sent with each classification.
    pub recent_questions: usize,
    /// A bare "Pourquoi ?" refers to the latest question only within this window.
    pub follow_up_window_ms: u64,
    /// The same words again within this window are a repeat, not a new question.
    pub repeat_window_ms: u64,
    /// Only technical questions and follow-ups on them become cards. The others are still
    /// tracked, so their repeats and follow-ups are recognised, and reported as `not-technical`.
    pub technical_only: bool,
}

impl DetectorConfig {
    pub const DEFAULT: Self = Self {
        include_microphone: false,
        context_lines: 10,
        recent_questions: 6,
        follow_up_window_ms: 180_000,
        repeat_window_ms: 120_000,
        technical_only: false,
    };
}

#[derive(Debug)]
struct Tracked {
    id: String,
    question: String,
    intent: Intent,
    /// Folded words of the original utterance, for exact-repeat detection.
    key: Vec<String>,
    at: u64,
    /// Got a card (always, unless `technical_only` left it out).
    shown: bool,
}

/// A classification waiting for the model.
#[derive(Debug)]
pub struct Pending {
    pub request: ClassifyRequest,
    utterance: Utterance,
    utterance_id: String,
    hints: Hints,
}

impl Pending {
    /// The utterance as announced before the model decides.
    pub fn escalated(&self) -> Escalated {
        Escalated {
            segment_ids: self.utterance.segment_ids.clone(),
            text: self.request.utterance.clone(),
        }
    }
}

#[derive(Debug)]
pub enum Step {
    /// Decided without the model.
    Done(Vec<PipelineEvent>),
    /// Call the model with `pending.request`, then `resolve`.
    Classify(Box<Pending>),
    /// Candidate speech, kept as context.
    Context,
}

pub struct Detector {
    config: DetectorConfig,
    session_id: String,
    model_available: bool,
    next_utterance: u64,
    next_question: u64,
    context: VecDeque<ContextLine>,
    questions: VecDeque<Tracked>,
}

fn clip(text: &str) -> String {
    text.trim().chars().take(MAX_TEXT_CHARS).collect()
}

fn source_id(source: SourceKind) -> &'static str {
    match source {
        SourceKind::System => "system",
        SourceKind::Microphone => "microphone",
    }
}

/// Folded words without hesitations: two utterances with the same key say the same thing.
fn repeat_key(text: &str) -> Vec<String> {
    const FILLERS: &[&str] = &["euh", "heu", "hum", "hmm", "bah", "ben", "um", "uh"];
    text::tokens(text)
        .into_iter()
        .filter(|w| !FILLERS.contains(&w.as_str()))
        .collect()
}

impl Detector {
    pub fn new(
        session_id: impl Into<String>,
        config: DetectorConfig,
        model_available: bool,
    ) -> Self {
        Self {
            config,
            session_id: session_id.into(),
            model_available,
            next_utterance: 0,
            next_question: 0,
            context: VecDeque::new(),
            questions: VecDeque::new(),
        }
    }

    /// The model failed for good (bad key, no credit): rules only from now on.
    pub fn disable_model(&mut self) {
        self.model_available = false;
    }

    pub fn on_utterance(&mut self, utterance: Utterance, now: u64, wall_ms: u64) -> Step {
        let text = clip(&utterance.text);
        if text.is_empty() {
            return Step::Context;
        }
        let asks_questions =
            utterance.source == SourceKind::System || self.config.include_microphone;
        let speaker = if asks_questions {
            Speaker::Interviewer
        } else {
            Speaker::Candidate
        };
        let transcript: Vec<ContextLine> = self.context.iter().cloned().collect();
        self.remember(speaker, &text);
        if !asks_questions {
            return Step::Context;
        }

        self.next_utterance += 1;
        let utterance_id = format!("{}-u{}", self.session_id, self.next_utterance);
        let report =
            |decision, reason, intent, stage, question_id: Option<String>| UtteranceReport {
                id: utterance_id.clone(),
                source_id: source_id(utterance.source).to_string(),
                segment_ids: utterance.segment_ids.clone(),
                text: text.clone(),
                decision,
                reason,
                intent,
                stage,
                question_id,
                latency_ms: None,
            };

        let latest_recent = self
            .questions
            .back()
            .filter(|q| now.saturating_sub(q.at) <= self.config.follow_up_window_ms)
            .map(|q| q.id.clone());

        let words = text::tokens(&text);
        match rules::evaluate(&text, latest_recent.is_some()) {
            Verdict::Drop(reason) => Step::Done(vec![classified(report(
                Decision::Ignored,
                Some(reason),
                None,
                Stage::Rules,
                None,
            ))]),
            Verdict::Greeting => {
                // "Bonjour bonjour !" twice is one greeting; "Tu m'entends ?" after "Bonjour"
                // is a new small-talk question that needs its own reply.
                let asks = matches!(text::last_mark(&text), Some('?'));
                let earlier = self
                    .recent_with_intent(Intent::GreetingOrSmallTalk, now)
                    .filter(|_| !asks);
                if let Some(earlier) = earlier {
                    return Step::Done(vec![classified(report(
                        Decision::Ignored,
                        Some(IgnoreReason::Repeat),
                        Some(Intent::GreetingOrSmallTalk),
                        Stage::Rules,
                        Some(earlier),
                    ))]);
                }
                let question = self.create(
                    &utterance,
                    &text,
                    &text,
                    Intent::GreetingOrSmallTalk,
                    text::guess_language(&words),
                    None,
                    None,
                    now,
                    wall_ms,
                );
                let report = report(
                    Decision::Question,
                    None,
                    Some(Intent::GreetingOrSmallTalk),
                    Stage::Rules,
                    None,
                );
                Step::Done(self.announce(question, report))
            }
            Verdict::FollowUp => {
                let question = self.create(
                    &utterance,
                    &text,
                    &text,
                    Intent::ClarificationOrFollowUp,
                    text::guess_language(&words),
                    None,
                    latest_recent,
                    now,
                    wall_ms,
                );
                let report = report(
                    Decision::Question,
                    None,
                    Some(Intent::ClarificationOrFollowUp),
                    Stage::Rules,
                    None,
                );
                Step::Done(self.announce(question, report))
            }
            Verdict::Escalate(hints) => {
                let key = repeat_key(&text);
                if let Some(earlier) = self.exact_repeat(&key, now) {
                    return Step::Done(vec![classified(report(
                        Decision::Ignored,
                        Some(IgnoreReason::Repeat),
                        None,
                        Stage::Rules,
                        Some(earlier),
                    ))]);
                }
                let pending = Pending {
                    request: ClassifyRequest {
                        utterance: text.clone(),
                        speaker,
                        transcript,
                        recent_questions: self.recent_questions(),
                    },
                    utterance,
                    utterance_id,
                    hints,
                };
                if self.model_available {
                    Step::Classify(Box::new(pending))
                } else {
                    Step::Done(self.fallback(pending, None, now, wall_ms))
                }
            }
        }
    }

    pub fn resolve(
        &mut self,
        pending: Pending,
        result: Result<Classification, ClassifyError>,
        latency_ms: u32,
        now: u64,
        wall_ms: u64,
    ) -> Vec<PipelineEvent> {
        let c = match result {
            Ok(c) => c,
            Err(_) => return self.fallback(pending, Some(latency_ms), now, wall_ms),
        };
        let mut report = UtteranceReport {
            id: pending.utterance_id.clone(),
            source_id: source_id(pending.utterance.source).to_string(),
            segment_ids: pending.utterance.segment_ids.clone(),
            text: pending.request.utterance.clone(),
            decision: Decision::Ignored,
            reason: None,
            intent: Some(c.intent),
            stage: Stage::Llm,
            question_id: None,
            latency_ms: Some(latency_ms),
        };

        match (c.relation, c.context_ref.clone()) {
            (Relation::Repeat, Some(earlier)) => {
                report.reason = Some(IgnoreReason::Repeat);
                report.question_id = Some(earlier);
                vec![classified(report)]
            }
            // A question left out has no card to revise: the completed one is judged anew.
            (Relation::Continuation, Some(target)) if !self.hidden(&target) => {
                if let Some(tracked) = self.questions.iter_mut().find(|q| q.id == target) {
                    tracked.question = c.question.clone();
                }
                report.decision = Decision::Revision;
                report.question_id = Some(target.clone());
                vec![
                    PipelineEvent::QuestionRevised {
                        question_id: target,
                        text: c.question,
                    },
                    classified(report),
                ]
            }
            _ if !c.requires_response || c.intent == Intent::Ambiguous => {
                report.reason = Some(if c.intent == Intent::Ambiguous {
                    IgnoreReason::Ambiguous
                } else {
                    IgnoreReason::Statement
                });
                vec![classified(report)]
            }
            (relation, context_ref) => {
                let context_ref = if relation == Relation::FollowUp {
                    context_ref
                } else {
                    None
                };
                let question = self.create(
                    &pending.utterance,
                    &pending.request.utterance,
                    &c.question,
                    c.intent,
                    c.language,
                    c.domain,
                    context_ref,
                    now,
                    wall_ms,
                );
                self.announce(question, report)
            }
        }
    }

    /// First-stage decision alone: explicit questions become cards, the rest is ignored.
    fn fallback(
        &mut self,
        pending: Pending,
        latency_ms: Option<u32>,
        now: u64,
        wall_ms: u64,
    ) -> Vec<PipelineEvent> {
        let hints = pending.hints;
        let report = UtteranceReport {
            id: pending.utterance_id,
            source_id: source_id(pending.utterance.source).to_string(),
            segment_ids: pending.utterance.segment_ids.clone(),
            text: pending.request.utterance.clone(),
            decision: Decision::Ignored,
            reason: Some(IgnoreReason::Statement),
            intent: Some(hints.intent),
            stage: Stage::Fallback,
            question_id: None,
            latency_ms,
        };
        if !hints.explicit_question {
            return vec![classified(report)];
        }
        let question = self.create(
            &pending.utterance,
            &pending.request.utterance,
            &pending.request.utterance,
            hints.intent,
            hints.language,
            hints.domain,
            None,
            now,
            wall_ms,
        );
        self.announce(question, report)
    }

    /// The card of a question just created, or, when it is out of scope, only its report.
    fn announce(
        &self,
        question: DetectedQuestion,
        mut report: UtteranceReport,
    ) -> Vec<PipelineEvent> {
        if self.hidden(&question.id) {
            report.decision = Decision::Ignored;
            report.reason = Some(IgnoreReason::NotTechnical);
            return vec![classified(report)];
        }
        report.decision = Decision::Question;
        report.reason = None;
        report.question_id = Some(question.id.clone());
        vec![
            PipelineEvent::QuestionDetected { question },
            classified(report),
        ]
    }

    /// A tracked question that got no card.
    fn hidden(&self, id: &str) -> bool {
        self.questions.iter().any(|q| q.id == id && !q.shown)
    }

    /// Everything gets a card, or with `technical_only`, technical questions and follow-ups on
    /// a question that has one.
    fn in_scope(&self, intent: Intent, context_ref: Option<&str>) -> bool {
        if !self.config.technical_only || intent.is_technical() {
            return true;
        }
        intent == Intent::ClarificationOrFollowUp
            && context_ref.is_some_and(|r| self.questions.iter().any(|q| q.id == r && q.shown))
    }

    #[allow(clippy::too_many_arguments)]
    fn create(
        &mut self,
        utterance: &Utterance,
        original: &str,
        question: &str,
        intent: Intent,
        language: Language,
        domain: Option<&'static str>,
        context_ref: Option<String>,
        now: u64,
        wall_ms: u64,
    ) -> DetectedQuestion {
        let shown = self.in_scope(intent, context_ref.as_deref());
        // A card never refers to a question that has none.
        let context_ref = context_ref.filter(|r| !self.hidden(r));
        self.next_question += 1;
        let id = format!("{}-q{}", self.session_id, self.next_question);
        self.questions.push_back(Tracked {
            id: id.clone(),
            question: question.to_string(),
            intent,
            key: repeat_key(original),
            at: now,
            shown,
        });
        while self.questions.len() > self.config.recent_questions {
            self.questions.pop_front();
        }
        DetectedQuestion {
            id,
            session_id: self.session_id.clone(),
            source: "speech",
            source_segment_ids: utterance.segment_ids.clone(),
            original_utterance: clip(original),
            normalized_question: clip(question),
            intent,
            language,
            domain: domain.map(str::to_string),
            context_ref,
            detected_at: wall_ms,
        }
    }

    fn remember(&mut self, speaker: Speaker, text: &str) {
        self.context.push_back(ContextLine {
            speaker,
            text: text.to_string(),
        });
        while self.context.len() > self.config.context_lines {
            self.context.pop_front();
        }
    }

    fn recent_questions(&self) -> Vec<RecentQuestion> {
        self.questions
            .iter()
            .map(|q| RecentQuestion {
                id: q.id.clone(),
                text: q.question.clone(),
                intent: q.intent,
            })
            .collect()
    }

    fn exact_repeat(&self, key: &[String], now: u64) -> Option<String> {
        self.questions
            .iter()
            .rev()
            .filter(|q| now.saturating_sub(q.at) <= self.config.repeat_window_ms)
            .find(|q| q.key == key)
            .map(|q| q.id.clone())
    }

    fn recent_with_intent(&self, intent: Intent, now: u64) -> Option<String> {
        self.questions
            .iter()
            .rev()
            .filter(|q| now.saturating_sub(q.at) <= self.config.repeat_window_ms)
            .find(|q| q.intent == intent)
            .map(|q| q.id.clone())
    }
}

fn classified(utterance: UtteranceReport) -> PipelineEvent {
    PipelineEvent::UtteranceClassified { utterance }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::classifier::Urgency;
    use crate::intent::types::ClassifyErrorCode;

    const SESSION: &str = "session-1";

    fn detector() -> Detector {
        Detector::new(SESSION, DetectorConfig::DEFAULT, true)
    }

    fn said(source: SourceKind, id: &str, text: &str) -> Utterance {
        Utterance {
            source,
            segment_ids: vec![id.to_string()],
            text: text.to_string(),
        }
    }

    fn interviewer(id: &str, text: &str) -> Utterance {
        said(SourceKind::System, id, text)
    }

    fn classification(intent: Intent, requires: bool, relation: Relation) -> Classification {
        Classification {
            question: "Question normalisée ?".into(),
            language: Language::Fr,
            intent,
            requires_response: requires,
            relation,
            context_ref: None,
            domain: Some("frontend"),
            urgency: Urgency::High,
            confidence: 0.9,
            timing: None,
        }
    }

    fn done(step: Step) -> Vec<PipelineEvent> {
        match step {
            Step::Done(events) => events,
            other => panic!("expected Done, got {other:?}"),
        }
    }

    fn pending(step: Step) -> Pending {
        match step {
            Step::Classify(p) => *p,
            other => panic!("expected Classify, got {other:?}"),
        }
    }

    fn detected(events: &[PipelineEvent]) -> Vec<&DetectedQuestion> {
        events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::QuestionDetected { question } => Some(question),
                _ => None,
            })
            .collect()
    }

    fn report(events: &[PipelineEvent]) -> &UtteranceReport {
        events
            .iter()
            .find_map(|e| match e {
                PipelineEvent::UtteranceClassified { utterance } => Some(utterance),
                _ => None,
            })
            .expect("every utterance is reported")
    }

    /// Asks a question through the model and returns its id.
    fn ask(d: &mut Detector, id: &str, text: &str, intent: Intent, now: u64) -> String {
        let p = pending(d.on_utterance(interviewer(id, text), now, 1_000 + now));
        let events = d.resolve(
            p,
            Ok(classification(intent, true, Relation::New)),
            400,
            now,
            0,
        );
        detected(&events)[0].id.clone()
    }

    #[test]
    fn a_greeting_gets_a_card_at_once_and_only_once() {
        let mut d = detector();
        let events = done(d.on_utterance(interviewer("s-1-0", "Bonjour, merci d'être là."), 0, 42));
        let q = detected(&events)[0];
        assert_eq!(q.id, "session-1-q1");
        assert_eq!(q.intent, Intent::GreetingOrSmallTalk);
        assert_eq!(q.source_segment_ids, ["s-1-0"]);
        assert_eq!(q.detected_at, 42);
        assert_eq!(report(&events).stage, Stage::Rules);

        let again = done(d.on_utterance(interviewer("s-1-1", "Bonjour bonjour !"), 5_000, 0));
        assert!(detected(&again).is_empty());
        assert_eq!(report(&again).reason, Some(IgnoreReason::Repeat));
        assert_eq!(report(&again).question_id.as_deref(), Some("session-1-q1"));

        // Regression (found by the corpus): small talk that asks something is not a repeat.
        let check = done(d.on_utterance(
            interviewer("s-1-2", "Est-ce que tu m'entends bien ?"),
            8_000,
            0,
        ));
        assert_eq!(detected(&check)[0].intent, Intent::GreetingOrSmallTalk);
    }

    #[test]
    fn acknowledgements_and_hesitations_are_ignored_without_the_model() {
        let mut d = detector();
        for (i, text) in ["D'accord, très bien.", "Euh…", "Ok."].iter().enumerate() {
            let events = done(d.on_utterance(interviewer(&format!("s-1-{i}"), text), 0, 0));
            assert!(detected(&events).is_empty(), "{text}");
            assert_eq!(report(&events).decision, Decision::Ignored);
        }
    }

    #[test]
    fn statements_are_suppressed_when_the_model_says_no_answer_is_expected() {
        let mut d = detector();
        let p = pending(d.on_utterance(interviewer("s-1-0", "On utilise Kafka chez nous."), 0, 0));
        let events = d.resolve(
            p,
            Ok(classification(
                Intent::NonQuestionStatement,
                false,
                Relation::New,
            )),
            300,
            0,
            0,
        );
        assert!(detected(&events).is_empty());
        let r = report(&events);
        assert_eq!(r.reason, Some(IgnoreReason::Statement));
        assert_eq!(r.stage, Stage::Llm);
        assert_eq!(r.latency_ms, Some(300));
    }

    #[test]
    fn ambiguous_fragments_wait_for_more_context() {
        let mut d = detector();
        let p = pending(d.on_utterance(interviewer("s-1-0", "Alors pour la suite, euh"), 0, 0));
        let events = d.resolve(
            p,
            Ok(classification(Intent::Ambiguous, true, Relation::New)),
            1,
            0,
            0,
        );
        assert!(detected(&events).is_empty());
        assert_eq!(report(&events).reason, Some(IgnoreReason::Ambiguous));
        // The fragment is part of the next request's transcript.
        let next = pending(d.on_utterance(
            interviewer("s-1-1", "Comment tu sécuriserais une API ?"),
            1,
            0,
        ));
        assert_eq!(
            next.request.transcript.last().unwrap().text,
            "Alors pour la suite, euh"
        );
    }

    #[test]
    fn a_bare_follow_up_references_the_latest_question() {
        let mut d = detector();
        let first = ask(
            &mut d,
            "s-1-0",
            "Pourquoi Pinia plutôt que Vuex ?",
            Intent::TechnicalQuestion,
            0,
        );
        let events = done(d.on_utterance(interviewer("s-1-1", "Et pourquoi ?"), 30_000, 0));
        let q = detected(&events)[0];
        assert_eq!(q.intent, Intent::ClarificationOrFollowUp);
        assert_eq!(q.context_ref.as_deref(), Some(first.as_str()));
        // Long after, the same words are no longer a bare follow-up: the model decides.
        assert!(matches!(
            d.on_utterance(interviewer("s-1-2", "Pourquoi ?"), 500_000, 0),
            Step::Classify(_)
        ));
    }

    #[test]
    fn model_follow_ups_keep_their_reference_and_new_questions_drop_it() {
        let mut d = detector();
        let first = ask(
            &mut d,
            "s-1-0",
            "Comment tu tries ce tableau ?",
            Intent::CodingProblem,
            0,
        );
        let p = pending(d.on_utterance(interviewer("s-1-1", "Et la complexité, du coup ?"), 10, 0));
        assert_eq!(p.request.recent_questions[0].id, first);
        let mut c = classification(Intent::ClarificationOrFollowUp, true, Relation::FollowUp);
        c.context_ref = Some(first.clone());
        let events = d.resolve(p, Ok(c), 1, 10, 0);
        assert_eq!(
            detected(&events)[0].context_ref.as_deref(),
            Some(first.as_str())
        );

        let p = pending(d.on_utterance(interviewer("s-1-2", "Tu connais Docker ?"), 20, 0));
        let mut c = classification(Intent::TechnicalQuestion, true, Relation::New);
        c.context_ref = Some(first);
        let events = d.resolve(p, Ok(c), 1, 20, 0);
        assert_eq!(detected(&events)[0].context_ref, None);
    }

    #[test]
    fn a_continuation_revises_the_question_instead_of_creating_a_card() {
        let mut d = detector();
        let first = ask(
            &mut d,
            "s-1-0",
            "Comment tu gères le state ?",
            Intent::TechnicalQuestion,
            0,
        );
        let p = pending(d.on_utterance(interviewer("s-1-1", "Avec Vue 3, je veux dire."), 10, 0));
        let mut c = classification(Intent::TechnicalQuestion, true, Relation::Continuation);
        c.context_ref = Some(first.clone());
        c.question = "Comment gères-tu le state avec Vue 3 ?".into();
        let events = d.resolve(p, Ok(c), 1, 10, 0);
        assert!(detected(&events).is_empty());
        assert_eq!(
            events[0],
            PipelineEvent::QuestionRevised {
                question_id: first.clone(),
                text: "Comment gères-tu le state avec Vue 3 ?".into()
            }
        );
        assert_eq!(report(&events).decision, Decision::Revision);
        // The revised text is what the model sees next time.
        let next = pending(d.on_utterance(interviewer("s-1-2", "Et côté tests ?"), 20, 0));
        assert_eq!(
            next.request.recent_questions[0].text,
            "Comment gères-tu le state avec Vue 3 ?"
        );
    }

    #[test]
    fn repeats_never_create_a_second_card() {
        let mut d = detector();
        let first = ask(
            &mut d,
            "s-1-0",
            "Tu connais Kubernetes ?",
            Intent::TechnicalQuestion,
            0,
        );
        // Identical words: caught before the model.
        let events = done(d.on_utterance(
            interviewer("s-2-0", "Euh, tu connais Kubernetes ?"),
            5_000,
            0,
        ));
        assert_eq!(report(&events).reason, Some(IgnoreReason::Repeat));
        assert_eq!(report(&events).question_id.as_deref(), Some(first.as_str()));
        // Rephrased: the model says so.
        let p = pending(d.on_utterance(
            interviewer("s-2-1", "Kubernetes, tu as déjà utilisé ?"),
            6_000,
            0,
        ));
        let mut c = classification(Intent::TechnicalQuestion, true, Relation::Repeat);
        c.context_ref = Some(first.clone());
        let events = d.resolve(p, Ok(c), 1, 6_000, 0);
        assert!(detected(&events).is_empty());
        assert_eq!(report(&events).question_id.as_deref(), Some(first.as_str()));
        // Long after, the same words are a new question again.
        assert!(matches!(
            d.on_utterance(interviewer("s-3-0", "Tu connais Kubernetes ?"), 600_000, 0),
            Step::Classify(_)
        ));
    }

    #[test]
    fn similar_but_different_questions_are_both_kept() {
        let mut d = detector();
        ask(
            &mut d,
            "s-1-0",
            "Quelle différence entre let et const ?",
            Intent::TechnicalQuestion,
            0,
        );
        assert!(matches!(
            d.on_utterance(
                interviewer("s-1-1", "Quelle différence entre let et var ?"),
                1_000,
                0
            ),
            Step::Classify(_)
        ));
    }

    #[test]
    fn candidate_speech_is_context_only_unless_enabled_for_tests() {
        let mut d = detector();
        let mic = said(
            SourceKind::Microphone,
            "m-1-0",
            "Comment je gère le state ? Avec Pinia.",
        );
        assert!(matches!(d.on_utterance(mic.clone(), 0, 0), Step::Context));
        let p = pending(d.on_utterance(interviewer("s-1-0", "Pourquoi Pinia ?"), 1, 0));
        assert_eq!(p.request.transcript[0].speaker, Speaker::Candidate);

        let mut testing = Detector::new(
            SESSION,
            DetectorConfig {
                include_microphone: true,
                ..DetectorConfig::DEFAULT
            },
            true,
        );
        let p = pending(testing.on_utterance(mic, 0, 0));
        assert_eq!(p.request.speaker, Speaker::Interviewer);
    }

    #[test]
    fn without_the_model_explicit_questions_still_become_cards() {
        let mut d = Detector::new(SESSION, DetectorConfig::DEFAULT, false);
        let events = done(d.on_utterance(
            interviewer("s-1-0", "Comment tu gères le state management dans Vue ?"),
            0,
            0,
        ));
        let q = detected(&events)[0];
        assert_eq!(q.intent, Intent::TechnicalQuestion);
        assert_eq!(q.domain.as_deref(), Some("frontend"));
        assert_eq!(report(&events).stage, Stage::Fallback);

        let events =
            done(d.on_utterance(interviewer("s-1-1", "On utilise Kafka chez nous."), 1, 0));
        assert!(detected(&events).is_empty());
        assert_eq!(report(&events).reason, Some(IgnoreReason::Statement));
    }

    #[test]
    fn a_model_failure_falls_back_to_the_first_stage() {
        let mut d = detector();
        let p = pending(d.on_utterance(interviewer("s-1-0", "Explique-moi les closures."), 0, 0));
        let error = ClassifyError::new(ClassifyErrorCode::Timeout, "slow");
        let events = d.resolve(p, Err(error), 10_000, 0, 0);
        assert_eq!(
            detected(&events)[0].normalized_question,
            "Explique-moi les closures."
        );
        let r = report(&events);
        assert_eq!(r.stage, Stage::Fallback);
        assert_eq!(r.latency_ms, Some(10_000));
    }

    #[test]
    fn technical_only_leaves_out_other_questions_and_their_follow_ups() {
        let mut d = Detector::new(
            SESSION,
            DetectorConfig {
                technical_only: true,
                ..DetectorConfig::DEFAULT
            },
            true,
        );
        let greeting =
            done(d.on_utterance(interviewer("s-1-0", "Bonjour, merci d'être là."), 0, 0));
        assert!(detected(&greeting).is_empty());
        assert_eq!(report(&greeting).decision, Decision::Ignored);
        assert_eq!(report(&greeting).reason, Some(IgnoreReason::NotTechnical));

        let p = pending(d.on_utterance(
            interviewer("s-1-1", "Parle-moi d'un conflit dans ton équipe."),
            10,
            0,
        ));
        let behavioral = d.resolve(
            p,
            Ok(classification(
                Intent::BehavioralQuestion,
                true,
                Relation::New,
            )),
            1,
            10,
            0,
        );
        assert!(detected(&behavioral).is_empty());
        assert_eq!(report(&behavioral).intent, Some(Intent::BehavioralQuestion));
        // Its follow-up is left out too.
        let why = done(d.on_utterance(interviewer("s-1-2", "Et pourquoi ?"), 20, 0));
        assert_eq!(report(&why).reason, Some(IgnoreReason::NotTechnical));

        let first = ask(
            &mut d,
            "s-1-3",
            "Comment fonctionne l'event loop ?",
            Intent::TechnicalQuestion,
            30,
        );
        let why = done(d.on_utterance(interviewer("s-1-4", "Et pourquoi ?"), 40, 0));
        assert_eq!(
            detected(&why)[0].context_ref.as_deref(),
            Some(first.as_str())
        );
    }

    #[test]
    fn context_and_recent_questions_stay_bounded() {
        let mut d = detector();
        for i in 0..30u64 {
            ask(
                &mut d,
                &format!("s-1-{i}"),
                &format!("Question numéro {i} ?"),
                Intent::TechnicalQuestion,
                i,
            );
        }
        let p = pending(d.on_utterance(interviewer("s-2-0", "Dernière question ?"), 100, 0));
        assert_eq!(
            p.request.transcript.len(),
            DetectorConfig::DEFAULT.context_lines
        );
        assert_eq!(
            p.request.recent_questions.len(),
            DetectorConfig::DEFAULT.recent_questions
        );
        assert_eq!(
            p.request.recent_questions.last().unwrap().id,
            "session-1-q30"
        );
    }
}
