//! CV import: the PDF goes to Claude once (as a `document` block) and comes back as a draft
//! profile in structured JSON. The draft is never used as is: the user reviews it, and only
//! the saved version is verified. The webview reads the dropped file itself and sends its
//! bytes, so Rust never opens a path chosen by the page.

use std::time::Duration;

use serde::Deserialize;

use super::{MAX_FIELD_CHARS, Profile, ProfileError, ProfileErrorCode};
use crate::llm::{ClaudeClient, ClaudeModel, LlmError, LlmErrorCode};

pub const PROMPT: &str = include_str!("../../prompts/profile-extraction.v1.md");
pub const PROMPT_VERSION: &str = "profile-extraction.v1";
pub const MAX_PDF_BYTES: usize = 10 * 1024 * 1024;
/// One-off, accuracy over speed: the default (most capable) model, thinking allowed.
pub const MODEL: ClaudeModel = ClaudeModel::Opus55;
pub const TIMEOUT: Duration = Duration::from_secs(150);

/// Checks that `base64` is a PDF of acceptable size, without decoding it.
pub fn validate_pdf(base64: &str) -> Result<(), ProfileError> {
    let invalid = |detail: &str| Err(ProfileError::new(ProfileErrorCode::Invalid, detail));
    // "%PDF" encodes as "JVBERi".
    if !base64.starts_with("JVBERi") {
        return invalid("this file is not a PDF");
    }
    if base64.len() > MAX_PDF_BYTES.div_ceil(3) * 4 {
        return invalid("the PDF is larger than 10 MB");
    }
    if !base64
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'='))
    {
        return invalid("the file content is not valid base64");
    }
    Ok(())
}

fn schema() -> serde_json::Value {
    let fields = [
        "name",
        "headline",
        "summary",
        "skills",
        "experience",
        "projects",
        "education",
        "languages",
    ];
    let properties: serde_json::Map<String, serde_json::Value> = fields
        .iter()
        .map(|f| (f.to_string(), serde_json::json!({ "type": "string" })))
        .collect();
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": fields,
        "additionalProperties": false
    })
}

pub fn request_body(pdf_base64: &str) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": MODEL.id(),
        "max_tokens": 8_000,
        "stream": true,
        "system": PROMPT,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "document",
                    "source": { "type": "base64", "media_type": "application/pdf", "data": pdf_base64 }
                },
                { "type": "text", "text": "Extract the profile from this CV." }
            ]
        }],
        "output_config": {
            "effort": "medium",
            "format": { "type": "json_schema", "schema": schema() }
        }
    });
    MODEL.apply_settings(&mut body);
    body
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Extracted {
    name: String,
    headline: String,
    summary: String,
    skills: String,
    experience: String,
    projects: String,
    education: String,
    languages: String,
}

fn clip(text: String) -> String {
    text.trim().chars().take(MAX_FIELD_CHARS).collect()
}

/// Parses the model's JSON into an unverified draft.
pub fn parse_draft(json: &str) -> Result<Profile, LlmError> {
    let e: Extracted = serde_json::from_str(json.trim()).map_err(|err| {
        LlmError::new(
            LlmErrorCode::InvalidResponse,
            format!("profile JSON: {err}"),
        )
    })?;
    Ok(Profile {
        name: clip(e.name),
        headline: clip(e.headline),
        summary: clip(e.summary),
        skills: clip(e.skills),
        experience: clip(e.experience),
        projects: clip(e.projects),
        education: clip(e.education),
        languages: clip(e.languages),
        target_role: String::new(),
        verified_at: None,
    })
}

pub async fn extract(api_key: String, pdf_base64: &str) -> Result<Profile, LlmError> {
    let client = ClaudeClient::new(api_key, TIMEOUT, None)?;
    let body = request_body(pdf_base64);
    let streamed = client
        .stream(&body, MODEL.beta(), &mut |_, _, _| {})
        .await?;
    parse_draft(&streamed.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_reasonable_pdfs_are_sent() {
        assert!(validate_pdf("JVBERi0xLjcKJeLjz9MK").is_ok());
        assert_eq!(
            validate_pdf("iVBORw0KGgo=").unwrap_err().code,
            ProfileErrorCode::Invalid,
            "a PNG"
        );
        assert!(validate_pdf("JVBERi0x\nLjcK").is_err(), "no line breaks");
        let huge = format!("JVBERi{}", "A".repeat(MAX_PDF_BYTES.div_ceil(3) * 4));
        assert!(validate_pdf(&huge).is_err());
    }

    #[test]
    fn the_request_sends_the_pdf_as_a_document_with_structured_output() {
        let body = request_body("JVBERi0xLjcK");
        assert_eq!(body["model"], "claude-opus-5-5");
        let document = &body["messages"][0]["content"][0];
        assert_eq!(document["type"], "document");
        assert_eq!(document["source"]["media_type"], "application/pdf");
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["fallbacks"], "default");
        let schema = &body["output_config"]["format"]["schema"];
        assert_eq!(schema["required"].as_array().unwrap().len(), 8);
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn drafts_are_unverified_clipped_and_strictly_parsed() {
        let json = r#"{"name":" Alex Martin ","headline":"","summary":"","skills":"Vue.js","experience":"","projects":"","education":"","languages":"Français"}"#;
        let draft = parse_draft(json).unwrap();
        assert_eq!(draft.name, "Alex Martin");
        assert_eq!(draft.verified_at, None);
        assert_eq!(draft.target_role, "");
        assert!(parse_draft(r#"{"name":"x"}"#).is_err());
        assert!(parse_draft(&json.replace("}", r#","age":"30"}"#)).is_err());
    }
}
