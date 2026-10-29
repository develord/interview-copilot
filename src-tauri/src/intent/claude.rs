//! Claude classification adapter: one streamed Messages API call per utterance, with
//! structured JSON output (`output_config.format`), `effort: "low"`, and the stable system
//! prompt cached with `cache_control`. Transport, retries and errors are `crate::llm`.
//!
//! Streaming lets the adapter time the first token and the moment the decisive fields are
//! complete, separately from the end of the response.

use std::time::Duration;

use futures_util::future::BoxFuture;

use super::classifier::{
    CallTiming, Classification, Classifier, ClassifyRequest, DECISIVE_FIELDS, PROMPT,
    complete_fields, output_schema, parse_classification, render_user_message,
};
use super::types::{ClassifyError, ClaudeModel};
use crate::llm::{ClaudeClient, Streamed};

/// Thinking and the JSON fit comfortably; a truncated answer is rejected, not repaired.
const MAX_TOKENS: u32 = 2_048;

pub struct ClaudeClassifier {
    client: ClaudeClient,
    model: ClaudeModel,
}

fn request_timeout(model: ClaudeModel) -> Duration {
    match model {
        ClaudeModel::Opus55 => Duration::from_secs(10),
        ClaudeModel::Sonnet55 => Duration::from_secs(8),
        ClaudeModel::Haiku55 => Duration::from_secs(6),
    }
}

impl ClaudeClassifier {
    pub fn new(
        api_key: String,
        model: ClaudeModel,
        endpoint: Option<String>,
    ) -> Result<Self, ClassifyError> {
        Ok(Self {
            client: ClaudeClient::new(api_key, request_timeout(model), endpoint)?,
            model,
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
        self.model.apply_settings(&mut body);
        body
    }

    /// Sends any streamed Messages request once, with this model's headers. Latency probes.
    pub async fn send_once(&self, body: &serde_json::Value) -> Result<Streamed, ClassifyError> {
        self.client.stream_once(body, self.model.beta()).await
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
            let mut decision_ms = None;
            let streamed = self
                .client
                .stream(&body, self.model.beta(), &mut |_, so_far, at| {
                    if decision_ms.is_none() {
                        let fields = complete_fields(so_far);
                        if DECISIVE_FIELDS.iter().all(|k| fields.contains_key(*k)) {
                            decision_ms = Some(at);
                        }
                    }
                })
                .await?;
            let mut classification = parse_classification(&streamed.text, request)?;
            classification.timing = Some(CallTiming {
                first_token_ms: streamed.first_token_ms,
                decision_ms,
                total_ms: streamed.total_ms,
                input_tokens: streamed.usage.input_tokens,
                cache_read_tokens: streamed.usage.cache_read_tokens,
                cache_write_tokens: streamed.usage.cache_write_tokens,
                output_tokens: streamed.usage.output_tokens,
            });
            Ok(classification)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::classifier::{Relation, Speaker};
    use crate::intent::types::Intent;
    use crate::llm::test_support::{fake_api, streamed_text};

    fn request() -> ClassifyRequest {
        ClassifyRequest {
            utterance: "Comment tu gères le state management dans Vue ?".into(),
            speaker: Speaker::Interviewer,
            transcript: Vec::new(),
            recent_questions: Vec::new(),
        }
    }

    const CLASSIFICATION: &str = r#"{"requiresResponse":true,"intent":"TECHNICAL_QUESTION","relation":"new","contextReference":null,"language":"fr","question":"Comment gères-tu le state management dans Vue ?","domain":"frontend","urgency":"high","confidence":0.95}"#;

    #[test]
    fn requests_use_structured_output_low_effort_caching_and_no_sampling() {
        for model in [
            ClaudeModel::Opus55,
            ClaudeModel::Sonnet55,
            ClaudeModel::Haiku55,
        ] {
            let classifier = ClaudeClassifier::new("k".into(), model, None).unwrap();
            let body = classifier.request_body(&request());
            assert_eq!(body["model"], model.id());
            assert_eq!(body["stream"], true);
            assert_eq!(body["output_config"]["effort"], "low");
            assert_eq!(body["output_config"]["format"]["type"], "json_schema");
            assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
            for forbidden in ["temperature", "top_p", "top_k"] {
                assert!(body.get(forbidden).is_none(), "{forbidden}");
            }
        }
        let haiku = ClaudeClassifier::new("k".into(), ClaudeModel::Haiku55, None).unwrap();
        assert_eq!(
            haiku.request_body(&request())["thinking"]["type"],
            "disabled"
        );
    }

    #[tokio::test]
    async fn classifies_through_the_api_and_times_the_decision() {
        let (url, seen) = fake_api(vec![(200, streamed_text(CLASSIFICATION, 17))]).await;
        let classifier =
            ClaudeClassifier::new("test-key".into(), ClaudeModel::Opus55, Some(url)).unwrap();
        let c = classifier.classify(&request()).await.unwrap();
        assert_eq!(c.intent, Intent::TechnicalQuestion);
        assert_eq!(c.relation, Relation::New);
        let timing = c.timing.unwrap();
        assert_eq!(timing.cache_read_tokens, 1400);
        // The decisive fields come first, so the decision precedes the end of the stream.
        let decision = timing.decision_ms.unwrap();
        assert!(decision <= timing.total_ms);
        let raw = seen.lock().unwrap()[0].to_lowercase();
        assert!(raw.contains("anthropic-beta: server-side-fallback-2026-07-01"));
        assert!(raw.contains("\"fallbacks\":\"default\""));
    }
}
