//! Wiring of one live session: STT events → question detection → overlay and answer engine.
//! `audio_start` uses it, and so does the end-to-end test below, which drives the same chain
//! from transcripts to a filled card without any paid API.

use std::sync::Arc;

use tokio::runtime::Handle;

use crate::answer::engine::AnswerEngine;
use crate::audio::engine::Emit;
use crate::events::PipelineEvent;
use crate::intent::{
    assembler::AssemblerConfig,
    classifier::Classifier,
    detector::DetectorConfig,
    engine::{self, EngineOptions},
    types::ClassifyError,
};

/// How a session detects questions.
pub struct Detection {
    pub include_microphone: bool,
    /// Cards and answers for technical questions only.
    pub technical_only: bool,
    /// `None` runs the rules alone; `unavailable` then says why.
    pub classifier: Option<Arc<dyn Classifier>>,
    pub unavailable: Option<ClassifyError>,
}

/// Returns the `Emit` the session's STT streams use. Every event goes to the overlay first,
/// then to the answer engine, so a card always exists before its answer starts. With
/// detection on, transcripts also feed the question engine, and every utterance it sends to
/// the model starts its answer early.
pub fn session_emit(
    runtime: &Handle,
    session_id: String,
    detection: Option<Detection>,
    overlay: Emit,
    answers: Arc<AnswerEngine>,
) -> Emit {
    let answered: Emit = {
        let answers = answers.clone();
        Arc::new(move |event: PipelineEvent| {
            overlay(event.clone());
            answers.observe(&event);
        })
    };
    let Some(detection) = detection else {
        return answered;
    };
    let questions = engine::spawn(
        runtime,
        detection.classifier,
        detection.unavailable,
        EngineOptions {
            session_id,
            detector: DetectorConfig {
                include_microphone: detection.include_microphone,
                technical_only: detection.technical_only,
                ..DetectorConfig::DEFAULT
            },
            assembler: AssemblerConfig::DEFAULT,
            on_escalate: Some(Arc::new(move |utterance| answers.speculate(utterance))),
        },
        answered.clone(),
    );
    Arc::new(move |event| {
        questions.observe(&event);
        answered(event);
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use futures_util::future::BoxFuture;

    use super::*;
    use crate::intent::classifier::{Classification, ClassifyRequest, Relation, Urgency};
    use crate::intent::types::{Intent, Language};
    use crate::llm::test_support::{fake_api, streamed_text};
    use crate::profile::ProfileStore;
    use crate::stt::types::TranscriptSegment;

    /// Decides after 300 ms: statements are statements, the rest are new technical questions.
    struct Classifier300;

    impl Classifier for Classifier300 {
        fn model(&self) -> &'static str {
            "claude-haiku-5-5"
        }

        fn classify<'a>(
            &'a self,
            request: &'a ClassifyRequest,
        ) -> BoxFuture<'a, Result<Classification, ClassifyError>> {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                let asks = request.utterance.contains('?');
                Ok(Classification {
                    question: request.utterance.clone(),
                    language: Language::Fr,
                    intent: if asks {
                        Intent::TechnicalQuestion
                    } else {
                        Intent::NonQuestionStatement
                    },
                    requires_response: asks,
                    relation: Relation::New,
                    context_ref: None,
                    domain: None,
                    urgency: Urgency::High,
                    confidence: 0.9,
                    timing: None,
                })
            })
        }
    }

    fn final_(id: &str, text: &str) -> PipelineEvent {
        PipelineEvent::TranscriptFinal {
            segment: TranscriptSegment {
                id: id.into(),
                session_id: "s1".into(),
                source_id: "system".into(),
                start_ms: 0,
                end_ms: 0,
                text: text.into(),
                normalized_text: None,
                is_final: true,
                confidence: None,
                speaker_id: None,
            },
        }
    }

    fn kind(event: &PipelineEvent) -> String {
        match event {
            PipelineEvent::TranscriptFinal { segment } => format!("final {}", segment.id),
            PipelineEvent::QuestionDetected { question } => format!("question {}", question.id),
            PipelineEvent::AnswerStarted { request_id, .. } => format!("started {request_id}"),
            PipelineEvent::AnswerDelta { .. } => "delta".into(),
            PipelineEvent::AnswerCompleted { request_id, .. } => {
                format!("completed {request_id}")
            }
            PipelineEvent::AnswerFailed { message, .. } => format!("failed {message}"),
            PipelineEvent::UtteranceClassified { utterance } => {
                format!("classified {:?}", utterance.decision)
            }
            PipelineEvent::ClassifierStatus(_) => "classifier".into(),
            other => format!("{other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn transcripts_become_cards_with_their_answer_and_statements_stay_quiet() {
        // Two early answers are requested: the statement's (never shown) and the question's.
        let (url, seen) = fake_api(vec![
            (200, streamed_text("Ah, très bien.", 4)),
            (
                200,
                streamed_text("La boucle d'événements traite la file des tâches.", 6),
            ),
        ])
        .await;
        let overlay_events = Arc::new(Mutex::new(Vec::new()));
        let log = overlay_events.clone();
        let overlay: Emit = Arc::new(move |e| log.lock().unwrap().push(e));
        let answers = Arc::new(AnswerEngine::new(
            overlay.clone(),
            Handle::current(),
            Arc::new(ProfileStore::open(None)),
            Box::new(|| Ok(Some("k".into()))),
            Some(url),
        ));
        let emit = session_emit(
            &Handle::current(),
            "s1".into(),
            Some(Detection {
                include_microphone: false,
                technical_only: true,
                classifier: Some(Arc::new(Classifier300)),
                unavailable: None,
            }),
            overlay,
            answers,
        );

        emit(final_("system-1-0", "On utilise beaucoup Vue chez nous."));
        emit(final_(
            "system-1-1",
            "Comment fonctionne l'event loop en JavaScript ?",
        ));
        for _ in 0..500 {
            let done = overlay_events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, PipelineEvent::AnswerCompleted { .. }));
            if done {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;

        let events = overlay_events.lock().unwrap();
        let kinds: Vec<String> = events
            .iter()
            .map(kind)
            .filter(|k| k != "classifier")
            .collect();
        let card = kinds
            .iter()
            .position(|k| k == "question s1-q1")
            .unwrap_or_else(|| panic!("{kinds:?}"));
        assert_eq!(kinds[..2], ["final system-1-0", "final system-1-1"]);
        assert!(kinds[..card].contains(&"classified Ignored".to_string()));
        assert_eq!(kinds[card + 1], "started s1-q1-a1", "{kinds:?}");
        // The early answer may be complete when the card appears: it is then replayed at once.
        assert!(kinds[card..].contains(&"completed s1-q1-a1".to_string()));
        // The statement got no card and its early answer never reached the overlay.
        assert_eq!(
            kinds.iter().filter(|k| k.starts_with("question")).count(),
            1
        );
        assert_eq!(kinds.iter().filter(|k| k.starts_with("started")).count(), 1);
        let text: String = events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::AnswerDelta { delta, .. } => Some(delta.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "La boucle d'événements traite la file des tâches.");
        assert_eq!(seen.lock().unwrap().len(), 2);
    }
}
