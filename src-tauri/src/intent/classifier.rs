//! Second stage of question detection: a model classifies the utterance with its context and
//! returns structured JSON (PRD §4). This module owns the provider-independent part: the
//! request, the prompt, the JSON schema, and the runtime validation of the answer. Invalid
//! output is an error the detector recovers from; it never crashes the session.

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};

use super::rules::DOMAINS;
use super::types::{ClassifyError, ClassifyErrorCode, Intent, Language};

/// Versioned prompt (PRD §5 "versioned files"). Bump the file name when its meaning changes.
/// v1 and v2 quoted phrases too close to the evaluation corpus; v3 is a third shorter (less
/// time before the first token) and shares no more than two consecutive words with it.
pub const PROMPT: &str = include_str!("../../prompts/classification.v4.md");
pub const PROMPT_VERSION: &str = "classification.v4";

const MAX_QUESTION_CHARS: usize = 4_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    Interviewer,
    Candidate,
}

impl Speaker {
    fn tag(self) -> &'static str {
        match self {
            Self::Interviewer => "interviewer",
            Self::Candidate => "candidate",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContextLine {
    pub speaker: Speaker,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecentQuestion {
    pub id: String,
    pub text: String,
    pub intent: Intent,
}

/// Everything the model sees. Bounded by the detector (PRD §5: never the whole transcript).
#[derive(Clone, Debug, PartialEq)]
pub struct ClassifyRequest {
    pub utterance: String,
    pub speaker: Speaker,
    pub transcript: Vec<ContextLine>,
    /// Oldest first; shown to the model as Q1, Q2…
    pub recent_questions: Vec<RecentQuestion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    New,
    FollowUp,
    Continuation,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    High,
    Normal,
    Low,
}

/// Where the time of one model call went (streaming adapters only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTiming {
    /// Request sent → first output token.
    pub first_token_ms: Option<u32>,
    /// Request sent → every field of `DECISIVE_FIELDS` complete.
    pub decision_ms: Option<u32>,
    /// Request sent → end of the response.
    pub total_ms: u32,
    pub input_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
    pub output_tokens: u32,
}

/// The fields that decide whether a card is created and how it is linked. The schema lists
/// them first, so they are generated before the (longer) normalized question.
pub const DECISIVE_FIELDS: [&str; 5] = [
    "requiresResponse",
    "intent",
    "relation",
    "contextReference",
    "language",
];

/// A validated classification. `context_ref` is a real question id, never a prompt alias.
#[derive(Clone, Debug, PartialEq)]
pub struct Classification {
    pub question: String,
    pub language: Language,
    pub intent: Intent,
    pub requires_response: bool,
    pub relation: Relation,
    pub context_ref: Option<String>,
    pub domain: Option<&'static str>,
    /// Not used yet: answer prioritization arrives with the generation engine (Phase 5).
    pub urgency: Urgency,
    pub confidence: f32,
    pub timing: Option<CallTiming>,
}

pub trait Classifier: Send + Sync + 'static {
    fn model(&self) -> &'static str;

    fn classify<'a>(
        &'a self,
        request: &'a ClassifyRequest,
    ) -> BoxFuture<'a, Result<Classification, ClassifyError>>;
}

/// Speech-recognition text cannot close the prompt's tags.
fn sanitize(text: &str) -> String {
    text.replace(['<', '>'], " ")
}

pub fn render_user_message(request: &ClassifyRequest) -> String {
    let mut out = String::from("<recent_questions>\n");
    if request.recent_questions.is_empty() {
        out.push_str("none\n");
    }
    for (i, q) in request.recent_questions.iter().enumerate() {
        let intent = serde_json::to_value(q.intent)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        out.push_str(&format!("Q{} ({intent}): {}\n", i + 1, sanitize(&q.text)));
    }
    out.push_str("</recent_questions>\n<transcript>\n");
    if request.transcript.is_empty() {
        out.push_str("(start of the interview)\n");
    }
    for line in &request.transcript {
        out.push_str(&format!(
            "[{}] {}\n",
            line.speaker.tag(),
            sanitize(&line.text)
        ));
    }
    out.push_str(&format!(
        "</transcript>\n<utterance speaker=\"{}\">{}</utterance>",
        request.speaker.tag(),
        sanitize(&request.utterance)
    ));
    out
}

