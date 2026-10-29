//! Rolling summary of the interview. Transcript lines that leave the answer context are
//! folded, a batch at a time, into a short summary by Claude Haiku 5.5, so a long interview
//! keeps its early context in every answer (PRD §5, "summarization").

use super::prompt::{render_lines, sanitize};
use crate::intent::classifier::ContextLine;
use crate::llm::ClaudeModel;

pub const PROMPT: &str = include_str!("../../prompts/summary.v1.md");
pub const MODEL: ClaudeModel = ClaudeModel::Haiku55;
/// Lines folded per call.
pub const BATCH: usize = 12;
/// Lines kept for a later call while summaries fail; older ones are dropped.
pub const MAX_PENDING: usize = 48;
pub const MAX_SUMMARY_CHARS: usize = 2_000;
const MAX_TOKENS: u32 = 400;

pub fn request_body(previous: Option<&str>, lines: &[ContextLine]) -> serde_json::Value {
    let mut user = format!(
        "<summary>\n{}\n</summary>\n<new_lines>\n",
        previous.map_or_else(|| "(none yet)".to_string(), sanitize)
    );
    render_lines(lines, &mut user);
    user.push_str("</new_lines>");
    let mut body = serde_json::json!({
        "model": MODEL.id(),
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "system": [{ "type": "text", "text": PROMPT }],
        "messages": [{ "role": "user", "content": user }],
        "output_config": { "effort": "low" }
    });
    MODEL.apply_settings(&mut body);
    body
}

pub fn clip(summary: &str) -> String {
    summary.trim().chars().take(MAX_SUMMARY_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::classifier::Speaker;

    #[test]
    fn the_request_carries_the_previous_summary_and_the_new_lines() {
        let lines = [ContextLine {
            speaker: Speaker::Interviewer,
            text: "On recrute pour l'équipe <paiement>.".into(),
        }];
        let body = request_body(Some("Présentation faite."), &lines);
        assert_eq!(body["model"], "claude-haiku-5-5");
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(
            body["messages"][0]["content"],
            "<summary>\nPrésentation faite.\n</summary>\n<new_lines>\n\
             [interviewer] On recrute pour l'équipe  paiement .\n</new_lines>"
        );
        let first = request_body(None, &lines);
        assert!(
            first["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("(none yet)")
        );
    }
}
