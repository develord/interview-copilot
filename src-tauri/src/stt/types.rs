//! Serializable transcription contracts shared with the frontend (`src/domain/stt.ts` and the
//! `transcript.*` schemas in `src/domain/events.ts`).

use serde::{Deserialize, Serialize};

use crate::audio::types::SourceKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    Assemblyai,
    Soniox,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 2] = [Self::Assemblyai, Self::Soniox];

    pub fn id(self) -> &'static str {
        match self {
            Self::Assemblyai => "assemblyai",
            Self::Soniox => "soniox",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionConfig {
    pub provider: ProviderKind,
    /// Terms from the job description or stack, merged with the built-in technical vocabulary.
    #[serde(default)]
    pub extra_terms: Vec<String>,
    /// Silence (ms) after speech before the app forces the provider to close the turn.
    /// `None` leaves end-of-turn detection entirely to the provider.
    #[serde(default)]
    pub force_endpoint_ms: Option<u32>,
    /// Also bias recognition with the built-in technical vocabulary.
    #[serde(default = "enabled")]
    pub builtin_vocabulary: bool,
}

fn enabled() -> bool {
    true
}

/// Mirrors `transcriptSegmentSchema` in `src/domain/events.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    /// Stable across partial updates of the same turn: `{source}-{connection}-{turn}`.
    pub id: String,
    pub session_id: String,
    pub source_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub normalized_text: Option<String>,
    pub is_final: bool,
    pub confidence: Option<f32>,
    pub speaker_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SttState {
    Connecting,
    Streaming,
    Reconnecting,
    Error,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SttErrorCode {
    /// No API key stored for the selected provider.
    MissingKey,
    Unauthorized,
    /// Balance or budget exhausted.
    Quota,
    /// Connection lost, timeout, rate limit — retried.
    Network,
    /// Malformed request or response — a bug or an API change; not retried.
    Protocol,
    /// Server-side failure — retried.
    Provider,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SttError {
    pub code: SttErrorCode,
    pub detail: String,
}

impl SttError {
    pub fn new(code: SttErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(self.code, SttErrorCode::Network | SttErrorCode::Provider)
    }
}

impl std::fmt::Display for SttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SttStatus {
    pub source: SourceKind,
    pub provider: ProviderKind,
    pub state: SttState,
    pub error: Option<SttError>,
    /// Time from local end-of-speech to the provider's final transcript, for the last turn.
    pub last_final_latency_ms: Option<u32>,
    /// Audio chunks dropped (provider too slow or disconnected). Never replayed.
    pub dropped_chunks: u64,
}
