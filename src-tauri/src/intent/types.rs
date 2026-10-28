//! Serializable question-detection contracts shared with the frontend
//! (`src/domain/types.ts`, `src/domain/intent.ts` and `detectedQuestionSchema` in
//! `src/domain/events.ts`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Intent {
    TechnicalQuestion,
    CodingProblem,
    SystemDesign,
    ExperienceQuestion,
    BehavioralQuestion,
    GreetingOrSmallTalk,
    ClarificationOrFollowUp,
    NonQuestionStatement,
    Ambiguous,
}

impl Intent {
    pub const ALL: [Intent; 9] = [
        Self::TechnicalQuestion,
        Self::CodingProblem,
        Self::SystemDesign,
        Self::ExperienceQuestion,
        Self::BehavioralQuestion,
        Self::GreetingOrSmallTalk,
        Self::ClarificationOrFollowUp,
        Self::NonQuestionStatement,
        Self::Ambiguous,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Fr,
    En,
    Mixed,
}

/// Claude models offered for classification. Haiku is the default: on the labeled corpus it
/// passes every quality metric at a fraction of Opus's latency and cost
/// (see docs/QUESTION_DETECTION.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClaudeModel {
    #[serde(rename = "claude-opus-5-5")]
    Opus55,
    #[serde(rename = "claude-haiku-5-5")]
    Haiku55,
}

impl ClaudeModel {
    pub fn id(self) -> &'static str {
        match self {
            Self::Opus55 => "claude-opus-5-5",
            Self::Haiku55 => "claude-haiku-5-5",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionDetectionConfig {
    pub model: ClaudeModel,
    /// Also detect questions in microphone speech (testing without an interviewer). Off by
    /// default: the microphone is the candidate, whose speech is context only.
    #[serde(default)]
    pub include_microphone: bool,
}

/// Mirrors `detectedQuestionSchema` in `src/domain/events.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedQuestion {
    pub id: String,
    pub session_id: String,
    /// Always `"speech"` here; manual questions are created by the overlay.
    pub source: &'static str,
    pub source_segment_ids: Vec<String>,
    pub original_utterance: String,
    pub normalized_question: String,
    pub intent: Intent,
    pub language: Language,
    pub domain: Option<String>,
    /// Id of the earlier question this one follows up on.
    pub context_ref: Option<String>,
    /// Unix time in milliseconds.
    pub detected_at: u64,
}

/// What became of one assembled utterance, for the diagnostics transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decision {
    /// A new answer card was created.
    Question,
    /// The utterance completed or corrected an existing question (`question.revised`).
    Revision,
    Ignored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IgnoreReason {
    /// Only hesitations ("euh", "hum").
    Filler,
    /// Acknowledgements such as "d'accord", "ok", "très bien".
    Backchannel,
    /// A statement that expects no answer.
    Statement,
    /// Not enough context yet; kept as context for the next utterance.
    Ambiguous,
    /// The same question was asked again.
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// Decided by the deterministic first stage.
    Rules,
    /// Decided by the model.
    Llm,
    /// The model was unavailable; decided by the first stage alone.
    Fallback,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UtteranceReport {
    pub id: String,
    pub source_id: String,
    pub segment_ids: Vec<String>,
    pub text: String,
    pub decision: Decision,
    pub reason: Option<IgnoreReason>,
    pub intent: Option<Intent>,
    pub stage: Stage,
    pub question_id: Option<String>,
    /// Model round-trip time, when the model was called.
    pub latency_ms: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClassifierState {
    /// Rules + model.
    Ready,
    /// The model is not configured (no key): explicit questions and greetings only.
    RulesOnly,
    /// The model failed on the last call; the rules decided instead.
    Degraded,
    /// The model refused the key or the account; rules only until the next session.
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClassifyErrorCode {
    MissingKey,
    Unauthorized,
    /// Billing problem or exhausted credit.
    Quota,
    RateLimited,
    /// 5xx / 529 — retried once.
    Overloaded,
    Network,
    Timeout,
    /// The response did not match the expected JSON.
    InvalidResponse,
    /// The model declined to answer (`stop_reason: "refusal"`).
    Refused,
    /// Malformed request — a bug or an API change.
    Protocol,
    /// The OS credential store failed (saving or reading the key).
    CredentialStore,
}

impl ClassifyErrorCode {
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::Overloaded | Self::Network | Self::Timeout
        )
    }

    /// Errors that will not go away by themselves during this session.
    pub fn is_fatal(self) -> bool {
        matches!(
            self,
            Self::MissingKey | Self::Unauthorized | Self::Quota | Self::CredentialStore
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClassifyError {
    pub code: ClassifyErrorCode,
    pub detail: String,
}

impl ClassifyError {
    pub fn new(code: ClassifyErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for ClassifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierStatus {
    pub state: ClassifierState,
    pub model: Option<&'static str>,
    pub error: Option<ClassifyError>,
    pub last_latency_ms: Option<u32>,
    /// Model calls that returned a valid classification.
    pub classified: u64,
    /// Model calls that failed (the rules decided instead).
    pub failed: u64,
}
