//! Claude Messages API access shared by the classifier (`intent`), the answer engine (`answer`)
//! and the CV import (`profile`): models, errors, the streamed HTTP client and the API key.
//! Raw HTTP (there is no official Rust SDK), following the Claude API reference:
//! `anthropic-version: 2026-10-21`, and no sampling parameters (rejected on that version).

pub mod credentials;
mod sse;
#[cfg(test)]
pub mod test_support;

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub use sse::{SseBuffer, StreamState, Streamed, Usage};

pub const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
/// Opus 5.5 and Sonnet 5.5: a request declined by a safety classifier is re-run server-side
/// on the model Anthropic recommends for that category. Haiku 5.5 has no server-side fallback.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_millis(500);
const MAX_RETRY_AFTER: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClaudeModel {
    #[serde(rename = "claude-opus-5-5")]
    Opus55,
    #[serde(rename = "claude-sonnet-5-5")]
    Sonnet55,
    #[serde(rename = "claude-haiku-5-5")]
    Haiku55,
}

impl ClaudeModel {
    pub fn id(self) -> &'static str {
        match self {
            Self::Opus55 => "claude-opus-5-5",
            Self::Sonnet55 => "claude-sonnet-5-5",
            Self::Haiku55 => "claude-haiku-5-5",
        }
    }

    /// The `anthropic-beta` header this model's requests need.
    pub fn beta(self) -> Option<&'static str> {
        match self {
            Self::Opus55 | Self::Sonnet55 => Some(FALLBACK_BETA),
            Self::Haiku55 => None,
        }
    }

    /// Adds the model-specific fields: server-side fallbacks where they exist, and thinking
    /// off where the model allows it (the requests here are latency-sensitive).
    pub fn apply_settings(self, body: &mut serde_json::Value) {
        match self {
            // Thinking cannot be turned off on Opus 5.5; a low effort keeps it short.
            Self::Opus55 => body["fallbacks"] = serde_json::json!("default"),
            // `between_tools` is Sonnet 5.5's way to turn thinking off (accepted at effort
            // `high` or below, with no other thinking field).
            Self::Sonnet55 => {
                body["fallbacks"] = serde_json::json!("default");
                body["thinking"] = serde_json::json!({ "type": "between_tools" });
            }
            // Accepted at effort `high` or below.
            Self::Haiku55 => body["thinking"] = serde_json::json!({ "type": "disabled" }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LlmErrorCode {
    MissingKey,
    Unauthorized,
    /// Billing problem or exhausted credit.
    Quota,
    RateLimited,
    /// 5xx / 529.
    Overloaded,
    Network,
    Timeout,
    /// The response did not match what was asked (JSON, format, truncation).
    InvalidResponse,
    /// The model declined (`stop_reason: "refusal"`).
    Refused,
    /// Malformed request — a bug or an API change.
    Protocol,
    /// The OS credential store failed (saving or reading the key).
    CredentialStore,
}

impl LlmErrorCode {
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::Overloaded | Self::Network | Self::Timeout
        )
    }

    /// Errors that will not go away by themselves during a session.
    pub fn is_fatal(self) -> bool {
        matches!(
            self,
            Self::MissingKey | Self::Unauthorized | Self::Quota | Self::CredentialStore
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LlmError {
    pub code: LlmErrorCode,
    pub detail: String,
}

impl LlmError {
    pub fn new(code: LlmErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

/// One HTTP client per use (its timeout covers the whole streamed response).
pub struct ClaudeClient {
    http: reqwest::Client,
    api_key: String,
    endpoint: String,
}

struct Failure {
    error: LlmError,
    retry_after: Option<Duration>,
    /// Text was already handed to the caller: retrying would duplicate it.
    emitted: bool,
}

impl ClaudeClient {
    pub fn new(
        api_key: String,
        timeout: Duration,
        endpoint: Option<String>,
    ) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(timeout)
            .build()
            .map_err(|e| LlmError::new(LlmErrorCode::Network, e.to_string()))?;
        Ok(Self {
            http,
            api_key,
            endpoint: endpoint.unwrap_or_else(|| ENDPOINT.to_string()),
        })
    }

    /// Sends a streamed Messages request. `on_text(piece, text_so_far, at_ms)` runs on every
    /// text delta. A transient failure before any text is retried once (a timeout is not:
    /// it already spent the time budget).
    pub async fn stream(
        &self,
        body: &serde_json::Value,
        beta: Option<&str>,
        on_text: &mut (dyn FnMut(&str, &str, u32) + Send),
    ) -> Result<Streamed, LlmError> {
        match self.attempt(body, beta, on_text).await {
            Ok(streamed) => Ok(streamed),
            Err(failure)
                if !failure.emitted
                    && failure.error.code.is_retryable()
                    && failure.error.code != LlmErrorCode::Timeout =>
            {
                let delay = failure
                    .retry_after
                    .filter(|d| *d <= MAX_RETRY_AFTER)
                    .unwrap_or(RETRY_DELAY);
                tokio::time::sleep(delay).await;
                self.attempt(body, beta, on_text).await.map_err(|f| f.error)
            }
            Err(failure) => Err(failure.error),
        }
    }

    /// One attempt, no retry (latency probes).
    pub async fn stream_once(
        &self,
        body: &serde_json::Value,
        beta: Option<&str>,
    ) -> Result<Streamed, LlmError> {
        self.attempt(body, beta, &mut |_, _, _| {})
            .await
            .map_err(|f| f.error)
    }

    async fn attempt(
        &self,
        body: &serde_json::Value,
        beta: Option<&str>,
        on_text: &mut (dyn FnMut(&str, &str, u32) + Send),
    ) -> Result<Streamed, Failure> {
        let fail = |error: LlmError, emitted: bool| Failure {
            error,
            retry_after: None,
            emitted,
        };
        let started = Instant::now();
        let mut http = self
            .http
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(body);
        if let Some(beta) = beta {
            http = http.header("anthropic-beta", beta);
        }
        let mut response = http
            .send()
            .await
            .map_err(|e| fail(transport_error(e), false))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let text = response
                .text()
                .await
                .map_err(|e| fail(transport_error(e), false))?;
            return Err(Failure {
                error: status_error(status, &text),
                retry_after,
                emitted: false,
            });
        }
        let mut events = SseBuffer::default();
        let mut stream = StreamState::default();
        loop {
            let emitted = stream.has_text();
            let chunk = match response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(e) => return Err(fail(transport_error(e), emitted)),
            };
            for data in events.push(&chunk) {
                let at = millis(started);
                let piece = stream
                    .apply(&data, at)
                    .map_err(|e| fail(e, stream.has_text()))?;
                if let Some(piece) = piece {
                    on_text(&piece, stream.text(), at);
                }
            }
        }
        let emitted = stream.has_text();
        stream.finish(millis(started)).map_err(|e| fail(e, emitted))
    }
}

pub fn millis(since: Instant) -> u32 {
    since.elapsed().as_millis().min(u128::from(u32::MAX)) as u32
}

fn transport_error(error: reqwest::Error) -> LlmError {
    let code = if error.is_timeout() {
        LlmErrorCode::Timeout
    } else {
        LlmErrorCode::Network
    };
    LlmError::new(code, error.to_string())
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
pub(crate) fn kind_error(kind: &str, message: &str) -> LlmError {
    let code = match kind {
        "authentication_error" | "permission_error" => LlmErrorCode::Unauthorized,
        "billing_error" => LlmErrorCode::Quota,
        "rate_limit_error" => LlmErrorCode::RateLimited,
        "overloaded_error" | "api_error" => LlmErrorCode::Overloaded,
        _ if message.to_lowercase().contains("credit balance") => LlmErrorCode::Quota,
        _ => LlmErrorCode::Protocol,
    };
    LlmError::new(code, message)
}

/// Maps an HTTP error to a stable code (status first, then the documented error type).
pub fn status_error(status: u16, body: &str) -> LlmError {
    let detail = serde_json::from_str::<ErrorBody>(body)
        .ok()
        .map(|b| b.error);
    let message = detail.as_ref().map_or_else(
        || format!("HTTP {status}"),
        |d| format!("HTTP {status}: {}", d.message),
    );
    let kind = detail.as_ref().map(|d| d.kind.as_str()).unwrap_or_default();
    let code = match status {
        401 | 403 => LlmErrorCode::Unauthorized,
        402 => LlmErrorCode::Quota,
        429 => LlmErrorCode::RateLimited,
        500..=599 => LlmErrorCode::Overloaded,
        _ => kind_error(kind, &message).code,
    };
    LlmError::new(code, message)
}

#[cfg(test)]
mod tests {
    use super::test_support::{fake_api, streamed_text};
    use super::*;

    #[test]
    fn model_settings_follow_each_models_rules() {
        let mut opus = serde_json::json!({});
        ClaudeModel::Opus55.apply_settings(&mut opus);
        assert_eq!(opus["fallbacks"], "default");
        assert!(
            opus.get("thinking").is_none(),
            "Opus 5.5 rejects thinking off"
        );

        let mut sonnet = serde_json::json!({});
        ClaudeModel::Sonnet55.apply_settings(&mut sonnet);
        assert_eq!(
            sonnet["thinking"],
            serde_json::json!({ "type": "between_tools" })
        );
        assert_eq!(sonnet["fallbacks"], "default");

        let mut haiku = serde_json::json!({});
        ClaudeModel::Haiku55.apply_settings(&mut haiku);
        assert_eq!(haiku["thinking"]["type"], "disabled");
        assert!(
            haiku.get("fallbacks").is_none(),
            "Haiku 5.5 has no fallback"
        );
        assert_eq!(ClaudeModel::Haiku55.beta(), None);
        assert_eq!(
            ClaudeModel::Sonnet55.beta(),
            Some("server-side-fallback-2026-07-01")
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
                LlmErrorCode::Unauthorized,
            ),
            (
                403,
                body("permission_error", "no"),
                LlmErrorCode::Unauthorized,
            ),
            (402, body("billing_error", "pay"), LlmErrorCode::Quota),
            (
                400,
                body("invalid_request_error", "Your credit balance is too low"),
                LlmErrorCode::Quota,
            ),
            (
                400,
                body("invalid_request_error", "bad field"),
                LlmErrorCode::Protocol,
            ),
            (
                404,
                body("not_found_error", "model"),
                LlmErrorCode::Protocol,
            ),
            (
                429,
                body("rate_limit_error", "slow down"),
                LlmErrorCode::RateLimited,
            ),
            (
                529,
                body("overloaded_error", "busy"),
                LlmErrorCode::Overloaded,
            ),
            (
                502,
                "<html>bad gateway</html>".into(),
                LlmErrorCode::Overloaded,
            ),
        ];
        for (status, text, expected) in cases {
            let error = status_error(status, &text);
            assert_eq!(error.code, expected, "{status} {text}");
            assert!(error.detail.starts_with(&format!("HTTP {status}")));
        }
    }

    fn client(url: String) -> ClaudeClient {
        ClaudeClient::new("test-key".into(), Duration::from_secs(5), Some(url)).unwrap()
    }

    #[tokio::test]
    async fn streams_text_pieces_with_the_required_headers() {
        let (url, seen) = fake_api(vec![(200, streamed_text("Bonjour à tous", 4))]).await;
        let mut pieces = Vec::new();
        let streamed = client(url)
            .stream(
                &serde_json::json!({}),
                Some("beta-x"),
                &mut |piece, so_far, _| pieces.push((piece.to_string(), so_far.to_string())),
            )
            .await
            .unwrap();
        assert_eq!(streamed.text, "Bonjour à tous");
        assert_eq!(pieces.first().unwrap().0, "Bonj");
        assert_eq!(pieces.last().unwrap().1, "Bonjour à tous");
        let raw = seen.lock().unwrap()[0].to_lowercase();
        assert!(raw.contains("x-api-key: test-key"));
        assert!(raw.contains("anthropic-version: 2026-10-21"));
        assert!(raw.contains("anthropic-beta: beta-x"));
    }

    #[tokio::test]
    async fn a_transient_failure_before_any_text_is_retried_once() {
        let busy = r#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#;
        let (url, seen) = fake_api(vec![(529, busy.into()), (200, streamed_text("ok", 2))]).await;
        let streamed = client(url)
            .stream(&serde_json::json!({}), None, &mut |_, _, _| {})
            .await
            .unwrap();
        assert_eq!(streamed.text, "ok");
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_rejected_key_and_a_failure_after_text_are_not_retried() {
        let denied =
            r#"{"type":"error","error":{"type":"authentication_error","message":"bad key"}}"#;
        let (url, seen) = fake_api(vec![(401, denied.into()), (200, streamed_text("x", 1))]).await;
        let error = client(url)
            .stream(&serde_json::json!({}), None, &mut |_, _, _| {})
            .await
            .unwrap_err();
        assert_eq!(error.code, LlmErrorCode::Unauthorized);
        assert_eq!(seen.lock().unwrap().len(), 1);

        // Text, then an overloaded error mid-stream: retrying would duplicate the text.
        let cut = streamed_text("partial", 3).replace(
            "event: message_stop",
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\nevent: message_stop",
        );
        let (url, seen) = fake_api(vec![(200, cut), (200, streamed_text("again", 2))]).await;
        let error = client(url)
            .stream(&serde_json::json!({}), None, &mut |_, _, _| {})
            .await
            .unwrap_err();
        assert_eq!(error.code, LlmErrorCode::Overloaded);
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}
