//! Answer generation (Phase 5, PRD §5): streamed answers from Claude for every detected or
//! typed question, written into the question's card as `answer.*` events, with the verified
//! profile as the only source of personal facts. See docs/ANSWER_ENGINE.md.

pub mod commands;
pub mod engine;
pub mod prompt;
pub mod sections;
pub mod summary;
