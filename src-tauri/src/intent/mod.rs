//! Question and intent detection (Phase 4, PRD §4). Final transcripts are assembled into
//! utterances, filtered and partly decided by deterministic rules, then classified by Claude
//! with structured JSON output. The result is `question.detected` / `question.revised` events
//! for the card lifecycle, plus `utterance.classified` diagnostics. See
//! docs/QUESTION_DETECTION.md.

pub mod assembler;
pub mod classifier;
pub mod claude;
pub mod commands;
pub mod detector;
pub mod engine;
pub mod eval;
pub mod rules;
pub mod text;
pub mod types;
