//! Soniox real-time (`wss://stt-rt.eu.soniox.com/transcribe-websocket`, model `stt-rt-v5`).
//!
//! Protocol (docs checked 2026-10-28): auth via `Authorization: Bearer <key>` (the `api_key`
//! config field is deprecated and refused from 2027-01-15); a JSON start request, then binary
//! PCM; responses carry `tokens[]` (`text`, `start_ms`, `end_ms`, `confidence`, `is_final`,
//! `language`). Final tokens are sent once; non-final tokens are replaced on every response.
//! `<end>` (endpoint) and `<fin>` (after `{"type":"finalize"}`) close a segment. An empty
//! *text* frame ends the stream; the server answers `finished: true`.

use futures_util::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

use super::provider::{AudioMsg, EventSender, ProviderEvent, ProviderSettings, SttProvider};
use super::types::{ProviderKind, SttError, SttErrorCode};
use super::ws::{self, Codec, CodecOutput};

pub const EU_ENDPOINT: &str = "wss://stt-rt.eu.soniox.com/transcribe-websocket";
pub const MODEL: &str = "stt-rt-v5";

pub struct Soniox {
    pub settings: ProviderSettings,
}

impl SttProvider for Soniox {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Soniox
    }

    fn stream<'a>(
        &'a self,
        audio: &'a mut mpsc::Receiver<AudioMsg>,
        events: &'a EventSender,
    ) -> BoxFuture<'a, Result<(), SttError>> {
        Box::pin(async move {
            let mut codec = SonioxCodec::new(&self.settings);
            let headers = [("authorization", format!("Bearer {}", self.settings.api_key))];
            ws::run(&self.settings.endpoint, &headers, &mut codec, audio, events).await
        })
    }
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    tokens: Vec<Token>,
    #[serde(default)]
    finished: bool,
    #[serde(default)]
    error_code: Option<u16>,
    #[serde(default)]
    error_type: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Clone, Deserialize)]
struct Token {
    text: String,
    #[serde(default)]
    start_ms: Option<u64>,
    #[serde(default)]
    end_ms: Option<u64>,
    #[serde(default)]
    confidence: Option<f32>,
    is_final: bool,
    #[serde(default)]
    language: Option<String>,
}

impl Token {
    fn is_marker(&self) -> bool {
        self.text == "<end>" || self.text == "<fin>"
    }
}

pub struct SonioxCodec {
    config: serde_json::Value,
    turn: u64,
    finals: Vec<Token>,
    last_partial: String,
}

impl SonioxCodec {
    pub fn new(settings: &ProviderSettings) -> Self {
        let mut config = json!({
            "model": MODEL,
            "audio_format": "pcm_s16le",
            "sample_rate": 16000,
            "num_channels": 1,
            "enable_language_identification": true,
            "enable_endpoint_detection": true,
            // Soniox's suggested low-latency starting point for voice agents.
            "endpoint_latency_adjustment_level": 2,
            "endpoint_sensitivity": 0.3,
            "max_endpoint_delay_ms": 1500,
        });
        if !settings.language_hints.is_empty() {
            config["language_hints"] = json!(settings.language_hints);
        }
        let mut context = serde_json::Map::new();
        if !settings.terms.is_empty() {
            context.insert("terms".into(), json!(settings.terms));
        }
        if !settings.context.is_empty() {
            context.insert("text".into(), json!(settings.context));
        }
        if !context.is_empty() {
            config["context"] = serde_json::Value::Object(context);
        }
        Self {
            config,
            turn: 0,
            finals: Vec::new(),
            last_partial: String::new(),
        }
    }

    fn close_turn(&mut self) -> Option<ProviderEvent> {
        let tokens = std::mem::take(&mut self.finals);
        self.last_partial.clear();
        let text = join(&tokens);
        if text.is_empty() {
            return None;
        }
        let confidences: Vec<f32> = tokens.iter().filter_map(|t| t.confidence).collect();
        let event = ProviderEvent::Final {
            turn: self.turn,
            text,
            start_ms: tokens.iter().find_map(|t| t.start_ms).unwrap_or(0),
            end_ms: tokens.iter().rev().find_map(|t| t.end_ms).unwrap_or(0),
            confidence: (!confidences.is_empty())
                .then(|| confidences.iter().sum::<f32>() / confidences.len() as f32),
            language: majority_language(&tokens),
        };
        self.turn += 1;
        Some(event)
    }
}

/// Soniox tokens carry their own leading spaces (sub-word pieces), so they are concatenated.
fn join(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect::<String>()
        .trim()
        .to_string()
}

