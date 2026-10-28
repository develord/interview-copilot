//! Claude classification adapter: one streamed Messages API call per utterance, structured JSON
//! output, raw HTTP (there is no official Rust SDK). Request shape per the Claude API
//! reference: `anthropic-version: 2026-10-21`, `output_config.format` JSON schema,
//! `effort: "low"`, the stable system prompt cached with `cache_control`, and no sampling
//! parameters (rejected on this API version).
//!
//! Streaming (server-sent events) lets the adapter time the first token and the moment the
//! decisive fields are complete, separately from the end of the response.

use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use serde::Deserialize;

use super::classifier::{
    CallTiming, Classification, Classifier, ClassifyRequest, DECISIVE_FIELDS, PROMPT,
    complete_fields, output_schema, parse_classification, render_user_message,
};
use super::types::{ClassifyError, ClassifyErrorCode, ClaudeModel};

pub const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
/// Opus only: a request declined by a safety classifier is re-run server-side on the model
/// Anthropic recommends for that category. Haiku 5.5 has no server-side fallback.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Thinking and the JSON fit comfortably; a truncated answer is rejected, not repaired.
const MAX_TOKENS: u32 = 2_048;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_millis(500);
const MAX_RETRY_AFTER: Duration = Duration::from_secs(3);

pub struct ClaudeClassifier {
    client: reqwest::Client,
    api_key: String,
    model: ClaudeModel,
    endpoint: String,
}

impl ClaudeClassifier {
    pub fn new(
        api_key: String,
        model: ClaudeModel,
        endpoint: Option<String>,
    ) -> Result<Self, ClassifyError> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(request_timeout(model))
            .build()
            .map_err(|e| ClassifyError::new(ClassifyErrorCode::Network, e.to_string()))?;
        Ok(Self {
            client,
            api_key,
            model,
            endpoint: endpoint.unwrap_or_else(|| ENDPOINT.to_string()),
        })
    }

    pub fn request_body(&self, request: &ClassifyRequest) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.model.id(),
            "max_tokens": MAX_TOKENS,
            "stream": true,
            "system": [{
                "type": "text",
                "text": PROMPT,
                "cache_control": { "type": "ephemeral" }
            }],
            "messages": [{ "role": "user", "content": render_user_message(request) }],
            "output_config": {
                "effort": "low",
                "format": { "type": "json_schema", "schema": output_schema() }
            }
        });
        match self.model {
            // Thinking cannot be turned off on Opus 5.5; low effort keeps it short.
            ClaudeModel::Opus55 => body["fallbacks"] = serde_json::json!("default"),
            // Accepted at effort `high` or below; the fastest setting for a classifier.
            ClaudeModel::Haiku55 => body["thinking"] = serde_json::json!({ "type": "disabled" }),
        }
        body
    }

    /// Sends any streamed Messages request once (no retry). For latency probes.
    pub async fn send_once(&self, body: &serde_json::Value) -> Result<Streamed, ClassifyError> {
        self.attempt(body).await.map_err(|f| f.error)
    }

    async fn attempt(&self, body: &serde_json::Value) -> Result<Streamed, Failure> {
        let started = Instant::now();
        let mut http = self
            .client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(body);
        if self.model == ClaudeModel::Opus55 {
            http = http.header("anthropic-beta", FALLBACK_BETA);
        }
        let mut response = http.send().await.map_err(transport_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let text = response.text().await.map_err(transport_error)?;
            return Err(Failure {
                error: status_error(status, &text),
                retry_after,
            });
        }
        let mut events = SseBuffer::default();
        let mut stream = StreamState::default();
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            for data in events.push(&chunk) {
                stream.apply(&data, millis(started))?;
            }
        }
        Ok(stream.finish(millis(started))?)
    }
}

fn millis(since: Instant) -> u32 {
    since.elapsed().as_millis().min(u128::from(u32::MAX)) as u32
}

fn request_timeout(model: ClaudeModel) -> Duration {
    match model {
        ClaudeModel::Opus55 => Duration::from_secs(10),
        ClaudeModel::Haiku55 => Duration::from_secs(6),
    }
}

struct Failure {
    error: ClassifyError,
    retry_after: Option<Duration>,
}

impl From<ClassifyError> for Failure {
    fn from(error: ClassifyError) -> Self {
        Self {
            error,
            retry_after: None,
        }
    }
}

