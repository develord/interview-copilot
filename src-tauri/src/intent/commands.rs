//! Tauri commands for the Claude key. The webview can store, test for and delete it; it can
//! never read it back.

use serde::Serialize;

use super::types::ClassifyError;
use crate::llm::credentials;

#[derive(Debug, Serialize)]
pub struct LlmKeyStatus {
    pub anthropic: bool,
}

#[tauri::command]
pub async fn llm_key_status() -> Result<LlmKeyStatus, ClassifyError> {
    Ok(LlmKeyStatus {
        anthropic: credentials::get()?.is_some(),
    })
}

#[tauri::command]
pub async fn llm_set_key(key: String) -> Result<(), ClassifyError> {
    credentials::set(&key)
}

#[tauri::command]
pub async fn llm_clear_key() -> Result<(), ClassifyError> {
    credentials::delete()
}
