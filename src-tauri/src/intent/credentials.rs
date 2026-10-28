//! The Claude API key in the OS credential store (see `crate::secrets`).

use super::types::{ClassifyError, ClassifyErrorCode};
use crate::secrets::{self, SecretError};

const SERVICE: &str = "dev.interviewcopilot.desktop/llm";
const ACCOUNT: &str = "anthropic";

fn map(error: SecretError) -> ClassifyError {
    match error {
        SecretError::Invalid => ClassifyError::new(
            ClassifyErrorCode::Unauthorized,
            "this does not look like an API key",
        ),
        SecretError::Store(detail) => {
            ClassifyError::new(ClassifyErrorCode::CredentialStore, detail)
        }
    }
}

pub fn get() -> Result<Option<String>, ClassifyError> {
    secrets::get(SERVICE, ACCOUNT).map_err(map)
}

pub fn set(key: &str) -> Result<(), ClassifyError> {
    secrets::set(SERVICE, ACCOUNT, key).map_err(map)
}

pub fn delete() -> Result<(), ClassifyError> {
    secrets::delete(SERVICE, ACCOUNT).map_err(map)
}