/// JSON schema for structured outputs. Numeric bounds are not supported there; `confidence`
/// is clamped when validating instead. Property order is generation order: decisive fields
/// first (needs serde_json's `preserve_order`; without it the keys would be sorted).
pub fn output_schema() -> serde_json::Value {
    let intents: Vec<serde_json::Value> = Intent::ALL
        .iter()
        .map(|i| serde_json::to_value(i).expect("intent serializes"))
        .collect();
    serde_json::json!({
        "type": "object",
        "properties": {
            "requiresResponse": { "type": "boolean" },
            "intent": { "type": "string", "enum": intents },
            "relation": {
                "type": "string",
                "enum": ["new", "follow_up", "continuation", "repeat"]
            },
            "contextReference": { "anyOf": [{ "type": "string" }, { "type": "null" }] },
            "language": { "type": "string", "enum": ["fr", "en", "mixed"] },
            "question": { "type": "string" },
            "domain": {
                "anyOf": [{ "type": "string", "enum": DOMAINS }, { "type": "null" }]
            },
            "urgency": { "type": "string", "enum": ["high", "normal", "low"] },
            "confidence": { "type": "number" }
        },
        "required": [
            "question", "language", "requiresResponse", "intent", "relation",
            "contextReference", "domain", "urgency", "confidence"
        ],
        "additionalProperties": false
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawClassification {
    question: String,
    language: Language,
    intent: Intent,
    requires_response: bool,
    relation: Relation,
    context_reference: Option<String>,
    domain: Option<String>,
    urgency: Urgency,
    confidence: f64,
}

/// Validates the model's JSON against the request it answers.
pub fn parse_classification(
    json: &str,
    request: &ClassifyRequest,
) -> Result<Classification, ClassifyError> {
    let raw: RawClassification = serde_json::from_str(json.trim()).map_err(|e| {
        ClassifyError::new(
            ClassifyErrorCode::InvalidResponse,
            format!("classification JSON: {e}"),
        )
    })?;

    let context_ref = raw.context_reference.as_deref().and_then(|alias| {
        let index: usize = alias.trim().strip_prefix('Q')?.parse().ok()?;
        request
            .recent_questions
            .get(index.checked_sub(1)?)
            .map(|q| q.id.clone())
    });
    // A relation that needs a reference falls back to "new" without a valid one.
    let relation = match (raw.relation, &context_ref) {
        (Relation::New, _) | (_, None) => Relation::New,
        (relation, Some(_)) => relation,
    };
    let context_ref = if relation == Relation::New {
        None
    } else {
        context_ref
    };

    let question: String = raw
        .question
        .trim()
        .chars()
        .take(MAX_QUESTION_CHARS)
        .collect();
    let question = if question.is_empty() {
        request
            .utterance
            .trim()
            .chars()
            .take(MAX_QUESTION_CHARS)
            .collect()
    } else {
        question
    };
    let confidence = if raw.confidence.is_finite() {
        raw.confidence.clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    Ok(Classification {
        question,
        language: raw.language,
        intent: raw.intent,
        requires_response: raw.requires_response,
        relation,
        context_ref,
        domain: raw
            .domain
            .and_then(|d| DOMAINS.iter().copied().find(|known| *known == d)),
        urgency: raw.urgency,
        confidence,
        timing: None,
    })
}

/// The top-level fields of a JSON object whose values are complete in `prefix`, the text
/// streamed so far. A number at the very end may still grow, so it counts only once a `,` or
/// `}` follows it. Never fails: an unexpected character just ends the scan.
pub fn complete_fields(prefix: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut fields = serde_json::Map::new();
    let b = prefix.as_bytes();
    let mut i = skip_ws(b, 0);
    if b.get(i) != Some(&b'{') {
        return fields;
    }
    i += 1;
    loop {
        i = skip_ws(b, i);
        if b.get(i) == Some(&b',') {
            i = skip_ws(b, i + 1);
        }
        if b.get(i) != Some(&b'"') {
            return fields;
        }
        let Some(key_end) = string_end(b, i) else {
            return fields;
        };
        let Ok(key) = serde_json::from_str::<String>(&prefix[i..key_end]) else {
            return fields;
        };
        i = skip_ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return fields;
        }
        i = skip_ws(b, i + 1);
        let Some(value_end) = value_end(b, i) else {
            return fields;
        };
        let Ok(value) = serde_json::from_str(&prefix[i..value_end]) else {
            return fields;
        };
        fields.insert(key, value);
        i = value_end;
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

/// `b[start]` is an opening quote; returns the index just after the closing one.
fn string_end(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn value_end(b: &[u8], start: usize) -> Option<usize> {
    match *b.get(start)? {
        b'"' => string_end(b, start),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut i = start;
            while i < b.len() {
                match b[i] {
                    b'"' => {
                        i = string_end(b, i)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
        _ => {
            let end = (start..b.len())
                .find(|&i| matches!(b[i], b',' | b'}') || b[i].is_ascii_whitespace())?;
            (end > start).then_some(end)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ClassifyRequest {
        ClassifyRequest {
            utterance: "Et en termes de complexité ?".into(),
            speaker: Speaker::Interviewer,
            transcript: vec![
                ContextLine {
                    speaker: Speaker::Interviewer,
                    text: "Comment trierais-tu ce tableau <vite> ?".into(),
                },
                ContextLine {
                    speaker: Speaker::Candidate,
                    text: "Avec un tri fusion.".into(),
                },
            ],
            recent_questions: vec![
                RecentQuestion {
                    id: "session-1-q1".into(),
                    text: "Bonjour !".into(),
                    intent: Intent::GreetingOrSmallTalk,
                },
                RecentQuestion {
                    id: "session-1-q2".into(),
                    text: "Comment trierais-tu ce tableau ?".into(),
                    intent: Intent::CodingProblem,
                },
            ],
        }
    }

    fn json(relation: &str, reference: &str) -> String {
        format!(
            r#"{{"question":"Quelle est la complexité du tri fusion ?","language":"fr",
            "intent":"CLARIFICATION_OR_FOLLOW_UP","requiresResponse":true,"relation":"{relation}",
            "contextReference":{reference},"domain":"algorithms","urgency":"high","confidence":0.92}}"#
        )
    }

    #[test]
    fn the_user_message_lists_aliases_speakers_and_the_utterance() {
        let message = render_user_message(&request());
        assert_eq!(
            message,
            "<recent_questions>\n\
             Q1 (GREETING_OR_SMALL_TALK): Bonjour !\n\
             Q2 (CODING_PROBLEM): Comment trierais-tu ce tableau ?\n\
             </recent_questions>\n<transcript>\n\
             [interviewer] Comment trierais-tu ce tableau  vite  ?\n\
             [candidate] Avec un tri fusion.\n\
             </transcript>\n<utterance speaker=\"interviewer\">Et en termes de complexité ?</utterance>"
        );
    }

    #[test]
    fn an_empty_context_is_explicit() {
        let mut empty = request();
        empty.transcript.clear();
        empty.recent_questions.clear();
        let message = render_user_message(&empty);
        assert!(message.contains("<recent_questions>\nnone\n"));
        assert!(message.contains("(start of the interview)"));
    }

    #[test]
    fn a_valid_answer_is_parsed_and_its_alias_resolved() {
        let c = parse_classification(&json("follow_up", "\"Q2\""), &request()).unwrap();
        assert_eq!(c.intent, Intent::ClarificationOrFollowUp);
        assert!(c.requires_response);
        assert_eq!(c.relation, Relation::FollowUp);
        assert_eq!(c.context_ref.as_deref(), Some("session-1-q2"));
        assert_eq!(c.domain, Some("algorithms"));
        assert!((c.confidence - 0.92).abs() < 1e-6);
    }

    #[test]
    fn unknown_references_demote_the_relation_to_new() {
        for reference in ["\"Q9\"", "\"Q0\"", "\"session-1-q2\"", "null"] {
            let c = parse_classification(&json("continuation", reference), &request()).unwrap();
            assert_eq!(c.relation, Relation::New, "{reference}");
            assert_eq!(c.context_ref, None, "{reference}");
        }
        // "new" never keeps a reference.
        let c = parse_classification(&json("new", "\"Q2\""), &request()).unwrap();
        assert_eq!(c.context_ref, None);
    }

    #[test]
    fn invalid_output_is_an_error_not_a_panic() {
        for bad in [
            "",
            "not json",
            r#"{"question":"x"}"#,
            &json("follow_up", "\"Q2\"").replace("CLARIFICATION_OR_FOLLOW_UP", "QUESTION"),
            &json("follow_up", "\"Q2\"").replace("\"fr\"", "\"de\""),
            &json("follow_up", "\"Q2\"").replace("}", ",\"extra\":1}"),
        ] {
            let error = parse_classification(bad, &request()).unwrap_err();
            assert_eq!(error.code, ClassifyErrorCode::InvalidResponse, "{bad}");
        }
    }

    #[test]
    fn out_of_range_values_are_repaired() {
        let text = json("follow_up", "\"Q2\"")
            .replace("0.92", "7.5")
            .replace("\"algorithms\"", "\"cooking\"")
            .replace("Quelle est la complexité du tri fusion ?", "   ");
        let c = parse_classification(&text, &request()).unwrap();
        assert_eq!(c.confidence, 1.0);
        assert_eq!(c.domain, None);
        assert_eq!(c.question, "Et en termes de complexité ?");
    }

    #[test]
    fn the_schema_lists_every_intent_and_domain() {
        let schema = output_schema();
        assert_eq!(
            schema["properties"]["intent"]["enum"]
                .as_array()
                .unwrap()
                .len(),
            Intent::ALL.len()
        );
        assert_eq!(
            schema["properties"]["domain"]["anyOf"][0]["enum"]
                .as_array()
                .unwrap()
                .len(),
            DOMAINS.len()
        );
        assert_eq!(schema["additionalProperties"], false);
        let required = schema["required"].as_array().unwrap().len();
        assert_eq!(required, schema["properties"].as_object().unwrap().len());
    }

    #[test]
    fn partial_json_yields_only_the_complete_fields() {
        let full = r#"{"requiresResponse": true, "intent":"TECHNICAL_QUESTION","relation":"follow_up","contextReference":null,"language":"fr","question":"Il a dit \"ok\", puis { ?","domain":null,"urgency":"high","confidence":0.92}"#;
        let mut seen = 0;
        for end in 0..=full.len() {
            let fields = complete_fields(&full[..end]);
            assert!(fields.len() >= seen, "fields never disappear ({end})");
            seen = fields.len();
        }
        let all = complete_fields(full);
        assert_eq!(all.len(), 9);
        assert_eq!(all["question"], "Il a dit \"ok\", puis { ?");
        // A number at the very end may still grow.
        assert!(!complete_fields(&full[..full.len() - 1]).contains_key("confidence"));
        // The decisive fields are complete before the question starts.
        let early = complete_fields(&full[..full.find("\"question\"").unwrap()]);
        assert!(DECISIVE_FIELDS.iter().all(|k| early.contains_key(*k)));
        for garbage in ["", "[", "{\"a\"", "{\"a\":", "{1:2}", "not json"] {
            assert!(complete_fields(garbage).is_empty(), "{garbage}");
        }
    }

    #[test]
    fn the_schema_lists_the_decisive_fields_first() {
        let schema = output_schema();
        let order: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(order[..DECISIVE_FIELDS.len()], DECISIVE_FIELDS);
        // The serialized request keeps that order (serde_json `preserve_order`).
        let text = serde_json::to_string(&schema).unwrap();
        assert!(text.find("requiresResponse").unwrap() < text.find("question").unwrap());
    }

    #[test]
    fn the_prompt_is_versioned_and_long_enough_to_cache() {
        assert!(PROMPT.contains("requiresResponse"));
        // The 512-token minimum cacheable prefix is about 2 000 characters.
        assert!(PROMPT.len() > 3_000, "{} chars", PROMPT.len());
    }
}
