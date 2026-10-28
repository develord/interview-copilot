//! AssemblyAI Universal Streaming v3 (`wss://streaming.eu.assemblyai.com/v3/ws`).
//!
//! Protocol (docs checked 2026-10-28): auth via `Authorization: <key>` header; binary PCM
//! frames; server messages `Begin`, `Turn` (`turn_order`, `end_of_turn`, `transcript`,
//! `utterance`, `words[]`), `Termination`; client messages `ForceEndpoint`,
//! `UpdateConfiguration`, `Terminate`. Keyterms and context are sent with
//! `UpdateConfiguration` right after `Begin` (JSON arrays, avoiding query-string encoding).

use futures_util::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

use super::provider::{AudioMsg, EventSender, ProviderEvent, ProviderSettings, SttProvider};
use super::types::{ProviderKind, SttError, SttErrorCode};
use super::ws::{self, Codec, CodecOutput};

pub const EU_ENDPOINT: &str = "wss://streaming.eu.assemblyai.com/v3/ws";
pub const MODEL: &str = "universal-3-6-pro";
const MAX_TERMS: usize = 100;
const MAX_CONTEXT_CHARS: usize = 1_750;

pub struct AssemblyAi {
    pub settings: ProviderSettings,
}

impl AssemblyAi {
    pub fn url(&self) -> String {
        format!(
            "{}?speech_model={MODEL}&sample_rate=16000&encoding=pcm_s16le&include_partial_turns=true",
            self.settings.endpoint
        )
    }
}