impl Classifier for ClaudeClassifier {
    fn model(&self) -> &'static str {
        self.model.id()
    }

    fn classify<'a>(
        &'a self,
        request: &'a ClassifyRequest,
    ) -> BoxFuture<'a, Result<Classification, ClassifyError>> {
        Box::pin(async move {
            let body = self.request_body(request);
            let streamed = match self.attempt(&body).await {
                Ok(streamed) => streamed,
                // One quick retry for transient failures; a timeout already spent the budget.
                Err(failure)
                    if failure.error.code.is_retryable()
                        && failure.error.code != ClassifyErrorCode::Timeout =>
                {
                    let delay = failure
                        .retry_after
                        .filter(|d| *d <= MAX_RETRY_AFTER)
                        .unwrap_or(RETRY_DELAY);
                    tokio::time::sleep(delay).await;
                    self.attempt(&body).await.map_err(|f| f.error)?
                }
                Err(failure) => return Err(failure.error),
            };
            let mut classification = parse_classification(&streamed.text, request)?;
            classification.timing = Some(streamed.timing);
            Ok(classification)
        })
    }
}

fn transport_error(error: reqwest::Error) -> Failure {
    let code = if error.is_timeout() {
        ClassifyErrorCode::Timeout
    } else {
        ClassifyErrorCode::Network
    };
    ClassifyError::new(code, error.to_string()).into()
}

/// Splits a server-sent event stream into the `data:` payloads of complete events. Bytes may
/// arrive cut anywhere, including inside a UTF-8 character.
#[derive(Default)]
pub struct SseBuffer {
    pending: Vec<u8>,
}

impl SseBuffer {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut payloads = Vec::new();
        while let Some((end, separator)) = event_end(&self.pending) {
            let event: Vec<u8> = self.pending.drain(..end + separator).collect();
            let text = String::from_utf8_lossy(&event[..end]);
            let data: Vec<&str> = text
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|d| d.strip_prefix(' ').unwrap_or(d))
                .collect();
            if !data.is_empty() {
                payloads.push(data.join("\n"));
            }
        }
        payloads
    }
}

fn event_end(bytes: &[u8]) -> Option<(usize, usize)> {
    (0..bytes.len()).find_map(|i| {
        if bytes[i..].starts_with(b"\n\n") {
            Some((i, 2))
        } else if bytes[i..].starts_with(b"\r\n\r\n") {
            Some((i, 4))
        } else {
            None
        }
    })
}

/// What one streamed response produced.
#[derive(Debug, PartialEq)]
pub struct Streamed {
    pub text: String,
    pub timing: CallTiming,
}

/// Folds the Messages API stream events into the JSON text and its timing.
#[derive(Default)]
pub struct StreamState {
    text: String,
    text_block: Option<u64>,
    stop_reason: Option<String>,
    timing: CallTiming,
}

fn count(value: &serde_json::Value) -> u32 {
    value
        .as_u64()
        .map_or(0, |n| n.min(u64::from(u32::MAX)) as u32)
}

impl StreamState {
    pub fn apply(&mut self, data: &str, at_ms: u32) -> Result<(), ClassifyError> {
        let event: serde_json::Value = serde_json::from_str(data).map_err(|e| {
            ClassifyError::new(
                ClassifyErrorCode::InvalidResponse,
                format!("stream event: {e}"),
            )
        })?;
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                let usage = &event["message"]["usage"];
                self.timing.input_tokens = count(&usage["input_tokens"]);
                self.timing.cache_read_tokens = count(&usage["cache_read_input_tokens"]);
                self.timing.cache_write_tokens = count(&usage["cache_creation_input_tokens"]);
                self.timing.output_tokens = count(&usage["output_tokens"]);
            }
            // Thinking blocks come first on Opus; the JSON is the text block.
            "content_block_start" if event["content_block"]["type"] == "text" => {
                self.text_block = event["index"].as_u64();
                self.text.clear();
            }
            "content_block_delta"
                if event["delta"]["type"] == "text_delta"
                    && event["index"].as_u64() == self.text_block =>
            {
                if let Some(piece) = event["delta"]["text"].as_str() {
                    self.timing.first_token_ms.get_or_insert(at_ms);
                    self.text.push_str(piece);
                    if self.timing.decision_ms.is_none() {
                        let fields = complete_fields(&self.text);
                        if DECISIVE_FIELDS.iter().all(|k| fields.contains_key(*k)) {
                            self.timing.decision_ms = Some(at_ms);
                        }
                    }
                }
            }
            "message_delta" => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.to_string());
                }
                if event["usage"]["output_tokens"].is_u64() {
                    self.timing.output_tokens = count(&event["usage"]["output_tokens"]);
                }
            }
            "error" => {
                let error = &event["error"];
                return Err(kind_error(
                    error["type"].as_str().unwrap_or_default(),
                    &format!(
                        "stream error: {}",
                        error["message"].as_str().unwrap_or_default()
                    ),
                ));
            }
            // ping, content_block_stop, message_stop, thinking deltas.
            _ => {}
        }
        Ok(())
    }

    /// The JSON text, after checking why generation stopped.
    pub fn finish(self, total_ms: u32) -> Result<Streamed, ClassifyError> {
        match self.stop_reason.as_deref() {
            Some("refusal") => {
                return Err(ClassifyError::new(
                    ClassifyErrorCode::Refused,
                    "the model declined to classify this utterance",
                ));
            }
            Some("max_tokens") => {
                return Err(ClassifyError::new(
                    ClassifyErrorCode::InvalidResponse,
                    "the classification was cut off (max_tokens)",
                ));
            }
            _ => {}
        }
        if self.text.trim().is_empty() {
            return Err(ClassifyError::new(
                ClassifyErrorCode::InvalidResponse,
                "no text block in the response",
            ));
        }
        Ok(Streamed {
            text: self.text,
            timing: CallTiming {
                total_ms,
                ..self.timing
            },
        })
    }
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    #[serde(rename = "type")]
    kind: String,
    message: String,
}

