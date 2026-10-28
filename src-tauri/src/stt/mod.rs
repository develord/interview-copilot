//! Streaming speech-to-text (Phase 3). Provider-agnostic: a `Codec` per provider translates
//! its WebSocket protocol into normalized events; `session` adds pacing, VAD, reconnection and
//! stable transcript ids. See docs/TRANSCRIPTION.md.

pub mod assemblyai;
pub mod commands;
pub mod credentials;
pub mod eval;
pub mod pacing;
pub mod provider;
pub mod session;
pub mod soniox;
pub mod types;
pub mod vad;
pub mod vocabulary;
mod ws;

use std::sync::Arc;

use provider::{ProviderSettings, SttProvider};
use types::{ProviderKind, TranscriptionConfig};

/// French is the main language; English appears inside French sentences (code-switching).
pub const LANGUAGE_HINTS: [&str; 2] = ["fr", "en"];
pub const INTERVIEW_CONTEXT: &str = "Entretien d'embauche technique en français. Les termes techniques \
     anglais (frameworks, outils, concepts) sont fréquents au milieu des phrases.";

/// Bias-term budget per provider (AssemblyAI: 100 keyterms; Soniox: 8 000-token context).
fn term_limit(provider: ProviderKind) -> usize {
    match provider {
        ProviderKind::Assemblyai => 100,
        ProviderKind::Soniox => 300,
    }
}

pub fn eu_endpoint(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::Assemblyai => assemblyai::EU_ENDPOINT,
        ProviderKind::Soniox => soniox::EU_ENDPOINT,
    }
}

pub fn build_provider(
    config: &TranscriptionConfig,
    api_key: String,
    endpoint: Option<String>,
) -> Arc<dyn SttProvider> {
    let settings = ProviderSettings {
        endpoint: endpoint.unwrap_or_else(|| eu_endpoint(config.provider).to_string()),
        api_key,
        terms: vocabulary::bias_terms(
            &config.extra_terms,
            config.builtin_vocabulary,
            term_limit(config.provider),
        ),
        context: INTERVIEW_CONTEXT.to_string(),
        language_hints: LANGUAGE_HINTS.iter().map(|l| l.to_string()).collect(),
    };
    match config.provider {
        ProviderKind::Assemblyai => Arc::new(assemblyai::AssemblyAi { settings }),
        ProviderKind::Soniox => Arc::new(soniox::Soniox { settings }),
    }
}
