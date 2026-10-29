//! Question detection for one live session: two tasks fed by the transcript events of every
//! STT session.
//!
//! - **assembler task**: merges cut turns and releases utterances on time (its timers never
//!   wait for the model);
//! - **classifier task**: runs the detector and the model one utterance at a time, in order,
//!   so each classification sees the context and the questions of the previous ones.
//!
//! The engine lives as long as an `QuestionEngine` handle: when the STT sessions end and drop
//! theirs, pending utterances are flushed, classified, and the tasks finish.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::assembler::{Assembler, AssemblerConfig, Utterance};
use super::classifier::Classifier;
use super::detector::{Detector, DetectorConfig, Step};
use super::types::{ClassifierState, ClassifierStatus, ClassifyError, ClassifyErrorCode};
use crate::audio::engine::Emit;
use crate::audio::types::SourceKind;
use crate::events::PipelineEvent;

pub struct EngineOptions {
    pub session_id: String,
    pub detector: DetectorConfig,
    pub assembler: AssemblerConfig,
    /// Told when an utterance goes to the model, before its decision.
    pub on_escalate: Option<OnEscalate>,
}

/// An interviewer utterance sent to the model, whose decision is not known yet. The answer
/// engine starts its answer early and keeps it only if it becomes a question.
#[derive(Clone, Debug, PartialEq)]
pub struct Escalated {
    pub segment_ids: Vec<String>,
    pub text: String,
}

pub type OnEscalate = Arc<dyn Fn(&Escalated) + Send + Sync>;

enum Input {
    Partial(SourceKind),
    Final {
        source: SourceKind,
        segment_id: String,
        text: String,
    },
}

/// Feeds transcript events to the engine. Cheap to clone; never blocks.
#[derive(Clone)]
pub struct QuestionEngine {
    input: mpsc::UnboundedSender<Input>,
}

impl QuestionEngine {
    pub fn observe(&self, event: &PipelineEvent) {
        let input = match event {
            PipelineEvent::TranscriptPartial { segment } => {
                Input::Partial(source_of(&segment.source_id))
            }
            PipelineEvent::TranscriptFinal { segment } => Input::Final {
                source: source_of(&segment.source_id),
                segment_id: segment.id.clone(),
                text: segment.text.clone(),
            },
            _ => return,
        };
        let _ = self.input.send(input);
    }
}

fn source_of(source_id: &str) -> SourceKind {
    if source_id == "microphone" {
        SourceKind::Microphone
    } else {
        SourceKind::System
    }
}

/// `classifier` is `None` when no model is configured; `unavailable` then says why.
pub fn spawn(
    runtime: &Handle,
    classifier: Option<Arc<dyn Classifier>>,
    unavailable: Option<ClassifyError>,
    options: EngineOptions,
    emit: Emit,
) -> QuestionEngine {
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    let (utterance_tx, utterance_rx) = mpsc::unbounded_channel();
    let epoch = Instant::now();
    runtime.spawn(assemble(
        Assembler::new(options.assembler),
        input_rx,
        utterance_tx,
        epoch,
    ));
    let detector = Detector::new(options.session_id, options.detector, classifier.is_some());
    runtime.spawn(classify(
        detector,
        classifier,
        unavailable,
        options.on_escalate,
        utterance_rx,
        emit,
        epoch,
    ));
    QuestionEngine { input: input_tx }
}

