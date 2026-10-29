//! The Claude API key in the OS credential store (see `crate::secrets`), shared by every
//! Claude feature.

use super::{LlmError, LlmErrorCode};
use crate::secrets::{self, SecretError};

const SERVICE: &str = "dev.interviewcopilot.desktop/llm";
const ACCOUNT: &str = "anthropic";

fn map(error: SecretError) -> LlmError {
    match error {
        SecretError::Invalid => LlmError::new(
            LlmErrorCode::Unauthorized,
            "this does not look like an API key",
        ),
        SecretError::Store(detail) => LlmError::new(LlmErrorCode::CredentialStore, detail),
    }
}

pub fn get() -> Result<Option<String>, LlmError> {
    secrets::get(SERVICE, ACCOUNT).map_err(map)
}

pub fn set(key: &str) -> Result<(), LlmError> {
    secrets::set(SERVICE, ACCOUNT, key).map_err(map)
}

pub fn delete() -> Result<(), LlmError> {
    secrets::delete(SERVICE, ACCOUNT).map_err(map)
}
