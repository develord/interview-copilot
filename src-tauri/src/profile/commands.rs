//! Tauri commands for the verified profile.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use super::{Profile, ProfileError, ProfileErrorCode, ProfileStore, extract};
use crate::llm::credentials;

#[tauri::command]
pub async fn profile_get(
    store: State<'_, Arc<ProfileStore>>,
) -> Result<Option<Profile>, ProfileError> {
    Ok(store.get())
}

#[tauri::command]
pub async fn profile_save(
    store: State<'_, Arc<ProfileStore>>,
    profile: Profile,
) -> Result<Profile, ProfileError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    store.save(profile, now)
}

#[tauri::command]
pub async fn profile_clear(store: State<'_, Arc<ProfileStore>>) -> Result<(), ProfileError> {
    store.clear()
}

/// Returns an unverified draft; nothing is saved until `profile_save`.
#[tauri::command]
pub async fn profile_extract_pdf(pdf_base64: String) -> Result<Profile, ProfileError> {
    extract::validate_pdf(&pdf_base64)?;
    let key = credentials::get()
        .map_err(|e| ProfileError::new(ProfileErrorCode::Extraction, e.detail))?
        .ok_or_else(|| {
            ProfileError::new(ProfileErrorCode::MissingKey, "no Claude API key stored")
        })?;
    extract::extract(key, &pdf_base64)
        .await
        .map_err(|e| ProfileError::new(ProfileErrorCode::Extraction, e.to_string()))
}
