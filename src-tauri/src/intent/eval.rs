//! Question-detection evaluation (PRD §12): replays the labeled dialogues of
//! `contracts/intent-corpus.v1.json` through the real assembler, detector and (optionally)
//! classifier on a virtual clock, then scores the cards against the labels. Used by the unit
//! tests (rules only, free) and by `intent-bench` (with Claude).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::assembler::{Assembler, AssemblerConfig, Utterance};
use super::classifier::{CallTiming, Classifier};
use super::detector::{Detector, DetectorConfig, Step};
use super::types::{DetectedQuestion, Intent, Language, UtteranceReport};
use crate::audio::types::SourceKind;
use crate::events::PipelineEvent;

pub const CORPUS: &str = include_str!("../../../contracts/intent-corpus.v1.json");
/// Written after prompt v3 was frozen and never used for tuning: the honest estimate.
pub const HOLDOUT: &str = include_str!("../../../contracts/intent-corpus-holdout.v1.json");
/// Silence before a turn when the corpus does not say.
pub const DEFAULT_GAP_MS: u64 = 2_500;

#[derive(Debug, Deserialize)]
pub struct Corpus {
    pub version: u32,
    pub dialogues: Vec<Dialogue>,
}

#[derive(Debug, Deserialize)]
pub struct Dialogue {
    pub id: String,
    pub language: Language,
    pub turns: Vec<Turn>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CorpusSpeaker {
    Interviewer,
    Candidate,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Turn {
    pub id: String,
    pub speaker: CorpusSpeaker,
    pub text: String,
    #[serde(default)]
    pub gap_ms: Option<u64>,
    #[serde(default)]
    pub expect: Option<Expect>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Expect {
    pub respond: Option<bool>,
    #[serde(default)]
    pub intents: Vec<Intent>,
    #[serde(default)]
    pub follow_up_of: Vec<String>,
    pub revises: Option<String>,
    pub repeat_of: Option<String>,
    #[serde(default)]
    pub merge: bool,
}

pub fn load(json: &str) -> Result<Corpus, String> {
    let corpus: Corpus = serde_json::from_str(json).map_err(|e| e.to_string())?;
    corpus.validate()?;
    Ok(corpus)
}

impl Corpus {
    /// Labels must be consistent, or the scores mean nothing.
    pub fn validate(&self) -> Result<(), String> {
        let mut dialogues = HashSet::new();
        for d in &self.dialogues {
            if !dialogues.insert(&d.id) {
                return Err(format!("duplicate dialogue {}", d.id));
            }
            let mut questions = HashSet::new();
            let mut seen = HashSet::new();
            for (i, turn) in d.turns.iter().enumerate() {
                let at = format!("{}/{}", d.id, turn.id);
                if !seen.insert(&turn.id) {
                    return Err(format!("{at}: duplicate turn id"));
                }
                match (turn.speaker, &turn.expect) {
                    (CorpusSpeaker::Candidate, Some(_)) => {
                        return Err(format!("{at}: candidate turns are context only"));
                    }
                    (CorpusSpeaker::Interviewer, None) => {
                        return Err(format!("{at}: interviewer turn without a label"));
                    }
                    (CorpusSpeaker::Interviewer, Some(e)) => {
                        let kinds = [
                            e.respond.is_some(),
                            e.revises.is_some(),
                            e.repeat_of.is_some(),
                            e.merge,
                        ];
                        if kinds.iter().filter(|k| **k).count() != 1 {
                            return Err(format!(
                                "{at}: exactly one of respond/revises/repeatOf/merge"
                            ));
                        }
                        let refs = e.follow_up_of.iter().chain(&e.revises).chain(&e.repeat_of);
                        for r in refs {
                            if !questions.contains(r) {
                                return Err(format!("{at}: {r} is not an earlier question"));
                            }
                        }
                        if e.merge
                            && !d.turns[i + 1..]
                                .iter()
                                .find(|t| t.speaker == CorpusSpeaker::Interviewer)
                                .and_then(|t| t.expect.as_ref())
                                .is_some_and(|x| x.respond == Some(true))
                        {
                            return Err(format!("{at}: a merge must precede a question"));
                        }
                        if e.respond == Some(true) {
                            questions.insert(&turn.id);
                        }
                    }
                    (CorpusSpeaker::Candidate, None) => {}
                }
            }
        }
        Ok(())
    }
}

pub fn segment_id(dialogue: &Dialogue, turn: &Turn) -> String {
    format!("{}:{}", dialogue.id, turn.id)
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueOutcome {
    pub dialogue: String,
    pub questions: Vec<DetectedQuestion>,
    /// (question id, revised text)
    pub revisions: Vec<(String, String)>,
    pub reports: Vec<UtteranceReport>,
    pub model_latencies_ms: Vec<u32>,
    /// Per-call breakdown, when the classifier streams (first token, decision, tokens).
    pub timings: Vec<CallTiming>,
    pub model_errors: Vec<String>,
    /// A key or billing error: the rest of the run would only measure the rules.
    pub fatal_error: Option<String>,
}

/// Replays one dialogue on a virtual clock. Model calls take real time but not virtual time.
pub async fn simulate(dialogue: &Dialogue, classifier: Option<&dyn Classifier>) -> DialogueOutcome {
    let mut detector = Detector::new(
        format!("eval-{}", dialogue.id),
        DetectorConfig::DEFAULT,
        classifier.is_some(),
    );
    let mut assembler = Assembler::new(AssemblerConfig::DEFAULT);
    let mut outcome = DialogueOutcome {
        dialogue: dialogue.id.clone(),
        ..DialogueOutcome::default()
    };
    let mut now = 0;
    for turn in &dialogue.turns {
        now += turn.gap_ms.unwrap_or(DEFAULT_GAP_MS);
        for u in assembler.poll(now) {
            handle(&mut detector, classifier, u, now, &mut outcome).await;
        }
        let source = match turn.speaker {
            CorpusSpeaker::Interviewer => SourceKind::System,
            CorpusSpeaker::Candidate => SourceKind::Microphone,
        };
        if let Some(u) = assembler.on_final(source, &segment_id(dialogue, turn), &turn.text, now) {
            handle(&mut detector, classifier, u, now, &mut outcome).await;
        }
    }
    for u in assembler.flush() {
        handle(&mut detector, classifier, u, now, &mut outcome).await;
    }
    outcome
}

async fn handle(
    detector: &mut Detector,
    classifier: Option<&dyn Classifier>,
    utterance: Utterance,
    now: u64,
    outcome: &mut DialogueOutcome,
) {
    let events = match detector.on_utterance(utterance, now, now) {
        Step::Done(events) => events,
        Step::Context => return,
        Step::Classify(pending) => {
            let Some(model) = classifier else { return };
            let started = Instant::now();
            let result = model.classify(&pending.request).await;
            let latency = started.elapsed().as_millis() as u32;
            match &result {
                // Only successful calls measure classification latency.
                Ok(c) => {
                    outcome.model_latencies_ms.push(latency);
                    outcome.timings.extend(c.timing);
                }
                Err(e) => {
                    outcome.model_errors.push(e.to_string());
                    // Like the engine: a rejected key or no credit means rules only from now on.
                    if e.code.is_fatal() {
                        detector.disable_model();
                        outcome.fatal_error.get_or_insert_with(|| e.to_string());
                    }
                }
            }
            detector.resolve(*pending, result, latency, now, now)
        }
    };
    for event in events {
        match event {
            PipelineEvent::QuestionDetected { question } => outcome.questions.push(question),
            PipelineEvent::QuestionRevised { question_id, text } => {
                outcome.revisions.push((question_id, text))
            }
            PipelineEvent::UtteranceClassified { utterance } => outcome.reports.push(utterance),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub expected: u32,
    pub correct: u32,
}

impl Tally {
    fn add(&mut self, ok: bool) {
        self.expected += 1;
        self.correct += u32::from(ok);
    }

    pub fn rate(&self) -> Option<f64> {
        (self.expected > 0).then(|| f64::from(self.correct) / f64::from(self.expected))
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageScores {
    pub questions: Tally,
    pub statements: Tally,
    pub intents: Tally,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scores {
    /// Expected questions that got a card (recall).
    pub questions: Tally,
    /// Detected questions whose intent is acceptable.
    pub intents: Tally,
    /// Follow-ups whose card references the right earlier card.
    pub follow_ups: Tally,
    /// Turns that must not create a card, and did not (1 − false-positive rate).
    pub statements: Tally,
    /// Turns that must revise an earlier card instead of creating one.
    pub revisions: Tally,
    /// Repeated questions that did not create a second card.
    pub repeats: Tally,
    /// Cut turns that ended up in the same card as their continuation.
    pub merges: Tally,
    pub by_language: BTreeMap<String, LanguageScores>,
    /// One line per wrong decision, for reading the failures.
    pub failures: Vec<String>,
}

/// Scores the outcomes of `dialogues`, given in the same order.
pub fn score<'a>(
    dialogues: impl IntoIterator<Item = &'a Dialogue>,
    outcomes: &[DialogueOutcome],
) -> Scores {
    let mut scores = Scores::default();
    for (dialogue, outcome) in dialogues.into_iter().zip(outcomes) {
        let card_of = |turn: &str| -> Option<&DetectedQuestion> {
            let segment = format!("{}:{turn}", dialogue.id);
            outcome
                .questions
                .iter()
                .find(|q| q.source_segment_ids.contains(&segment))
        };
        let revised: HashMap<&str, ()> = outcome
            .revisions
            .iter()
            .map(|(id, _)| (id.as_str(), ()))
            .collect();
        let language = serde_json::to_value(dialogue.language)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();

        for (i, turn) in dialogue.turns.iter().enumerate() {
            let Some(expect) = &turn.expect else { continue };
            let at = format!("{}/{} «{}»", dialogue.id, turn.id, turn.text);
            let card = card_of(&turn.id);
            let lang = scores.by_language.entry(language.clone()).or_default();

            if expect.respond == Some(true) {
                scores.questions.add(card.is_some());
                lang.questions.add(card.is_some());
                let Some(card) = card else {
                    scores.failures.push(format!("missed question: {at}"));
                    continue;
                };
                let intent_ok = expect.intents.is_empty() || expect.intents.contains(&card.intent);
                scores.intents.add(intent_ok);
                lang.intents.add(intent_ok);
                if !intent_ok {
                    scores.failures.push(format!(
                        "intent {:?} not in {:?}: {at}",
                        card.intent, expect.intents
                    ));
                }
                if !expect.follow_up_of.is_empty() {
                    let ok = expect.follow_up_of.iter().any(|r| {
                        card_of(r)
                            .is_some_and(|earlier| card.context_ref.as_ref() == Some(&earlier.id))
                    });
                    scores.follow_ups.add(ok);
                    if !ok {
                        scores.failures.push(format!(
                            "follow-up reference {:?} not to {:?}: {at}",
                            card.context_ref, expect.follow_up_of
                        ));
                    }
                }
            } else if expect.respond == Some(false) {
                scores.statements.add(card.is_none());
                lang.statements.add(card.is_none());
                if card.is_some() {
                    scores.failures.push(format!("false positive: {at}"));
                }
            } else if let Some(target) = &expect.revises {
                let ok = card.is_none()
                    && card_of(target).is_some_and(|t| revised.contains_key(t.id.as_str()));
                scores.revisions.add(ok);
                if !ok {
                    scores
                        .failures
                        .push(format!("not a revision of {target}: {at}"));
                }
            } else if expect.repeat_of.is_some() {
                scores.repeats.add(card.is_none());
                if card.is_some() {
                    scores.failures.push(format!("repeat created a card: {at}"));
                }
            } else if expect.merge {
                let next = dialogue.turns[i + 1..]
                    .iter()
                    .find(|t| t.speaker == CorpusSpeaker::Interviewer);
                let ok = match (card, next.and_then(|n| card_of(&n.id))) {
                    (Some(a), Some(b)) => a.id == b.id,
                    _ => false,
                };
                scores.merges.add(ok);
                if !ok {
                    scores.failures.push(format!("cut turn not merged: {at}"));
                }
            }
        }
    }
    scores
}

/// Nearest-rank percentile; `None` for no samples.
pub fn percentile(samples: &[u32], p: f64) -> Option<u32> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, sorted.len()) - 1])
}

#[cfg(test)]
mod tests {
    use futures_util::future::BoxFuture;

    use super::*;
    use crate::intent::classifier::{Classification, ClassifyRequest, Relation, Urgency};
    use crate::intent::types::ClassifyError;

    fn corpus() -> Corpus {
        load(CORPUS).expect("the shared corpus is valid")
    }

    #[test]
    fn the_shared_corpus_is_consistent_and_covers_every_case() {
        let c = corpus();
        assert_eq!(c.version, 1);
        let expects: Vec<&Expect> = c
            .dialogues
            .iter()
            .flat_map(|d| d.turns.iter().filter_map(|t| t.expect.as_ref()))
            .collect();
        let count = |f: &dyn Fn(&Expect) -> bool| expects.iter().filter(|e| f(e)).count();
        assert!(count(&|e| e.respond == Some(true)) >= 40);
        assert!(count(&|e| e.respond == Some(false)) >= 25);
        assert!(count(&|e| !e.follow_up_of.is_empty()) >= 6);
        assert!(count(&|e| e.revises.is_some()) >= 2);
        assert!(count(&|e| e.repeat_of.is_some()) >= 2);
        assert!(count(&|e| e.merge) >= 1);
        for language in [Language::Fr, Language::En, Language::Mixed] {
            assert!(c.dialogues.iter().any(|d| d.language == language));
        }
    }

    #[test]
    fn the_holdout_corpus_is_consistent_and_disjoint_from_the_main_one() {
        let holdout = load(HOLDOUT).expect("the hold-out corpus is valid");
        let main = corpus();
        let texts: HashSet<&str> = main
            .dialogues
            .iter()
            .flat_map(|d| d.turns.iter().map(|t| t.text.as_str()))
            .collect();
        for d in &holdout.dialogues {
            assert!(main.dialogues.iter().all(|m| m.id != d.id));
            for t in &d.turns {
                assert!(!texts.contains(t.text.as_str()), "{} reused", t.text);
            }
        }
    }

    #[test]
    fn inconsistent_labels_are_rejected() {
        let bad = r#"{"version":1,"dialogues":[{"id":"d","language":"fr","turns":[
            {"id":"t1","speaker":"interviewer","text":"Pourquoi ?","expect":{"respond":true,"followUpOf":["t0"]}}]}]}"#;
        assert!(load(bad).unwrap_err().contains("not an earlier question"));
        let unlabeled = r#"{"version":1,"dialogues":[{"id":"d","language":"fr","turns":[
            {"id":"t1","speaker":"interviewer","text":"Bonjour"}]}]}"#;
        assert!(load(unlabeled).is_err());
    }

    #[test]
    fn percentiles_use_the_nearest_rank() {
        assert_eq!(percentile(&[], 50.0), None);
        assert_eq!(percentile(&[300, 100, 200], 50.0), Some(200));
        assert_eq!(percentile(&[300, 100, 200, 400], 95.0), Some(400));
    }

    /// Without the model: greetings, bare follow-ups, explicit questions, repeats and merges
    /// work; indirect questions and rhetorical ones are where the model is needed. These
    /// floors guard the first stage against regressions; the bench measures the full pipeline.
    #[tokio::test]
    async fn the_rules_alone_give_a_safe_baseline_on_the_corpus() {
        let c = corpus();
        let mut outcomes = Vec::new();
        for d in &c.dialogues {
            outcomes.push(simulate(d, None).await);
        }
        let s = score(&c.dialogues, &outcomes);
        assert!(s.questions.rate().unwrap() >= 0.75, "{:#?}", s.failures);
        assert!(s.statements.rate().unwrap() >= 0.75, "{:#?}", s.failures);
        assert_eq!(s.merges.rate(), Some(1.0), "{:#?}", s.failures);
        assert!(s.repeats.correct >= 1, "{:#?}", s.failures);
        // Every bare "Pourquoi ?" / "Why?" references the question it follows.
        let bare: Vec<_> = s
            .failures
            .iter()
            .filter(|f| f.contains("«Pourquoi ?»") || f.contains("«Why?»"))
            .collect();
        assert!(bare.is_empty(), "{bare:#?}");
    }

    /// A perfect model on the corpus: its answers are derived from the labels themselves.
    /// Identical texts (a repeated question) are told apart by taking turns in order.
    #[derive(Default)]
    struct OracleModel {
        used: std::sync::Mutex<HashSet<String>>,
    }

    impl Classifier for OracleModel {
        fn model(&self) -> &'static str {
            "oracle"
        }

        fn classify<'a>(
            &'a self,
            request: &'a ClassifyRequest,
        ) -> BoxFuture<'a, Result<Classification, ClassifyError>> {
            Box::pin(async move {
                let c = corpus();
                let mut used = self.used.lock().unwrap();
                let (dialogue, turn) = c
                    .dialogues
                    .iter()
                    .flat_map(|d| d.turns.iter().map(move |t| (d, t)))
                    .find(|(d, t)| {
                        request.utterance.ends_with(t.text.as_str())
                            && !used.contains(&segment_id(d, t))
                    })
                    .expect("utterance from the corpus");
                used.insert(segment_id(dialogue, turn));
                let e = turn.expect.as_ref().unwrap();
                // The earlier question as listed in the request (same normalized text).
                let alias = |turn_id: &str| {
                    let text = &dialogue
                        .turns
                        .iter()
                        .find(|t| t.id == turn_id)
                        .unwrap()
                        .text;
                    request
                        .recent_questions
                        .iter()
                        .find(|q| q.text == *text)
                        .map(|q| q.id.clone())
                };
                let (relation, context_ref) = if let Some(t) = &e.revises {
                    (Relation::Continuation, alias(t))
                } else if let Some(t) = &e.repeat_of {
                    (Relation::Repeat, alias(t))
                } else if let Some(t) = e.follow_up_of.first() {
                    (Relation::FollowUp, alias(t))
                } else {
                    (Relation::New, None)
                };
                // A revision keeps the target's text, so later references still find it.
                let question = match &e.revises {
                    Some(t) => dialogue
                        .turns
                        .iter()
                        .find(|x| x.id == *t)
                        .unwrap()
                        .text
                        .clone(),
                    None => request.utterance.clone(),
                };
                Ok(Classification {
                    question,
                    language: Language::Fr,
                    intent: e
                        .intents
                        .first()
                        .copied()
                        .unwrap_or(Intent::NonQuestionStatement),
                    requires_response: e.respond.unwrap_or(false),
                    relation,
                    context_ref,
                    domain: None,
                    urgency: Urgency::Normal,
                    confidence: 1.0,
                    timing: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn the_harness_scores_a_perfect_model_at_100_percent() {
        let c = corpus();
        let oracle = OracleModel::default();
        let mut outcomes = Vec::new();
        for d in &c.dialogues {
            outcomes.push(simulate(d, Some(&oracle)).await);
        }
        let s = score(&c.dialogues, &outcomes);
        for (name, tally) in [
            ("questions", s.questions),
            ("intents", s.intents),
            ("follow-ups", s.follow_ups),
            ("statements", s.statements),
            ("revisions", s.revisions),
            ("repeats", s.repeats),
            ("merges", s.merges),
        ] {
            assert_eq!(tally.rate(), Some(1.0), "{name}: {:#?}", s.failures);
        }
    }
}
