//! Provider API keys live in the Windows Credential Manager (PRD §11), are read only by the Rust
//! process, and are never returned to the webview.

use super::types::{ProviderKind, SttError, SttErrorCode};

const SERVICE: &str = "dev.interviewcopilot.desktop/stt";
const MAX_KEY_CHARS: usize = 512;

fn entry(provider: ProviderKind) -> Result<keyring::Entry, SttError> {
    keyring::Entry::new(SERVICE, provider.id()).map_err(|e| {
        SttError::new(
            SttErrorCode::Provider,
            format!("credential store unavailable: {e}"),
        )
    })
}

pub fn get(provider: ProviderKind) -> Result<Option<String>, SttError> {
    match entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SttError::new(
            SttErrorCode::Provider,
            format!("cannot read the key: {e}"),
        )),
    }
}

pub fn set(provider: ProviderKind, key: &str) -> Result<(), SttError> {
    let key = validate(key)?;
    let result = entry(provider)?.set_password(key);
    result
        .map_err(|e| SttError::new(SttErrorCode::Provider, format!("cannot store the key: {e}")))?;
    // Read back through a fresh entry, so a write that did not persist is caught immediately.
    match get(provider)? {
        Some(stored) if stored == key => Ok(()),
        _ => Err(SttError::new(
            SttErrorCode::Provider,
            "the key was not persisted by the credential store",
        )),
    }
}

pub fn delete(provider: ProviderKind) -> Result<(), SttError> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SttError::new(
            SttErrorCode::Provider,
            format!("cannot delete the key: {e}"),
        )),
    }
}

/// Rejects keys that would break an HTTP header or are obviously not keys.
pub fn validate(key: &str) -> Result<&str, SttError> {
    let key = key.trim();
    let valid = !key.is_empty()
        && key.chars().count() <= MAX_KEY_CHARS
        && key.chars().all(|c| c.is_ascii_graphic());
    if valid {
        Ok(key)
    } else {
        Err(SttError::new(
            SttErrorCode::Unauthorized,
            "this does not look like an API key",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_plausible_keys() {
        assert_eq!(validate("  sk_live_ABC123-xyz  "), Ok("sk_live_ABC123-xyz"));
    }

    #[test]
    fn rejects_values_that_would_break_a_header() {
        for bad in ["", "   ", "two words", "line\nbreak", "é-accent"] {
            assert!(validate(bad).is_err(), "{bad:?} accepted");
        }
        assert!(validate(&"x".repeat(MAX_KEY_CHARS + 1)).is_err());
    }
}

#[cfg(test)]
mod store_tests {
    /// Touches the real Windows Credential Manager (a throwaway entry, deleted at the end).
    #[test]
    #[ignore = "writes to the OS credential store"]
    fn round_trips_through_the_os_credential_store() {
        let entry =
            keyring::Entry::new("dev.interviewcopilot.desktop/stt-selftest", "probe").unwrap();
        entry.set_password("probe-value-123").unwrap();
        assert_eq!(entry.get_password().unwrap(), "probe-value-123");
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
    }
}
