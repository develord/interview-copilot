//! Pipeline events sent to the webview. They mirror `src/domain/events.ts`, where every payload
//! is validated again before it reaches UI state. Shared JSON fixtures in `contracts/fixtures`
//! are checked from both sides.

use serde::Serialize;

use crate::audio::types::SourceDiagnostics;
use crate::intent::types::{ClassifierStatus, DetectedQuestion, UtteranceReport};
use crate::stt::types::{SttStatus, TranscriptSegment};

/// Tauri event name carrying every `PipelineEvent`.
pub const PIPELINE_EVENT: &str = "pipeline:event";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionStatus {
    Idle,
    Listening,
    Paused,
    Reconnecting,
    Error,
}

/// The two independent parts of an answer card (`ANSWER_SECTIONS` in `src/domain/types.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AnswerSection {
    /// Readable in a few seconds.
    Primary,
    /// Code, complexity, trade-offs — the expandable part.
    Details,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum PipelineEvent {
    #[serde(rename = "session.status")]
    SessionStatus {
        status: SessionStatus,
        message: Option<String>,
    },
    #[serde(rename = "audio.diagnostics")]
    AudioDiagnostics { sources: Vec<SourceDiagnostics> },
    #[serde(rename = "transcript.partial")]
    TranscriptPartial { segment: TranscriptSegment },
    #[serde(rename = "transcript.final")]
    TranscriptFinal { segment: TranscriptSegment },
    #[serde(rename = "stt.status")]
    SttStatus(SttStatus),
    #[serde(rename = "question.detected")]
    QuestionDetected { question: DetectedQuestion },
    #[serde(rename = "question.revised")]
    QuestionRevised {
        #[serde(rename = "questionId")]
        question_id: String,
        text: String,
    },
    #[serde(rename = "utterance.classified")]
    UtteranceClassified { utterance: UtteranceReport },
    #[serde(rename = "classifier.status")]
    ClassifierStatus(ClassifierStatus),
    #[serde(rename = "answer.started", rename_all = "camelCase")]
    AnswerStarted {
        question_id: String,
        request_id: String,
    },
    /// `seq` starts at 0 for every request; the overlay drops duplicates and reorders.
    #[serde(rename = "answer.delta", rename_all = "camelCase")]
    AnswerDelta {
        question_id: String,
        request_id: String,
        seq: u64,
        section: AnswerSection,
        delta: String,
    },
    #[serde(rename = "answer.completed", rename_all = "camelCase")]
    AnswerCompleted {
        question_id: String,
        request_id: String,
    },
    #[serde(rename = "answer.failed", rename_all = "camelCase")]
    AnswerFailed {
        question_id: String,
        request_id: String,
        message: String,
        retryable: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::types::{AudioError, AudioErrorCode, SourceKind, SourceState};

    fn fixture(name: &str) -> serde_json::Value {
        let path = format!(
            "{}/../contracts/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn audio_diagnostics_match_the_shared_contract_fixture() {
        let mut system = SourceDiagnostics::new(SourceKind::System, true);
        system.state = SourceState::Capturing;
        system.device_name = Some("Speakers (Realtek(R) Audio)".into());
        system.sample_rate = Some(48_000);
        system.channels = Some(2);
        system.level_dbfs = -23.5;
        system.peak_dbfs = -12.25;
        system.receiving = true;
        system.captured_seconds = 12.5;

        let mut microphone = SourceDiagnostics::new(SourceKind::Microphone, false);
        microphone.state = SourceState::Error;
        microphone.error = Some(AudioError::new(
            AudioErrorCode::PermissionDenied,
            "Access is denied. (os error -2147024891)",
        ));

        let event = PipelineEvent::AudioDiagnostics {
            sources: vec![system, microphone],
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            fixture("audio-diagnostics.json")
        );
    }

    #[test]
    fn transcript_final_matches_the_shared_contract_fixture() {
        let event = PipelineEvent::TranscriptFinal {
            segment: TranscriptSegment {
                id: "system-1-0".into(),
                session_id: "session-1761649200000".into(),
                source_id: "system".into(),
                start_ms: 1200,
                end_ms: 3450,
                text: "Comment tu gères le state management dans Vue ?".into(),
                normalized_text: None,
                is_final: true,
                confidence: Some(0.5),
                speaker_id: None,
            },
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            fixture("transcript-final.json")
        );
    }

    #[test]
    fn stt_status_matches_the_shared_contract_fixture() {
        use crate::stt::types::{ProviderKind, SttError, SttErrorCode, SttState};
        let event = PipelineEvent::SttStatus(SttStatus {
            source: SourceKind::Microphone,
            provider: ProviderKind::Assemblyai,
            state: SttState::Reconnecting,
            error: Some(SttError::new(
                SttErrorCode::Network,
                "connection closed by the server",
            )),
            last_final_latency_ms: Some(420),
            dropped_chunks: 3,
        });
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            fixture("stt-status.json")
        );
    }

    #[test]
    fn question_events_match_the_shared_contract_fixtures() {
        use crate::intent::types::{DetectedQuestion, Intent, Language};
        let detected = PipelineEvent::QuestionDetected {
            question: DetectedQuestion {
                id: "session-1761649200000-q2".into(),
                session_id: "session-1761649200000".into(),
                source: "speech",
                source_segment_ids: vec!["system-1-3".into(), "system-1-4".into()],
                original_utterance:
                    "Et du coup, sur la partie base de données, tu choisirais quoi ?".into(),
                normalized_question: "Quelle base de données choisirais-tu ?".into(),
                intent: Intent::TechnicalQuestion,
                language: Language::Fr,
                domain: Some("databases".into()),
                context_ref: Some("session-1761649200000-q1".into()),
                detected_at: 1_761_649_212_345,
            },
        };
        assert_eq!(
            serde_json::to_value(&detected).unwrap(),
            fixture("question-detected.json")
        );

        let revised = PipelineEvent::QuestionRevised {
            question_id: "session-1761649200000-q2".into(),
            text: "Comment gères-tu le state management avec Vue 3 ?".into(),
        };
        assert_eq!(
            serde_json::to_value(&revised).unwrap(),
            fixture("question-revised.json")
        );
    }

    #[test]
    fn classification_diagnostics_match_the_shared_contract_fixtures() {
        use crate::intent::types::{
            ClassifierState, ClassifierStatus, ClassifyError, ClassifyErrorCode, Decision,
            IgnoreReason, Intent, Stage, UtteranceReport,
        };
        let classified = PipelineEvent::UtteranceClassified {
            utterance: UtteranceReport {
                id: "session-1761649200000-u5".into(),
                source_id: "system".into(),
                segment_ids: vec!["system-1-6".into()],
                text: "On utilise Kafka chez nous.".into(),
                decision: Decision::Ignored,
                reason: Some(IgnoreReason::Statement),
                intent: Some(Intent::NonQuestionStatement),
                stage: Stage::Llm,
                question_id: None,
                latency_ms: Some(640),
            },
        };
        assert_eq!(
            serde_json::to_value(&classified).unwrap(),
            fixture("utterance-classified.json")
        );

        let status = PipelineEvent::ClassifierStatus(ClassifierStatus {
            state: ClassifierState::Degraded,
            model: Some("claude-opus-5-5"),
            error: Some(ClassifyError::new(
                ClassifyErrorCode::RateLimited,
                "HTTP 429: slow down",
            )),
            last_latency_ms: Some(812),
            classified: 14,
            failed: 1,
        });
        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            fixture("classifier-status.json")
        );
    }

    #[test]
    fn answer_events_match_the_shared_contract_fixtures() {
        let delta = PipelineEvent::AnswerDelta {
            question_id: "session-1761649200000-q2".into(),
            request_id: "session-1761649200000-q2-a1".into(),
            seq: 3,
            section: AnswerSection::Details,
            delta: "```ts
const store = useStore()
```"
            .into(),
        };
        assert_eq!(
            serde_json::to_value(&delta).unwrap(),
            fixture("answer-delta.json")
        );
        let failed = PipelineEvent::AnswerFailed {
            question_id: "session-1761649200000-q2".into(),
            request_id: "session-1761649200000-q2-a1".into(),
            message: "Connexion à Claude perdue.".into(),
            retryable: true,
        };
        assert_eq!(
            serde_json::to_value(&failed).unwrap(),
            fixture("answer-failed.json")
        );
    }

    #[test]
    fn session_status_matches_the_shared_contract_fixture() {
        let event = PipelineEvent::SessionStatus {
            status: SessionStatus::Reconnecting,
            message: None,
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            fixture("session-status.json")
        );
    }
}
