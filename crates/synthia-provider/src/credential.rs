//! Credential validation — classify an API key BEFORE sending.
//!
//! R29 (dsh `normalizeApiKey` / `ApiKeyRejection` parity, see
//! `docs/subsystems/llm/`).
//!
//! dsh's invariant: a malformed credential fails with
//! `ApiKeyRejection('empty' | 'illegal-characters')` BEFORE
//! the request reaches `fetch`. Synthia had no such
//! preflight — a user-pasted `"Bearer \n"` or `"sk-…\t"`
//! would silently fail inside undici as
//! `TypeError: fetch failed`.
//!
//! The classifier lives at the resolve step in
//! `WorkspaceConfig::resolve_api_key` and
//! `ProviderProfile::resolve_api_key`, returning
//! [`CredentialError::Empty`] or
//! [`CredentialError::IllegalCharacters`] instead of
//! `Error::Config`. The server boundary maps these to
//! `INVALID_CREDENTIAL` (deliberately NOT in the default
//! retryable set).

use thiserror::Error;

/// Why a credential was rejected at the resolve step.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CredentialError {
    /// The credential was empty after trimming.
    #[error("API key is empty")]
    Empty,

    /// The credential contained characters outside the
    /// printable-ASCII set
    /// (`U+0021`–`U+007E`, excluding the space
    /// `U+0020` and the delete `U+007F`).
    #[error("API key contains illegal characters at byte index {byte_index}")]
    IllegalCharacters { byte_index: usize },

    /// The credential was provided through a mechanism the
    /// resolver does not know about. Synthia treats this as a
    /// missing-credential event for wire-level reporting.
    #[error("API key source '{source_name}' is unknown to the resolver")]
    UnknownSource { source_name: String },
}
/// [`CredentialError`] describing the first violation.
///
/// Rules (dsh `normalizeApiKey` parity, loosened for
/// provider-agnostic acceptance):
///
/// - Trim leading / trailing ASCII whitespace.
/// - The trimmed string must be at least 1 byte.
/// - Every byte must be in `0x21..=0x7E` (printable ASCII,
///   excluding space and DEL).
/// - The trimmed string is returned; the input is not
///   modified otherwise (no silent rewriting of common
///   `Bearer ` prefixes, no charset detection, etc.).
pub fn normalize_api_key(raw: &str) -> Result<String, CredentialError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CredentialError::Empty);
    }
    for (i, b) in trimmed.bytes().enumerate() {
        if !(0x21..=0x7E).contains(&b) {
            return Err(CredentialError::IllegalCharacters { byte_index: i });
        }
    }
    Ok(trimmed.to_string())
}

/// High-level classification. Lets the resolve path branch
/// with one match instead of inspecting error variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CredentialStatus {
    /// Credential is well-formed.
    Valid,
    /// Credential was empty after trim.
    Empty,
    /// Credential contained illegal characters.
    Illegal,
}

/// One-shot classify helper. Returns the
/// [`CredentialStatus`] and the trimmed value when valid.
pub fn classify(raw: &str) -> (CredentialStatus, Option<String>) {
    match normalize_api_key(raw) {
        Ok(trimmed) => (CredentialStatus::Valid, Some(trimmed)),
        Err(CredentialError::Empty) => (CredentialStatus::Empty, None),
        Err(CredentialError::IllegalCharacters { .. }) => {
            (CredentialStatus::Illegal, None)
        }
        // `UnknownSource` only fires from the resolver, not
        // the classifier.
        Err(CredentialError::UnknownSource { .. }) => {
            (CredentialStatus::Empty, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_a_well_formed_key() {
        let k = normalize_api_key("sk-test-12345").expect("ok");
        assert_eq!(k, "sk-test-12345");
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let k = normalize_api_key("  sk-test  ").expect("ok");
        assert_eq!(k, "sk-test");
    }

    #[test]
    fn rejects_empty_string() {
        assert_eq!(normalize_api_key(""), Err(CredentialError::Empty));
    }

    #[test]
    fn rejects_whitespace_only() {
        assert_eq!(normalize_api_key("   "), Err(CredentialError::Empty));
    }

    #[test]
    fn rejects_embedded_newline() {
        // A paste accident in the middle of a key — e.g. an
        // editor that injected a literal newline.
        let err = normalize_api_key("sk\ntest").unwrap_err();
        match err {
            CredentialError::IllegalCharacters { byte_index } => {
                assert_eq!(byte_index, 2);
            }
            other => panic!("expected IllegalCharacters, got {other:?}"),
        }
    }

    #[test]
    fn rejects_embedded_tab() {
        assert!(matches!(
            normalize_api_key("sk\ttest"),
            Err(CredentialError::IllegalCharacters { .. })
        ));
    }

    #[test]
    fn rejects_non_ascii() {
        assert!(matches!(
            normalize_api_key("sk-test-é"),
            Err(CredentialError::IllegalCharacters { .. })
        ));
    }

    #[test]
    fn accepts_underscore_and_hyphen() {
        let k = normalize_api_key("sk_test-abc_123-XYZ").expect("ok");
        assert_eq!(k, "sk_test-abc_123-XYZ");
    }

    #[test]
    fn classify_returns_status_triple() {
        let (status, value) = classify("sk-test");
        assert_eq!(status, CredentialStatus::Valid);
        assert_eq!(value.as_deref(), Some("sk-test"));

        let (status, value) = classify("");
        assert_eq!(status, CredentialStatus::Empty);
        assert!(value.is_none());

        let (status, value) = classify("sk\ntest");
        assert_eq!(status, CredentialStatus::Illegal);
        assert!(value.is_none());
    }
}
