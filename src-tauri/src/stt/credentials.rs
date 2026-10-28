//! Speech-to-text keys in the OS credential store (see `crate::secrets`).

use super::types::{ProviderKind, SttError, SttErrorCode};
use crate::secrets::{self, SecretError};

const SERVICE: &str = "dev.interviewcopilot.desktop/stt";

fn map(error: SecretError) -> SttError {
    match error {
        SecretError::Invalid => SttError::new(
            SttErrorCode::Unauthorized,
            "this does not look like an API key",
        ),
        SecretError::Store(detail) => SttError::new(SttErrorCode::Provider, detail),
    }
}

pub fn get(provider: ProviderKind) -> Result<Option<String>, SttError> {
    secrets::get(SERVICE, provider.id()).map_err(map)
}

pub fn set(provider: ProviderKind, key: &str) -> Result<(), SttError> {
    secrets::set(SERVICE, provider.id(), key).map_err(map)
}

pub fn delete(provider: ProviderKind) -> Result<(), SttError> {
    secrets::delete(SERVICE, provider.id()).map_err(map)
}
