//! Tauri commands for provider keys. The webview can store, test for and delete a key; it can
//! never read one back.

use serde::Serialize;

use super::credentials;
use super::types::{ProviderKind, SttError};

#[derive(Debug, Serialize)]
pub struct KeyStatus {
    pub assemblyai: bool,
    pub soniox: bool,
}

#[tauri::command]
pub async fn stt_key_status() -> Result<KeyStatus, SttError> {
    Ok(KeyStatus {
        assemblyai: credentials::get(ProviderKind::Assemblyai)?.is_some(),
        soniox: credentials::get(ProviderKind::Soniox)?.is_some(),
    })
}

#[tauri::command]
pub async fn stt_set_key(provider: ProviderKind, key: String) -> Result<(), SttError> {
    credentials::set(provider, &key)
}

#[tauri::command]
pub async fn stt_clear_key(provider: ProviderKind) -> Result<(), SttError> {
    credentials::delete(provider)
}
