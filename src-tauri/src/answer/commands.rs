//! Tauri commands for answers. Results arrive as `answer.*` pipeline events, not as return
//! values.

use std::sync::Arc;

use tauri::State;

use super::engine::AnswerEngine;
use crate::llm::{ClaudeModel, LlmError};

#[tauri::command]
pub async fn answer_configure(
    engine: State<'_, Arc<AnswerEngine>>,
    model: ClaudeModel,
    early_start: Option<bool>,
) -> Result<(), LlmError> {
    engine.configure(model, early_start.unwrap_or(true));
    Ok(())
}

/// Returns the new question's id; `question.detected` and the answer follow as events.
#[tauri::command]
pub async fn answer_ask_manual(
    engine: State<'_, Arc<AnswerEngine>>,
    text: String,
) -> Result<String, LlmError> {
    engine.ask_manual(&text)
}

#[tauri::command]
pub async fn answer_retry(
    engine: State<'_, Arc<AnswerEngine>>,
    question_id: String,
) -> Result<(), LlmError> {
    engine.retry(&question_id);
    Ok(())
}

#[tauri::command]
pub async fn answer_cancel_all(engine: State<'_, Arc<AnswerEngine>>) -> Result<(), LlmError> {
    engine.cancel_all();
    Ok(())
}
