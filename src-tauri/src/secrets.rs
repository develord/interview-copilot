//! Provider API keys live in the Windows Credential Manager (PRD §11), are read only by the Rust
//! process, and are never returned to the webview. Each caller maps `SecretError` to its own
//! error contract (`stt::credentials`, `intent::credentials`).

const MAX_KEY_CHARS: usize = 512;

#[derive(Debug, PartialEq, Eq)]
pub enum SecretError {
    /// The value cannot be a key (empty, too long, or would break an HTTP header).
    Invalid,
    /// The OS credential store failed or did not keep the value.
    Store(String),
}

fn entry(service: &str, account: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(service, account)
        .map_err(|e| SecretError::Store(format!("credential store unavailable: {e}")))
}

pub fn get(service: &str, account: &str) -> Result<Option<String>, SecretError> {
    match entry(service, account)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretError::Store(format!("cannot read the key: {e}"))),
    }
}

pub fn set(service: &str, account: &str, key: &str) -> Result<(), SecretError> {
    let key = validate(key)?;
    entry(service, account)?
        .set_password(key)
        .map_err(|e| SecretError::Store(format!("cannot store the key: {e}")))?;
    // Read back through a fresh entry, so a write that did not persist is caught immediately.
    match get(service, account)? {
        Some(stored) if stored == key => Ok(()),
        _ => Err(SecretError::Store(
            "the key was not persisted by the credential store".into(),
        )),
    }
}

pub fn delete(service: &str, account: &str) -> Result<(), SecretError> {
    match entry(service, account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretError::Store(format!("cannot delete the key: {e}"))),
    }
}

/// Rejects keys that would break an HTTP header or are obviously not keys.
pub fn validate(key: &str) -> Result<&str, SecretError> {
    let key = key.trim();
    let valid = !key.is_empty()
        && key.chars().count() <= MAX_KEY_CHARS
        && key.chars().all(|c| c.is_ascii_graphic());
    if valid {
        Ok(key)
    } else {
        Err(SecretError::Invalid)
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

    /// Touches the real Windows Credential Manager (a throwaway entry, deleted at the end).
    #[test]
    #[ignore = "writes to the OS credential store"]
    fn round_trips_through_the_os_credential_store() {
        let service = "dev.interviewcopilot.desktop/selftest";
        set(service, "probe", "probe-value-123").unwrap();
        assert_eq!(
            get(service, "probe").unwrap().as_deref(),
            Some("probe-value-123")
        );
        delete(service, "probe").unwrap();
        assert_eq!(get(service, "probe").unwrap(), None);
    }
}