impl SttProvider for AssemblyAi {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Assemblyai
    }

    fn stream<'a>(
        &'a self,
        audio: &'a mut mpsc::Receiver<AudioMsg>,
        events: &'a EventSender,
    ) -> BoxFuture<'a, Result<(), SttError>> {
        Box::pin(async move {
            let mut codec = AssemblyAiCodec::new(&self.settings);
            let headers = [("authorization", self.settings.api_key.clone())];
            ws::run(&self.url(), &headers, &mut codec, audio, events).await
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    Begin {
        #[serde(default)]
        configuration: Option<serde_json::Value>,
    },
    Turn(Turn),
    Termination {},
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Turn {
    turn_order: u64,
    end_of_turn: bool,
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    utterance: String,
    #[serde(default)]
    language_code: Option<String>,
    #[serde(default)]
    words: Vec<Word>,
}

#[derive(Deserialize)]
struct Word {
    text: String,
    start: u64,
    end: u64,
    #[serde(default)]
    confidence: Option<f32>,
}

pub struct AssemblyAiCodec {
    update: serde_json::Value,
}

impl AssemblyAiCodec {
    pub fn new(settings: &ProviderSettings) -> Self {
        let terms: Vec<&String> = settings.terms.iter().take(MAX_TERMS).collect();
        let context: String = settings.context.chars().take(MAX_CONTEXT_CHARS).collect();
        let mut update = json!({ "type": "UpdateConfiguration" });
        if !terms.is_empty() {
            update["keyterms_prompt"] = json!(terms);
        }
        if !context.is_empty() {
            update["agent_context"] = json!(context);
        }
        if !settings.language_hints.is_empty() {
            update["language_codes"] = json!(settings.language_hints);
        }
        Self { update }
    }
}

impl Codec for AssemblyAiCodec {
    fn on_open(&mut self) -> Vec<Message> {
        Vec::new()
    }

    fn on_text(&mut self, text: &str) -> Result<Vec<CodecOutput>, SttError> {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
            && let Some(error) = value.get("error").and_then(|e| e.as_str())
        {
            return Err(classify_error_text(error));
        }
        let message: ServerMessage = serde_json::from_str(text).map_err(|e| {
            SttError::new(SttErrorCode::Protocol, format!("unexpected message: {e}"))
        })?;
        Ok(match message {
            ServerMessage::Begin { configuration } => {
                let mut out = vec![CodecOutput::Event(ProviderEvent::Ready)];
                if let Some(model) = configuration
                    .as_ref()
                    .and_then(|c| c.get("model"))
                    .and_then(|m| m.as_str())
                    && model != MODEL
                {
                    eprintln!("[stt] AssemblyAI is serving {model}, expected {MODEL}");
                }
                if self.update.as_object().is_some_and(|o| o.len() > 1) {
                    out.push(CodecOutput::Send(Message::Text(
                        self.update.to_string().into(),
                    )));
                }
                out
            }
            ServerMessage::Turn(turn) => vec![CodecOutput::Event(turn_event(turn))],
            ServerMessage::Termination {} => vec![CodecOutput::Finished],
            ServerMessage::Other => Vec::new(),
        })
    }

    fn force_endpoint(&self) -> Message {
        Message::Text(json!({ "type": "ForceEndpoint" }).to_string().into())
    }

    fn finish(&self) -> Message {
        Message::Text(json!({ "type": "Terminate" }).to_string().into())
    }

    /// Close codes are not documented for v3; classify by the reason text and the v2-era
    /// codes (4001 not authorized, 4002 insufficient funds). Anything else is retried.
    fn classify_close(&self, frame: Option<&CloseFrame>) -> SttError {
        let (code, reason) = frame.map_or((0, String::new()), |f| {
            (u16::from(f.code), f.reason.to_string())
        });
        let lower = reason.to_lowercase();
        let kind = if code == 4001 || code == 1008 || lower.contains("auth") {
            SttErrorCode::Unauthorized
        } else if code == 4002 || lower.contains("insufficient") || lower.contains("balance") {
            SttErrorCode::Quota
        } else {
            SttErrorCode::Network
        };
        SttError::new(kind, format!("closed by AssemblyAI ({code}): {reason}"))
    }
}

/// AssemblyAI reports refusals as `{"error": "..."}` text; the wording is the only signal.
/// A concurrency limit is transient (retry once another session closes); a bad key or an empty
/// balance is not.
fn classify_error_text(error: &str) -> SttError {
    let lower = error.to_lowercase();
    let code = if lower.contains("concurrent") || lower.contains("too many") {
        SttErrorCode::Network
    } else if lower.contains("insufficient") || lower.contains("balance") || lower.contains("funds")
    {
        SttErrorCode::Quota
    } else if lower.contains("unauthorized") || lower.contains("api key") || lower.contains("auth")
    {
        SttErrorCode::Unauthorized
    } else {
        SttErrorCode::Provider
    };
    SttError::new(code, error.to_string())
}

fn turn_event(turn: Turn) -> ProviderEvent {
    let start_ms = turn.words.first().map_or(0, |w| w.start);
    let end_ms = turn.words.last().map_or(start_ms, |w| w.end);
    if turn.end_of_turn {
        let text = if turn.utterance.trim().is_empty() {
            turn.transcript
        } else {
            turn.utterance
        };
        let confidences: Vec<f32> = turn.words.iter().filter_map(|w| w.confidence).collect();
        ProviderEvent::Final {
            turn: turn.turn_order,
            text: text.trim().to_string(),
            start_ms,
            end_ms,
            confidence: (!confidences.is_empty())
                .then(|| confidences.iter().sum::<f32>() / confidences.len() as f32),
            language: turn.language_code,
        }
    } else {
        // Show every word heard so far (final and provisional), not only the finalized ones.
        let text = if turn.words.is_empty() {
            turn.transcript
        } else {
            turn.words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        ProviderEvent::Partial {
            turn: turn.turn_order,
            text: text.trim().to_string(),
            start_ms,
            end_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    fn settings() -> ProviderSettings {
        ProviderSettings {
            endpoint: EU_ENDPOINT.into(),
            api_key: "key".into(),
            terms: vec!["Vue.js".into(), "state management".into()],
            context: "Entretien technique".into(),
            language_hints: vec!["fr".into(), "en".into()],
        }
    }

    fn events(outputs: Vec<CodecOutput>) -> Vec<ProviderEvent> {
        outputs
            .into_iter()
            .filter_map(|o| match o {
                CodecOutput::Event(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn builds_the_eu_url_with_the_pinned_model_and_format() {
        let url = AssemblyAi {
            settings: settings(),
        }
        .url();
        assert!(url.starts_with("wss://streaming.eu.assemblyai.com/v3/ws?"));
        assert!(url.contains("speech_model=universal-3-6-pro"));
        assert!(url.contains("sample_rate=16000") && url.contains("encoding=pcm_s16le"));
    }

    #[test]
    fn begin_sends_keyterms_context_and_languages_as_json() {
        let mut codec = AssemblyAiCodec::new(&settings());
        let out = codec
            .on_text(r#"{"type":"Begin","id":"x","expires_at":1,"configuration":{"model":"universal-3-6-pro"}}"#)
            .unwrap();
        assert!(matches!(out[0], CodecOutput::Event(ProviderEvent::Ready)));
        let CodecOutput::Send(Message::Text(sent)) = &out[1] else {
            panic!("no update sent")
        };
        let update: serde_json::Value = serde_json::from_str(sent.as_str()).unwrap();
        assert_eq!(update["type"], "UpdateConfiguration");
        assert_eq!(
            update["keyterms_prompt"],
            json!(["Vue.js", "state management"])
        );
        assert_eq!(update["agent_context"], "Entretien technique");
        assert_eq!(update["language_codes"], json!(["fr", "en"]));
    }

    #[test]
    fn caps_terms_at_100() {
        let mut many = settings();
        many.terms = (0..150).map(|i| format!("term{i}")).collect();
        let codec = AssemblyAiCodec::new(&many);
        assert_eq!(
            codec.update["keyterms_prompt"].as_array().unwrap().len(),
            100
        );
    }

    #[test]
    fn partial_turns_show_all_words_and_final_turns_use_the_formatted_utterance() {
        let mut codec = AssemblyAiCodec::new(&settings());
        let partial = r#"{"type":"Turn","turn_order":0,"turn_is_formatted":false,"end_of_turn":false,
            "transcript":"comment tu","utterance":"","end_of_turn_confidence":0.1,
            "words":[{"text":"comment","start":100,"end":300,"confidence":0.9,"word_is_final":true},
                     {"text":"tu","start":320,"end":400,"confidence":0.8,"word_is_final":true},
                     {"text":"gères","start":420,"end":600,"confidence":0.5,"word_is_final":false}]}"#;
        assert_eq!(
            events(codec.on_text(partial).unwrap()),
            vec![ProviderEvent::Partial {
                turn: 0,
                text: "comment tu gères".into(),
                start_ms: 100,
                end_ms: 600
            }]
        );

        let last = r#"{"type":"Turn","turn_order":0,"turn_is_formatted":true,"end_of_turn":true,
            "transcript":"comment tu gères le state management","language_code":"fr","language_confidence":0.9,
            "utterance":"Comment tu gères le state management ?","end_of_turn_confidence":0.95,
            "words":[{"text":"Comment","start":100,"end":300,"confidence":1.0,"word_is_final":true},
                     {"text":"management ?","start":1400,"end":1900,"confidence":0.5,"word_is_final":true}]}"#;
        assert_eq!(
            events(codec.on_text(last).unwrap()),
            vec![ProviderEvent::Final {
                turn: 0,
                text: "Comment tu gères le state management ?".into(),
                start_ms: 100,
                end_ms: 1900,
                confidence: Some(0.75),
                language: Some("fr".into()),
            }]
        );
    }

    #[test]
    fn termination_finishes_and_unknown_messages_are_ignored() {
        let mut codec = AssemblyAiCodec::new(&settings());
        assert!(matches!(
            codec
                .on_text(r#"{"type":"Termination","audio_duration_seconds":3}"#)
                .unwrap()[..],
            [CodecOutput::Finished]
        ));
        assert!(
            codec
                .on_text(r#"{"type":"SpeechStarted","timestamp":10}"#)
                .unwrap()
                .is_empty()
        );
        assert!(codec.on_text(r#"{"type":"Heartbeat"}"#).unwrap().is_empty());
    }

    #[test]
    fn errors_and_garbage_are_reported_not_ignored() {
        let mut codec = AssemblyAiCodec::new(&settings());
        assert_eq!(
            codec
                .on_text(r#"{"error":"Invalid sample rate"}"#)
                .unwrap_err()
                .code,
            SttErrorCode::Provider
        );
        assert_eq!(
            codec.on_text("not json").unwrap_err().code,
            SttErrorCode::Protocol
        );
    }

    // Regression: "Unauthorized Connection: Too many concurrent sessions" (seen live,
    // 2026-10-28) is a transient concurrency limit, not a bad key.
    #[test]
    fn error_messages_separate_concurrency_limits_from_bad_keys() {
        let mut codec = AssemblyAiCodec::new(&settings());
        let mut error = |text: &str| {
            codec
                .on_text(&json!({ "error": text }).to_string())
                .unwrap_err()
        };
        let concurrent = error("Unauthorized Connection: Too many concurrent sessions");
        assert_eq!(concurrent.code, SttErrorCode::Network);
        assert!(concurrent.is_recoverable());
        assert_eq!(
            error("Unauthorized Connection: Invalid API key").code,
            SttErrorCode::Unauthorized
        );
        assert_eq!(
            error("Insufficient account balance").code,
            SttErrorCode::Quota
        );
        assert!(error("Something else").is_recoverable());
    }

    #[test]
    fn close_frames_separate_auth_and_quota_from_transient_failures() {
        let codec = AssemblyAiCodec::new(&settings());
        let frame = |code: u16, reason: &str| CloseFrame {
            code: CloseCode::from(code),
            reason: reason.to_string().into(),
        };
        assert_eq!(
            codec
                .classify_close(Some(&frame(4001, "Not authorized")))
                .code,
            SttErrorCode::Unauthorized
        );
        assert_eq!(
            codec
                .classify_close(Some(&frame(4002, "Insufficient funds")))
                .code,
            SttErrorCode::Quota
        );
        assert!(
            codec
                .classify_close(Some(&frame(1011, "internal")))
                .is_recoverable()
        );
        assert!(codec.classify_close(None).is_recoverable());
    }

    #[test]
    fn control_messages_match_the_documented_types() {
        let codec = AssemblyAiCodec::new(&settings());
        assert_eq!(
            codec.force_endpoint(),
            Message::Text(r#"{"type":"ForceEndpoint"}"#.into())
        );
        assert_eq!(
            codec.finish(),
            Message::Text(r#"{"type":"Terminate"}"#.into())
        );
    }
}
