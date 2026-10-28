//! Speech-to-text provider abstraction (PRD §3.2 `SpeechRecognitionProvider`).

use futures_util::future::BoxFuture;
use tokio::sync::mpsc;

use super::types::{ProviderKind, SttError};

/// What the session sends to a provider connection.
#[derive(Debug, PartialEq, Eq)]
pub enum AudioMsg {
    /// 16 kHz mono PCM16 little-endian, ~80 ms.
    Chunk(Vec<u8>),
    /// Local VAD saw the end of speech: ask the provider to close the current turn now.
    ForceEndpoint,
}

/// Normalized provider output. Turn numbers restart at 0 for every connection.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderEvent {
    /// The provider accepted a new connection (opens a new id namespace for turns).
    Ready,
    /// Current text of the turn being spoken; replaces the previous partial of that turn.
    Partial {
        turn: u64,
        text: String,
        start_ms: u64,
        end_ms: u64,
    },
    /// The turn is complete and its text will not change.
    Final {
        turn: u64,
        text: String,
        start_ms: u64,
        end_ms: u64,
        confidence: Option<f32>,
        language: Option<String>,
    },
}

pub type EventSender = mpsc::UnboundedSender<ProviderEvent>;

pub trait SttProvider: Send + Sync + 'static {
    fn kind(&self) -> ProviderKind;

    /// Runs one connection, streaming `audio` until the channel closes (then ends the session
    /// gracefully and returns `Ok`) or the connection fails (returns the classified error).
    fn stream<'a>(
        &'a self,
        audio: &'a mut mpsc::Receiver<AudioMsg>,
        events: &'a EventSender,
    ) -> BoxFuture<'a, Result<(), SttError>>;
}

/// Provider-independent tuning shared by every adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSettings {
    /// WebSocket endpoint (EU region by default; overridable for tests).
    pub endpoint: String,
    pub api_key: String,
    /// Bias terms, already deduplicated and capped by the caller.
    pub terms: Vec<String>,
    /// Short free-text description of the conversation.
    pub context: String,
    pub language_hints: Vec<String>,
}
