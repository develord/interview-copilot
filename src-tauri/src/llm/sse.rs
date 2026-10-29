//! Server-sent events of the Messages API: reassembling events from raw bytes, and folding
//! them into the answer text, its usage and timing.

use serde::Serialize;

use super::{LlmError, LlmErrorCode, kind_error};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
    pub output_tokens: u32,
}

/// What one streamed response produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Streamed {
    pub text: String,
    /// Request sent → first text token.
    pub first_token_ms: Option<u32>,
    /// Request sent → end of the response.
    pub total_ms: u32,
    pub usage: Usage,
}

/// Folds the stream events into the text of the (last) text block.
#[derive(Default)]
pub struct StreamState {
    text: String,
    text_block: Option<u64>,
    stop_reason: Option<String>,
    first_token_ms: Option<u32>,
    usage: Usage,
}

fn count(value: &serde_json::Value) -> u32 {
    value
        .as_u64()
        .map_or(0, |n| n.min(u64::from(u32::MAX)) as u32)
}

impl StreamState {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn has_text(&self) -> bool {
        !self.text.is_empty()
    }

    /// Applies one event; returns the text it added, if any.
    pub fn apply(&mut self, data: &str, at_ms: u32) -> Result<Option<String>, LlmError> {
        let event: serde_json::Value = serde_json::from_str(data).map_err(|e| {
            LlmError::new(LlmErrorCode::InvalidResponse, format!("stream event: {e}"))
        })?;
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                let usage = &event["message"]["usage"];
                self.usage.input_tokens = count(&usage["input_tokens"]);
                self.usage.cache_read_tokens = count(&usage["cache_read_input_tokens"]);
                self.usage.cache_write_tokens = count(&usage["cache_creation_input_tokens"]);
                self.usage.output_tokens = count(&usage["output_tokens"]);
            }
            // Thinking blocks come first on Opus; the answer is the text block.
            "content_block_start" if event["content_block"]["type"] == "text" => {
                self.text_block = event["index"].as_u64();
            }
            "content_block_delta"
                if event["delta"]["type"] == "text_delta"
                    && event["index"].as_u64() == self.text_block =>
            {
                if let Some(piece) = event["delta"]["text"].as_str().filter(|p| !p.is_empty()) {
                    self.first_token_ms.get_or_insert(at_ms);
                    self.text.push_str(piece);
                    return Ok(Some(piece.to_string()));
                }
            }
            "message_delta" => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.to_string());
                }
                if event["usage"]["output_tokens"].is_u64() {
                    self.usage.output_tokens = count(&event["usage"]["output_tokens"]);
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
        Ok(None)
    }

    /// The text, after checking why generation stopped.
    pub fn finish(self, total_ms: u32) -> Result<Streamed, LlmError> {
        match self.stop_reason.as_deref() {
            Some("refusal") => {
                return Err(LlmError::new(
                    LlmErrorCode::Refused,
                    "the model declined this request",
                ));
            }
            Some("max_tokens") => {
                return Err(LlmError::new(
                    LlmErrorCode::InvalidResponse,
                    "the response was cut off (max_tokens)",
                ));
            }
            _ => {}
        }
        if self.text.trim().is_empty() {
            return Err(LlmError::new(
                LlmErrorCode::InvalidResponse,
                "no text block in the response",
            ));
        }
        Ok(Streamed {
            text: self.text,
            first_token_ms: self.first_token_ms,
            total_ms,
            usage: self.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{sse, streamed_text};
    use super::*;

    fn replay(body: &str) -> Result<(Streamed, Vec<String>), LlmError> {
        let mut events = SseBuffer::default();
        let mut state = StreamState::default();
        let mut pieces = Vec::new();
        for (n, data) in events.push(body.as_bytes()).into_iter().enumerate() {
            pieces.extend(state.apply(&data, n as u32)?);
        }
        Ok((state.finish(999)?, pieces))
    }

    #[test]
    fn the_text_block_after_thinking_is_the_answer_with_usage_and_timing() {
        let (streamed, pieces) = replay(&streamed_text("Comment gères-tu le state ?", 5)).unwrap();
        assert_eq!(streamed.text, "Comment gères-tu le state ?");
        assert_eq!(pieces.concat(), streamed.text);
        assert_eq!(streamed.total_ms, 999);
        // Events 0-4: message_start, thinking start/stop, ping, text start.
        assert_eq!(streamed.first_token_ms, Some(5));
        let u = streamed.usage;
        assert_eq!(
            (u.input_tokens, u.cache_read_tokens, u.output_tokens),
            (210, 1400, 64)
        );
    }

    #[test]
    fn events_cut_anywhere_are_reassembled() {
        let body = streamed_text("Un état partagé, géré côté serveur.", 4);
        assert!(
            body.contains('é'),
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
        let stop = |reason: &str| {
            sse(serde_json::json!({ "type": "message_delta",
                "delta": { "stop_reason": reason }, "usage": { "output_tokens": 3 } }))
        };
        assert_eq!(
            replay(&stop("refusal")).unwrap_err().code,
            LlmErrorCode::Refused
        );
        let cut = streamed_text("{\"a\":", 2).replace("end_turn", "max_tokens");
        assert_eq!(
            replay(&cut).unwrap_err().code,
            LlmErrorCode::InvalidResponse
        );
        assert_eq!(
            replay(&stop("end_turn")).unwrap_err().code,
            LlmErrorCode::InvalidResponse
        );
        let overloaded = sse(serde_json::json!({ "type": "error",
            "error": { "type": "overloaded_error", "message": "Overloaded" } }));
        assert_eq!(
            replay(&overloaded).unwrap_err().code,
            LlmErrorCode::Overloaded
        );
        assert_eq!(
            replay("data: not json\n\n").unwrap_err().code,
            LlmErrorCode::InvalidResponse
        );
    }
}