/// Maps a documented error type (`overloaded_error`…) to a stable code.
fn kind_error(kind: &str, message: &str) -> ClassifyError {
    let code = match kind {
        "authentication_error" | "permission_error" => ClassifyErrorCode::Unauthorized,
        "billing_error" => ClassifyErrorCode::Quota,
        "rate_limit_error" => ClassifyErrorCode::RateLimited,
        "overloaded_error" | "api_error" => ClassifyErrorCode::Overloaded,
        _ if message.to_lowercase().contains("credit balance") => ClassifyErrorCode::Quota,
        _ => ClassifyErrorCode::Protocol,
    };
    ClassifyError::new(code, message)
}

/// Maps an HTTP error to a stable code (status first, then the documented error type).
pub fn status_error(status: u16, body: &str) -> ClassifyError {
    let detail = serde_json::from_str::<ErrorBody>(body)
        .ok()
        .map(|b| b.error);
    let message = detail.as_ref().map_or_else(
        || format!("HTTP {status}"),
        |d| format!("HTTP {status}: {}", d.message),
    );
    let kind = detail.as_ref().map(|d| d.kind.as_str()).unwrap_or_default();
    let code = match status {
        401 | 403 => ClassifyErrorCode::Unauthorized,
        402 => ClassifyErrorCode::Quota,
        429 => ClassifyErrorCode::RateLimited,
        500..=599 => ClassifyErrorCode::Overloaded,
        _ => kind_error(kind, &message).code,
    };
    ClassifyError::new(code, message)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::intent::classifier::{Relation, Speaker};
    use crate::intent::types::Intent;

    fn request() -> ClassifyRequest {
        ClassifyRequest {
            utterance: "Comment tu gères le state management dans Vue ?".into(),
            speaker: Speaker::Interviewer,
            transcript: Vec::new(),
            recent_questions: Vec::new(),
        }
    }

    const CLASSIFICATION: &str = r#"{"requiresResponse":true,"intent":"TECHNICAL_QUESTION","relation":"new","contextReference":null,"language":"fr","question":"Comment gères-tu le state management dans Vue ?","domain":"frontend","urgency":"high","confidence":0.95}"#;

    /// Characters per streamed text delta.
    const PIECE: usize = 17;

    /// One SSE event in the Messages API wire format.
    fn sse(data: serde_json::Value) -> String {
        format!(
            "event: {}\ndata: {data}\n\n",
            data["type"].as_str().unwrap()
        )
    }

    /// A streamed response: a thinking block (as on Opus), then the JSON in small pieces.
    fn success_body() -> String {
        let mut out = sse(serde_json::json!({
            "type": "message_start",
            "message": { "id": "msg_1", "type": "message", "role": "assistant",
                "model": "claude-haiku-5-5", "content": [],
                "usage": { "input_tokens": 210, "cache_read_input_tokens": 1400,
                    "cache_creation_input_tokens": 0, "output_tokens": 1 } }
        }));
        out += &sse(
            serde_json::json!({ "type": "content_block_start", "index": 0,
            "content_block": { "type": "thinking", "thinking": "" } }),
        );
        out += &sse(serde_json::json!({ "type": "content_block_stop", "index": 0 }));
        out += &sse(serde_json::json!({ "type": "ping" }));
        out += &sse(
            serde_json::json!({ "type": "content_block_start", "index": 1,
            "content_block": { "type": "text", "text": "" } }),
        );
        let chars: Vec<char> = CLASSIFICATION.chars().collect();
        for piece in chars.chunks(PIECE).map(|c| c.iter().collect::<String>()) {
            out += &sse(
                serde_json::json!({ "type": "content_block_delta", "index": 1,
                "delta": { "type": "text_delta", "text": piece } }),
            );
        }
        out += &sse(serde_json::json!({ "type": "content_block_stop", "index": 1 }));
        out += &sse(serde_json::json!({ "type": "message_delta",
            "delta": { "stop_reason": "end_turn" }, "usage": { "output_tokens": 64 } }));
        out += &sse(serde_json::json!({ "type": "message_stop" }));
        out
    }

    fn replay(body: &str) -> Result<Streamed, ClassifyError> {
        let mut events = SseBuffer::default();
        let mut state = StreamState::default();
        for (n, data) in events.push(body.as_bytes()).into_iter().enumerate() {
            state.apply(&data, n as u32)?;
        }
        state.finish(999)
    }

    #[test]
    fn opus_requests_use_structured_output_low_effort_caching_and_fallbacks() {
        let classifier = ClaudeClassifier::new("k".into(), ClaudeModel::Opus55, None).unwrap();
        let body = classifier.request_body(&request());
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["output_config"]["effort"], "low");
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["fallbacks"], "default");
        assert!(
            body.get("thinking").is_none(),
            "thinking cannot be disabled on Opus 5.5"
        );
        for forbidden in ["temperature", "top_p", "top_k"] {
            assert!(body.get(forbidden).is_none(), "{forbidden}");
        }
    }

    #[test]
    fn haiku_requests_disable_thinking_and_send_no_fallbacks() {
        let classifier = ClaudeClassifier::new("k".into(), ClaudeModel::Haiku55, None).unwrap();
        let body = classifier.request_body(&request());
        assert_eq!(body["model"], "claude-haiku-5-5");
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("fallbacks").is_none());
    }

    #[test]
    fn the_text_block_of_the_stream_is_the_classification_with_its_timing() {
        let streamed = replay(&success_body()).unwrap();
        assert_eq!(streamed.text, CLASSIFICATION);
        let t = streamed.timing;
        assert_eq!(t.total_ms, 999);
        assert_eq!(
            (t.input_tokens, t.cache_read_tokens, t.output_tokens),
            (210, 1400, 64)
        );
        // Event 5 is the first text delta; the decisive fields complete before the end.
        assert_eq!(t.first_token_ms, Some(5));
        let last_delta = 5 + CLASSIFICATION.chars().count().div_ceil(PIECE) as u32 - 1;
        let decision = t.decision_ms.unwrap();
        assert!(
            decision > 5 && decision < last_delta,
            "{decision} vs {last_delta}"
        );
    }

    #[test]
    fn events_cut_anywhere_are_reassembled() {
        let body = success_body();
        assert!(
            body.contains('è'),
            "the stream carries multi-byte characters"
        );
        let mut events = SseBuffer::default();
        let mut payloads = Vec::new();
        // One byte at a time: cuts inside lines, separators and UTF-8 characters.
        for byte in body.as_bytes() {
            payloads.extend(events.push(std::slice::from_ref(byte)));
        }
        assert_eq!(payloads, SseBuffer::default().push(body.as_bytes()));
        let crlf = SseBuffer::default().push(b"event: ping\r\ndata: {\"type\":\"ping\"}\r\n\r\n");
        assert_eq!(crlf, ["{\"type\":\"ping\"}"]);
    }

    #[test]
    fn refusals_truncation_empty_content_and_stream_errors_are_errors() {
        let start = |reason: &str| {
            sse(serde_json::json!({ "type": "message_delta",
                "delta": { "stop_reason": reason }, "usage": { "output_tokens": 3 } }))
        };
        assert_eq!(
            replay(&start("refusal")).unwrap_err().code,
            ClassifyErrorCode::Refused
        );
        let cut = success_body().replace("end_turn", "max_tokens");
        assert_eq!(
            replay(&cut).unwrap_err().code,
            ClassifyErrorCode::InvalidResponse
        );
        assert_eq!(
            replay(&start("end_turn")).unwrap_err().code,
            ClassifyErrorCode::InvalidResponse
        );
        let overloaded = sse(serde_json::json!({ "type": "error",
            "error": { "type": "overloaded_error", "message": "Overloaded" } }));
        assert_eq!(
            replay(&overloaded).unwrap_err().code,
            ClassifyErrorCode::Overloaded
        );
        assert_eq!(
            replay("data: not json\n\n").unwrap_err().code,
            ClassifyErrorCode::InvalidResponse
        );
    }

    #[test]
    fn http_errors_map_to_stable_codes() {
        let body = |kind: &str, msg: &str| {
            format!(r#"{{"type":"error","error":{{"type":"{kind}","message":"{msg}"}}}}"#)
        };
        let cases = [
            (
                401,
                body("authentication_error", "invalid x-api-key"),
                ClassifyErrorCode::Unauthorized,
            ),
            (
                403,
                body("permission_error", "no"),
                ClassifyErrorCode::Unauthorized,
            ),
            (402, body("billing_error", "pay"), ClassifyErrorCode::Quota),
            (
                400,
                body("invalid_request_error", "Your credit balance is too low"),
                ClassifyErrorCode::Quota,
            ),
            (
                400,
                body("invalid_request_error", "bad field"),
                ClassifyErrorCode::Protocol,
            ),
            (
                404,
                body("not_found_error", "model"),
                ClassifyErrorCode::Protocol,
            ),
            (
                429,
                body("rate_limit_error", "slow down"),
                ClassifyErrorCode::RateLimited,
            ),
            (
                529,
                body("overloaded_error", "busy"),
                ClassifyErrorCode::Overloaded,
            ),
            (
                502,
                "<html>bad gateway</html>".into(),
                ClassifyErrorCode::Overloaded,
            ),
        ];
        for (status, text, expected) in cases {
            let error = status_error(status, &text);
            assert_eq!(error.code, expected, "{status} {text}");
            assert!(error.detail.starts_with(&format!("HTTP {status}")));
        }
    }

    /// Minimal HTTP/1.1 server: answers each connection with the next canned response and
    /// records the raw requests.
    async fn fake_api(responses: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/messages", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            for (status, body) in responses {
                let (mut tcp, _) = listener.accept().await.unwrap();
                let mut raw = Vec::new();
                let mut buf = [0u8; 8192];
                loop {
                    let n = tcp.read(&mut buf).await.unwrap();
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if raw.len() >= end + 4 + length {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                log.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&raw).to_string());
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                tcp.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (url, seen)
    }

    #[tokio::test]
    async fn classifies_through_the_http_api_with_the_required_headers() {
        let (url, seen) = fake_api(vec![(200, success_body())]).await;
        let classifier =
            ClaudeClassifier::new("test-key".into(), ClaudeModel::Opus55, Some(url)).unwrap();
        let c = classifier.classify(&request()).await.unwrap();
        assert_eq!(c.intent, Intent::TechnicalQuestion);
        assert_eq!(c.relation, Relation::New);
        let timing = c.timing.unwrap();
        assert!(timing.decision_ms.is_some());
        assert_eq!(timing.cache_read_tokens, 1400);

        let raw = seen.lock().unwrap()[0].to_lowercase();
        assert!(raw.contains("x-api-key: test-key"));
        assert!(raw.contains("anthropic-version: 2026-10-21"));
        assert!(raw.contains("anthropic-beta: server-side-fallback-2026-07-01"));
    }

    #[tokio::test]
    async fn a_transient_failure_is_retried_once() {
        let overloaded = r#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#;
        let (url, seen) = fake_api(vec![(529, overloaded.into()), (200, success_body())]).await;
        let classifier =
            ClaudeClassifier::new("k".into(), ClaudeModel::Haiku55, Some(url)).unwrap();
        assert!(classifier.classify(&request()).await.is_ok());
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn an_unauthorized_key_is_not_retried() {
        let denied = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        let (url, seen) = fake_api(vec![(401, denied.into()), (200, success_body())]).await;
        let classifier =
            ClaudeClassifier::new("k".into(), ClaudeModel::Haiku55, Some(url)).unwrap();
        let error = classifier.classify(&request()).await.unwrap_err();
        assert_eq!(error.code, ClassifyErrorCode::Unauthorized);
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}