fn majority_language(tokens: &[Token]) -> Option<String> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for language in tokens.iter().filter_map(|t| t.language.as_deref()) {
        match counts.iter_mut().find(|(l, _)| *l == language) {
            Some((_, n)) => *n += 1,
            None => counts.push((language, 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(l, _)| l.to_string())
}

impl Codec for SonioxCodec {
    fn on_open(&mut self) -> Vec<Message> {
        vec![Message::Text(self.config.to_string().into())]
    }

    fn on_text(&mut self, text: &str) -> Result<Vec<CodecOutput>, SttError> {
        let response: Response = serde_json::from_str(text).map_err(|e| {
            SttError::new(SttErrorCode::Protocol, format!("unexpected message: {e}"))
        })?;
        if let Some(status) = response.error_code {
            let detail = format!(
                "{status} {}: {}",
                response.error_type.as_deref().unwrap_or("error"),
                response.error_message.as_deref().unwrap_or("")
            );
            let code = match status {
                401 | 403 => SttErrorCode::Unauthorized,
                402 => SttErrorCode::Quota,
                408 | 413 | 429 => SttErrorCode::Network,
                500..=599 => SttErrorCode::Provider,
                _ => SttErrorCode::Protocol,
            };
            return Err(SttError::new(code, detail));
        }

        let mut out = Vec::new();
        let mut pending = Vec::new();
        for token in response.tokens.into_iter().filter(|t| !t.text.is_empty()) {
            if token.is_marker() {
                if let Some(event) = self.close_turn() {
                    out.push(CodecOutput::Event(event));
                }
            } else if token.is_final {
                self.finals.push(token);
            } else {
                pending.push(token);
            }
        }

        let partial = join(&[self.finals.clone(), pending.clone()].concat());
        if !partial.is_empty() && partial != self.last_partial {
            let all: Vec<&Token> = self.finals.iter().chain(pending.iter()).collect();
            out.push(CodecOutput::Event(ProviderEvent::Partial {
                turn: self.turn,
                text: partial.clone(),
                start_ms: all.iter().find_map(|t| t.start_ms).unwrap_or(0),
                end_ms: all.iter().rev().find_map(|t| t.end_ms).unwrap_or(0),
            }));
            self.last_partial = partial;
        }

        if response.finished {
            if let Some(event) = self.close_turn() {
                out.push(CodecOutput::Event(event));
            }
            out.push(CodecOutput::Finished);
        }
        Ok(out)
    }

    fn force_endpoint(&self) -> Message {
        Message::Text(json!({ "type": "finalize" }).to_string().into())
    }

    fn finish(&self) -> Message {
        Message::Text(String::new().into())
    }

    fn classify_close(&self, frame: Option<&CloseFrame>) -> SttError {
        let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
        SttError::new(SttErrorCode::Network, format!("closed by Soniox: {reason}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec() -> SonioxCodec {
        SonioxCodec::new(&ProviderSettings {
            endpoint: EU_ENDPOINT.into(),
            api_key: "key".into(),
            terms: vec!["Vue.js".into()],
            context: "Entretien technique".into(),
            language_hints: vec!["fr".into(), "en".into()],
        })
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

    fn token(text: &str, start: u64, is_final: bool, language: &str) -> String {
        format!(
            r#"{{"text":"{text}","start_ms":{start},"end_ms":{},"confidence":0.9,"is_final":{is_final},"language":"{language}"}}"#,
            start + 100
        )
    }

    #[test]
    fn start_request_uses_pcm16_eu_settings_and_no_key_in_the_body() {
        let mut c = codec();
        let Message::Text(config) = &c.on_open()[0] else {
            panic!()
        };
        let config: serde_json::Value = serde_json::from_str(config.as_str()).unwrap();
        assert_eq!(config["model"], "stt-rt-v5");
        assert_eq!(config["audio_format"], "pcm_s16le");
        assert_eq!(config["sample_rate"], 16000);
        assert_eq!(config["language_hints"], json!(["fr", "en"]));
        assert_eq!(config["enable_endpoint_detection"], true);
        assert_eq!(config["context"]["terms"], json!(["Vue.js"]));
        assert!(
            config.get("api_key").is_none(),
            "the key travels in the Authorization header"
        );
    }

    #[test]
    fn assembles_partials_from_final_and_pending_tokens_then_closes_on_end() {
        let mut c = codec();
        let first = format!(
            r#"{{"tokens":[{},{}]}}"#,
            token("Comment", 0, true, "fr"),
            token(" tu", 100, false, "fr")
        );
        assert_eq!(
            events(c.on_text(&first).unwrap()),
            vec![ProviderEvent::Partial {
                turn: 0,
                text: "Comment tu".into(),
                start_ms: 0,
                end_ms: 200
            }]
        );

        // Non-final tokens are replaced: " tu" became final, the pending tail is new.
        let second = format!(
            r#"{{"tokens":[{},{},{}]}}"#,
            token(" tu", 100, true, "fr"),
            token(" gères le", 200, true, "fr"),
            token(" state", 300, false, "en")
        );
        assert_eq!(
            events(c.on_text(&second).unwrap()),
            vec![ProviderEvent::Partial {
                turn: 0,
                text: "Comment tu gères le state".into(),
                start_ms: 0,
                end_ms: 400
            }]
        );

        let end = format!(
            r#"{{"tokens":[{},{},{{"text":"<end>","is_final":true}}]}}"#,
            token(" state", 300, true, "en"),
            token(" management ?", 400, true, "en")
        );
        let out = events(c.on_text(&end).unwrap());
        assert_eq!(out.len(), 1);
        let ProviderEvent::Final {
            turn,
            text,
            start_ms,
            end_ms,
            language,
            ..
        } = &out[0]
        else {
            panic!("{out:?}")
        };
        assert_eq!(
            (*turn, text.as_str()),
            (0, "Comment tu gères le state management ?")
        );
        assert_eq!((*start_ms, *end_ms), (0, 500));
        assert_eq!(language.as_deref(), Some("fr"));

        // The next words start turn 1.
        let next = format!(r#"{{"tokens":[{}]}}"#, token("Pourquoi", 600, false, "fr"));
        assert!(matches!(
            events(c.on_text(&next).unwrap())[..],
            [ProviderEvent::Partial { turn: 1, .. }]
        ));
    }

    #[test]
    fn manual_finalization_closes_the_turn_and_empty_markers_emit_nothing() {
        let mut c = codec();
        c.on_text(&format!(
            r#"{{"tokens":[{}]}}"#,
            token("Bonjour", 0, true, "fr")
        ))
        .unwrap();
        let out = events(
            c.on_text(r#"{"tokens":[{"text":"<fin>","is_final":true}]}"#)
                .unwrap(),
        );
        assert!(matches!(&out[..], [ProviderEvent::Final { text, .. }] if text == "Bonjour"));
        assert!(
            events(
                c.on_text(r#"{"tokens":[{"text":"<end>","is_final":true}]}"#)
                    .unwrap()
            )
            .is_empty()
        );
    }

    #[test]
    fn unchanged_partials_are_not_repeated() {
        let mut c = codec();
        let msg = format!(r#"{{"tokens":[{}]}}"#, token("Hello", 0, false, "en"));
        assert_eq!(events(c.on_text(&msg).unwrap()).len(), 1);
        assert!(events(c.on_text(&msg).unwrap()).is_empty());
        assert!(
            events(
                c.on_text(r#"{"tokens":[],"final_audio_proc_ms":10}"#)
                    .unwrap()
            )
            .is_empty()
        );
    }

    #[test]
    fn finished_flushes_pending_final_words() {
        let mut c = codec();
        c.on_text(&format!(
            r#"{{"tokens":[{}]}}"#,
            token("Merci", 0, true, "fr")
        ))
        .unwrap();
        let out = c.on_text(r#"{"tokens":[],"finished":true}"#).unwrap();
        assert!(matches!(
            &out[..],
            [
                CodecOutput::Event(ProviderEvent::Final { .. }),
                CodecOutput::Finished
            ]
        ));
    }

    #[test]
    fn error_responses_map_to_retry_or_stop() {
        let mut c = codec();
        let err = |code: u16| {
            codec()
                .on_text(&format!(
                    r#"{{"error_code":{code},"error_type":"x","error_message":"m"}}"#
                ))
                .unwrap_err()
        };
        assert_eq!(err(401).code, SttErrorCode::Unauthorized);
        assert_eq!(err(402).code, SttErrorCode::Quota);
        assert!(
            err(413).is_recoverable(),
            "max duration: open a new connection"
        );
        assert!(err(503).is_recoverable());
        assert!(!err(400).is_recoverable());
        assert_eq!(c.on_text("oops").unwrap_err().code, SttErrorCode::Protocol);
    }

    #[test]
    fn control_messages_match_the_documented_formats() {
        let c = codec();
        assert_eq!(
            c.force_endpoint(),
            Message::Text(r#"{"type":"finalize"}"#.into())
        );
        assert_eq!(
            c.finish(),
            Message::Text("".into()),
            "an empty text frame ends the stream"
        );
    }
}