fn millis_since(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

async fn assemble(
    mut assembler: Assembler,
    mut input: mpsc::UnboundedReceiver<Input>,
    utterances: mpsc::UnboundedSender<Utterance>,
    epoch: Instant,
) {
    loop {
        let deadline = assembler
            .next_deadline()
            .map(|ms| epoch + std::time::Duration::from_millis(ms));
        tokio::select! {
            message = input.recv() => match message {
                Some(Input::Partial(source)) => assembler.on_partial(source, millis_since(epoch)),
                Some(Input::Final { source, segment_id, text }) => {
                    if let Some(u) = assembler.on_final(source, &segment_id, &text, millis_since(epoch)) {
                        let _ = utterances.send(u);
                    }
                }
                None => break,
            },
            _ = tokio::time::sleep_until(deadline.unwrap_or(epoch)), if deadline.is_some() => {
                for u in assembler.poll(millis_since(epoch)) {
                    let _ = utterances.send(u);
                }
            }
        }
    }
    for u in assembler.flush() {
        let _ = utterances.send(u);
    }
}

async fn classify(
    mut detector: Detector,
    classifier: Option<Arc<dyn Classifier>>,
    unavailable: Option<ClassifyError>,
    on_escalate: Option<OnEscalate>,
    mut utterances: mpsc::UnboundedReceiver<Utterance>,
    emit: Emit,
    epoch: Instant,
) {
    let mut status = ClassifierStatus {
        state: if classifier.is_some() {
            ClassifierState::Ready
        } else {
            ClassifierState::RulesOnly
        },
        model: classifier.as_ref().map(|c| c.model()),
        error: unavailable,
        last_latency_ms: None,
        classified: 0,
        failed: 0,
    };
    let mut model_usable = classifier.is_some();
    emit(PipelineEvent::ClassifierStatus(status.clone()));

    while let Some(utterance) = utterances.recv().await {
        let pending = match detector.on_utterance(utterance, millis_since(epoch), wall_ms()) {
            Step::Done(events) => {
                events.into_iter().for_each(|e| emit(e));
                continue;
            }
            Step::Context => continue,
            Step::Classify(pending) => pending,
        };
        let Some(model) = classifier.as_ref().filter(|_| model_usable) else {
            // Not reachable while the detector and `model_usable` agree; never drop the utterance.
            let error = status.error.clone().unwrap_or_else(|| {
                ClassifyError::new(ClassifyErrorCode::MissingKey, "no model configured")
            });
            let events = detector.resolve(*pending, Err(error), 0, millis_since(epoch), wall_ms());
            events.into_iter().for_each(|e| emit(e));
            continue;
        };
        if let Some(notify) = &on_escalate {
            notify(&pending.escalated());
        }
        let started = Instant::now();
        let result = model.classify(&pending.request).await;
        let latency = started.elapsed().as_millis().min(u32::MAX as u128) as u32;
        match &result {
            Ok(_) => {
                status.classified += 1;
                status.last_latency_ms = Some(latency);
                status.state = ClassifierState::Ready;
                status.error = None;
            }
            Err(error) => {
                status.failed += 1;
                status.error = Some(error.clone());
                if error.code.is_fatal() {
                    status.state = ClassifierState::Error;
                    model_usable = false;
                    detector.disable_model();
                } else {
                    status.state = ClassifierState::Degraded;
                }
            }
        }
        let events = detector.resolve(*pending, result, latency, millis_since(epoch), wall_ms());
        events.into_iter().for_each(|e| emit(e));
        emit(PipelineEvent::ClassifierStatus(status.clone()));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use futures_util::future::BoxFuture;

    use super::*;
    use crate::intent::classifier::{Classification, ClassifyRequest, Relation, Urgency};
    use crate::intent::types::{Decision, Intent, Language};
    use crate::stt::types::TranscriptSegment;

    /// Answers after a fixed delay with a scripted result per utterance text.
    struct ScriptedModel {
        delay: Duration,
        answers: Mutex<Vec<Result<Classification, ClassifyError>>>,
        seen: Mutex<Vec<ClassifyRequest>>,
    }

    impl Classifier for ScriptedModel {
        fn model(&self) -> &'static str {
            "claude-haiku-5-5"
        }

        fn classify<'a>(
            &'a self,
            request: &'a ClassifyRequest,
        ) -> BoxFuture<'a, Result<Classification, ClassifyError>> {
            Box::pin(async move {
                tokio::time::sleep(self.delay).await;
                self.seen.lock().unwrap().push(request.clone());
                self.answers.lock().unwrap().remove(0)
            })
        }
    }

    fn question(intent: Intent, text: &str) -> Result<Classification, ClassifyError> {
        Ok(Classification {
            question: text.into(),
            language: Language::Fr,
            intent,
            requires_response: true,
            relation: Relation::New,
            context_ref: None,
            domain: None,
            urgency: Urgency::High,
            confidence: 0.9,
            timing: None,
        })
    }

    fn recorder() -> (Emit, Arc<Mutex<Vec<PipelineEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        (Arc::new(move |e| sink.lock().unwrap().push(e)), events)
    }

    fn segment(id: &str, text: &str, is_final: bool) -> TranscriptSegment {
        TranscriptSegment {
            id: id.into(),
            session_id: "session-1".into(),
            source_id: "system".into(),
            start_ms: 0,
            end_ms: 0,
            text: text.into(),
            normalized_text: None,
            is_final,
            confidence: None,
            speaker_id: None,
        }
    }

    fn final_(id: &str, text: &str) -> PipelineEvent {
        PipelineEvent::TranscriptFinal {
            segment: segment(id, text, true),
        }
    }

    fn options() -> EngineOptions {
        EngineOptions {
            session_id: "session-1".into(),
            detector: DetectorConfig::DEFAULT,
            assembler: AssemblerConfig::DEFAULT,
            on_escalate: None,
        }
    }

    fn questions(events: &[PipelineEvent]) -> Vec<(String, Vec<String>, Intent)> {
        events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::QuestionDetected { question } => Some((
                    question.normalized_question.clone(),
                    question.source_segment_ids.clone(),
                    question.intent,
                )),
                _ => None,
            })
            .collect()
    }

    fn statuses(events: &[PipelineEvent]) -> Vec<ClassifierStatus> {
        events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::ClassifierStatus(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn transcripts_become_questions_in_order_with_cut_turns_merged() {
        let model = Arc::new(ScriptedModel {
            delay: Duration::from_millis(400),
            answers: Mutex::new(vec![
                question(
                    Intent::TechnicalQuestion,
                    "Quelle base de données choisirais-tu ?",
                ),
                question(
                    Intent::TechnicalQuestion,
                    "Comment indexerais-tu cette table ?",
                ),
            ]),
            seen: Mutex::new(Vec::new()),
        });
        let (emit, events) = recorder();
        let engine = spawn(
            &Handle::current(),
            Some(model.clone()),
            None,
            options(),
            emit,
        );

        engine.observe(&final_("system-1-0", "Bonjour !"));
        engine.observe(&final_("system-1-1", "Et du coup, sur la partie"));
        tokio::time::sleep(Duration::from_millis(500)).await;
        engine.observe(&PipelineEvent::TranscriptPartial {
            segment: segment("system-1-2", "base de", false),
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
        engine.observe(&final_(
            "system-1-2",
            "base de données, tu choisirais quoi ?",
        ));
        engine.observe(&final_("system-1-3", "D'accord."));
        engine.observe(&final_("system-1-4", "Et comment tu indexerais la table ?"));
        drop(engine);
        tokio::time::sleep(Duration::from_secs(5)).await;

        let events = events.lock().unwrap();
        assert_eq!(
            questions(&events),
            vec![
                (
                    "Bonjour !".to_string(),
                    vec!["system-1-0".to_string()],
                    Intent::GreetingOrSmallTalk
                ),
                (
                    "Quelle base de données choisirais-tu ?".to_string(),
                    vec!["system-1-1".to_string(), "system-1-2".to_string()],
                    Intent::TechnicalQuestion
                ),
                (
                    "Comment indexerais-tu cette table ?".to_string(),
                    vec!["system-1-4".to_string()],
                    Intent::TechnicalQuestion
                ),
            ]
        );
        // The second model call saw the first question and the acknowledgement in context.
        let seen = model.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1].recent_questions.len(), 2);
        assert_eq!(seen[1].transcript.last().unwrap().text, "D'accord.");

        let last = statuses(&events).pop().unwrap();
        assert_eq!(last.state, ClassifierState::Ready);
        assert_eq!(last.classified, 2);
        assert_eq!(last.last_latency_ms, Some(400));
    }

    #[tokio::test(start_paused = true)]
    async fn utterances_sent_to_the_model_are_announced_before_the_decision() {
        let model = Arc::new(ScriptedModel {
            delay: Duration::from_millis(400),
            answers: Mutex::new(vec![question(
                Intent::TechnicalQuestion,
                "Comment indexerais-tu la table ?",
            )]),
            seen: Mutex::new(Vec::new()),
        });
        let (emit, events) = recorder();
        let escalated = Arc::new(Mutex::new(Vec::new()));
        let log = escalated.clone();
        let mut opts = options();
        opts.on_escalate = Some(Arc::new(move |u: &Escalated| {
            log.lock().unwrap().push(u.clone())
        }));
        let engine = spawn(&Handle::current(), Some(model), None, opts, emit);
        engine.observe(&final_("system-1-0", "Bonjour !"));
        engine.observe(&final_("system-1-1", "Et comment tu indexerais la table ?"));
        drop(engine);
        tokio::time::sleep(Duration::from_secs(5)).await;
        // The greeting is decided by the rules; only the question went to the model.
        assert_eq!(
            *escalated.lock().unwrap(),
            [Escalated {
                segment_ids: vec!["system-1-1".into()],
                text: "Et comment tu indexerais la table ?".into(),
            }]
        );
        assert_eq!(questions(&events.lock().unwrap()).len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn without_a_key_the_rules_detect_explicit_questions_and_say_so() {
        let (emit, events) = recorder();
        let missing = ClassifyError::new(ClassifyErrorCode::MissingKey, "no key");
        let engine = spawn(&Handle::current(), None, Some(missing), options(), emit);
        engine.observe(&final_("system-1-0", "Tu connais Docker ?"));
        engine.observe(&final_("system-1-1", "On l'utilise beaucoup ici."));
        drop(engine);
        tokio::time::sleep(Duration::from_secs(1)).await;

        let events = events.lock().unwrap();
        let first = &statuses(&events)[0];
        assert_eq!(first.state, ClassifierState::RulesOnly);
        assert_eq!(
            first.error.as_ref().unwrap().code,
            ClassifyErrorCode::MissingKey
        );
        assert_eq!(questions(&events).len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejected_key_switches_to_rules_for_the_rest_of_the_session() {
        let denied = ClassifyError::new(ClassifyErrorCode::Unauthorized, "HTTP 401");
        let model = Arc::new(ScriptedModel {
            delay: Duration::from_millis(100),
            answers: Mutex::new(vec![Err(denied)]),
            seen: Mutex::new(Vec::new()),
        });
        let (emit, events) = recorder();
        let engine = spawn(
            &Handle::current(),
            Some(model.clone()),
            None,
            options(),
            emit,
        );
        engine.observe(&final_("system-1-0", "Explique-moi les closures."));
        engine.observe(&final_("system-1-1", "Tu connais Docker ?"));
        drop(engine);
        tokio::time::sleep(Duration::from_secs(2)).await;

        // One model call only; both explicit questions still became cards.
        assert_eq!(model.seen.lock().unwrap().len(), 1);
        let events = events.lock().unwrap();
        assert_eq!(questions(&events).len(), 2);
        let last = statuses(&events).pop().unwrap();
        assert_eq!(last.state, ClassifierState::Error);
        assert_eq!(last.failed, 1);
        let reports: Vec<Decision> = events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::UtteranceClassified { utterance } => Some(utterance.decision),
                _ => None,
            })
            .collect();
        assert_eq!(reports, [Decision::Question, Decision::Question]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_cut_turn_at_the_end_of_the_session_is_still_classified() {
        let model = Arc::new(ScriptedModel {
            delay: Duration::from_millis(10),
            answers: Mutex::new(vec![question(Intent::TechnicalQuestion, "Parle-moi de la")]),
            seen: Mutex::new(Vec::new()),
        });
        let (emit, events) = recorder();
        let engine = spawn(&Handle::current(), Some(model), None, options(), emit);
        engine.observe(&final_("system-1-0", "Est-ce que tu peux me parler de la"));
        drop(engine);
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(questions(&events.lock().unwrap()).len(), 1);
    }
}
