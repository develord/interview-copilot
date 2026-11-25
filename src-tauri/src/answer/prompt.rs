//! The answer request: versioned instructions and the verified profile as a cached system
//! prefix, then a bounded context and the question.

use crate::intent::classifier::{ContextLine, Speaker};
use crate::intent::types::{Intent, Language};
use crate::llm::ClaudeModel;
use crate::profile::Profile;

pub const PROMPT: &str = include_str!("../../prompts/answer.v2.md");
pub const PROMPT_VERSION: &str = "answer.v2";
/// Separates the main answer from the details (see the prompt's "Format").
pub const DETAILS_MARKER: &str = "<<<DETAILS>>>";
/// The main answer is 2 or 3 short lines; details can carry code.
const MAX_TOKENS: u32 = 3_000;
const MAX_EARLIER_ANSWER_CHARS: usize = 1_200;

#[derive(Clone, Debug, PartialEq)]
pub struct AnswerInput {
    /// The normalized question shown on the card.
    pub question: String,
    /// `None` for an early answer, started before the classifier decides: the model then
    /// picks the template and the language itself.
    pub intent: Option<Intent>,
    pub language: Option<Language>,
    /// The rolling summary of the lines that left `transcript`.
    pub summary: Option<String>,
    /// Latest transcript lines, oldest first.
    pub transcript: Vec<ContextLine>,
    /// The question this one follows up on, and the answer given to it.
    pub earlier: Option<(String, String)>,
}

pub(crate) fn sanitize(text: &str) -> String {
    text.replace(['<', '>'], " ")
}

fn tag<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Instructions, then the profile; the cache breakpoint covers both (they change only when
/// the profile is saved).
pub fn system_blocks(profile: Option<&Profile>) -> serde_json::Value {
    let profile_text = match profile.filter(|p| !p.is_empty()) {
        Some(p) => format!("<profile>\n{}\n</profile>", p.render()),
        None => {
            "<profile>\nNo verified profile: use placeholders for every personal fact.\n</profile>"
                .to_string()
        }
    };
    serde_json::json!([
        { "type": "text", "text": PROMPT },
        { "type": "text", "text": profile_text, "cache_control": { "type": "ephemeral" } }
    ])
}

/// One `[speaker] text` line per transcript line.
pub(crate) fn render_lines(lines: &[ContextLine], out: &mut String) {
    for line in lines {
        let who = match line.speaker {
            Speaker::Interviewer => "interviewer",
            Speaker::Candidate => "candidate",
        };
        out.push_str(&format!("[{who}] {}\n", sanitize(&line.text)));
    }
}

pub fn render_user_message(input: &AnswerInput) -> String {
    let mut out = String::new();
    if let Some(summary) = input.summary.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!(
            "<interview_summary>\n{}\n</interview_summary>\n",
            sanitize(summary.trim())
        ));
    }
    out.push_str("<transcript>\n");
    if input.transcript.is_empty() {
        out.push_str("(no transcript)\n");
    }
    render_lines(&input.transcript, &mut out);
    out.push_str("</transcript>\n");
    if let Some((question, answer)) = &input.earlier {
        let answer: String = answer.chars().take(MAX_EARLIER_ANSWER_CHARS).collect();
        out.push_str(&format!(
            "<earlier_question>{}</earlier_question>\n<earlier_answer>{}</earlier_answer>\n",
            sanitize(question),
            sanitize(&answer)
        ));
    }
    let mut attributes = String::new();
    if let Some(intent) = input.intent {
        attributes.push_str(&format!(" intent=\"{}\"", tag(intent)));
    }
    if let Some(language) = input.language {
        attributes.push_str(&format!(" language=\"{}\"", tag(language)));
    }
    out.push_str(&format!(
        "<question{attributes}>{}</question>",
        sanitize(&input.question)
    ));
    out
}

pub fn request_body(
    model: ClaudeModel,
    input: &AnswerInput,
    profile: Option<&Profile>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model.id(),
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "system": system_blocks(profile),
        "messages": [{ "role": "user", "content": render_user_message(input) }],
        "output_config": { "effort": "medium" }
    });
    model.apply_settings(&mut body);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> AnswerInput {
        AnswerInput {
            question: "Pourquoi as-tu choisi Pinia ?".into(),
            intent: Some(Intent::ClarificationOrFollowUp),
            language: Some(Language::Fr),
            summary: None,
            transcript: vec![
                ContextLine {
                    speaker: Speaker::Interviewer,
                    text: "Tu utilises quoi pour le <state> ?".into(),
                },
                ContextLine {
                    speaker: Speaker::Candidate,
                    text: "Pinia.".into(),
                },
            ],
            earlier: Some((
                "Quel state management utilises-tu ?".into(),
                "J'utilise Pinia.".into(),
            )),
        }
    }

    #[test]
    fn the_user_message_carries_context_follow_up_and_the_question() {
        let message = render_user_message(&input());
        assert_eq!(
            message,
            "<transcript>\n[interviewer] Tu utilises quoi pour le  state  ?\n[candidate] Pinia.\n</transcript>\n\
             <earlier_question>Quel state management utilises-tu ?</earlier_question>\n\
             <earlier_answer>J'utilise Pinia.</earlier_answer>\n\
             <question intent=\"CLARIFICATION_OR_FOLLOW_UP\" language=\"fr\">Pourquoi as-tu choisi Pinia ?</question>"
        );
        let mut plain = input();
        plain.earlier = None;
        plain.transcript.clear();
        let message = render_user_message(&plain);
        assert!(message.contains("(no transcript)"));
        assert!(!message.contains("earlier"));
        assert!(!message.contains("interview_summary"));
    }

    #[test]
    fn an_early_answer_leaves_intent_and_language_to_the_model_and_gets_the_summary() {
        let mut early = input();
        early.intent = None;
        early.language = None;
        early.summary = Some("On a parlé de Vue 3.".into());
        let message = render_user_message(&early);
        assert!(message.starts_with(
            "<interview_summary>\nOn a parlé de Vue 3.\n</interview_summary>\n<transcript>"
        ));
        assert!(message.contains("<question>Pourquoi as-tu choisi Pinia ?</question>"));
    }

    #[test]
    fn the_profile_is_part_of_the_cached_system_prefix() {
        let profile = Profile {
            name: "Alex Martin".into(),
            ..Profile::default()
        };
        let blocks = system_blocks(Some(&profile));
        assert_eq!(blocks[0]["text"], PROMPT);
        assert!(
            blocks[1]["text"]
                .as_str()
                .unwrap()
                .contains("## Name\nAlex Martin")
        );
        assert_eq!(blocks[1]["cache_control"]["type"], "ephemeral");
        let none = system_blocks(None);
        assert!(
            none[1]["text"]
                .as_str()
                .unwrap()
                .contains("No verified profile")
        );
        assert!(PROMPT.contains(DETAILS_MARKER));
    }

    #[test]
    fn sonnet_requests_stream_without_thinking_or_sampling() {
        let body = request_body(ClaudeModel::Sonnet55, &input(), None);
        assert_eq!(body["model"], "claude-sonnet-5-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["thinking"]["type"], "between_tools");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert!(body.get("temperature").is_none());
    }
}
