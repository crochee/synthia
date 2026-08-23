// Allow `result_large_err` for the whole file: P1b added 4 hidden
// fields to every struct-form variant (frames, backtrace, source,
// and the synthetic source chain), so every `Result<_, Error>` is
// at least 128 bytes. Boxing the error would force every call site
// to `.map_err(|e| *e)` (or accept the allocation), and the existing
// API has no `Box<Error>` in the public surface. Accept the size
// cost; revisit if profiling shows it matters.
#![allow(clippy::result_large_err)]

//! Request-parameter validation helpers for v1 endpoints.
//!
//! - [`validate_resource_name`]: regex `^[a-zA-Z0-9_-]{1,255}$`
//!   — prevents path traversal (`../etc/passwd`) and other
//!   injected path components.
//! - [`validate_sort`]: whitelist enforcement for sort fields
//!   (after stripping the optional `-` descending prefix).
//! - [`api_key_mask`]: keep first 4 + last 3 chars, middle `***`.
//!
//! Validators return the cross-crate
//! [`synthia_core::Error`] (specifically [`Error::Validation`] or
//! [`Error::InvalidItem`]) so HTTP transports can layer their own
//! error-envelope mapping on top — this crate stays free of
//! wire-format concerns.

use std::sync::OnceLock;

use regex::Regex;
use synthia_core::Error;

static RESOURCE_NAME_RE: OnceLock<Regex> = OnceLock::new();

fn resource_name_regex() -> &'static Regex {
    RESOURCE_NAME_RE.get_or_init(|| {
        // ^[a-zA-Z0-9_-]{1,255}$ — alphanumerics, underscore, hyphen.
        // 1-255 chars total. Anchored to reject suffix/prefix
        // tricks (e.g. "../etc/passwd" or "name\x00.txt").
        Regex::new(r"^[a-zA-Z0-9_-]{1,255}$")
            .expect("resource name regex is valid")
    })
}

/// Validate a resource name path parameter against
/// `^[a-zA-Z0-9_-]{1,255}$`.
///
/// Returns `Ok(())` if valid, otherwise `Err(Error)` with
/// the [`Error::InvalidItem`] variant.
///
/// # Examples
///
/// ```
/// use synthia_server::api::validate_resource_name;
/// assert!(validate_resource_name("debugging").is_ok());
/// assert!(validate_resource_name("../etc/passwd").is_err());
/// assert!(validate_resource_name("").is_err());
/// ```
pub fn validate_resource_name(name: &str) -> Result<(), Error> {
    if resource_name_regex().is_match(name) {
        Ok(())
    } else {
        Err(Error::invalid_item(name))
    }
}

/// Validate a sort parameter against a whitelist.
///
/// Strips a leading `-` (descending marker) before checking
/// membership. Empty sort strings (after trimming) are rejected.
/// Returns `Ok(())` if the field is whitelisted, otherwise
/// `Err(Error)` with the [`Error::Validation`] variant.
///
/// # Examples
///
/// ```
/// use synthia_server::api::validate_sort;
/// assert!(validate_sort("name", &["name", "created_at"]).is_ok());
/// assert!(validate_sort("-created_at", &["name", "created_at"]).is_ok());
/// assert!(validate_sort("invalid", &["name", "created_at"]).is_err());
/// ```
pub fn validate_sort(sort: &str, whitelist: &[&str]) -> Result<(), Error> {
    let trimmed = sort.trim();
    if trimmed.is_empty() {
        return Err(Error::validation("sort field cannot be empty"));
    }
    let field = trimmed.strip_prefix('-').unwrap_or(trimmed);
    if field.is_empty() {
        return Err(Error::validation("sort field cannot be empty"));
    }
    if whitelist.contains(&field) {
        Ok(())
    } else {
        Err(Error::invalid_item(field))
    }
}

/// Mask an API key for safe display.
///
/// - Empty input → `""`.
/// - ≤ 7 chars → `"***"` (full mask — first 4 + last 3 would
///   overlap and leak too much).
/// - Otherwise → `"{first_4_chars}***{last_3_chars}"`.
///
/// Operates on Unicode code points (`char`) rather than bytes
/// so multibyte keys are not corrupted.
///
/// # Examples
///
/// ```
/// use synthia_server::api::api_key_mask;
/// assert_eq!(api_key_mask("sk-1234567890abcdef"), "sk-1***def");
/// assert_eq!(api_key_mask(""), "");
/// assert_eq!(api_key_mask("abc"), "***");
/// ```
pub fn api_key_mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= 7 {
        return "***".to_string();
    }
    let first: String = chars.iter().take(4).collect();
    let last: String = chars
        .iter()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}***{}", first, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_resource_name ---

    #[test]
    fn valid_resource_names_pass() {
        assert!(validate_resource_name("debugging").is_ok());
        assert!(validate_resource_name("rust-tool").is_ok());
        assert!(validate_resource_name("mcp_server_1").is_ok());
        assert!(validate_resource_name("a").is_ok());
        assert!(validate_resource_name("A_B-C-123").is_ok());
    }

    #[test]
    fn resource_name_path_traversal_rejected() {
        assert!(validate_resource_name("../etc/passwd").is_err());
    }

    #[test]
    fn resource_name_empty_rejected() {
        assert!(validate_resource_name("").is_err());
    }

    #[test]
    fn resource_name_with_slash_rejected() {
        assert!(validate_resource_name("a/b").is_err());
    }

    #[test]
    fn resource_name_with_space_rejected() {
        assert!(validate_resource_name("hello world").is_err());
    }

    #[test]
    fn resource_name_with_dot_rejected() {
        assert!(validate_resource_name("skill.json").is_err());
    }

    #[test]
    fn resource_name_with_special_chars_rejected() {
        for bad in &["skill@name", "skill#1", "skill:foo", "skill;bar"] {
            assert!(
                validate_resource_name(bad).is_err(),
                "{:?} should be rejected",
                bad
            );
        }
    }

    #[test]
    fn resource_name_256_chars_rejected() {
        let long = "a".repeat(256);
        assert!(validate_resource_name(&long).is_err());
    }

    #[test]
    fn resource_name_255_chars_passes() {
        let max = "a".repeat(255);
        assert!(validate_resource_name(&max).is_ok());
    }

    // --- validate_sort ---

    #[test]
    fn validate_sort_plain_field_passes() {
        assert!(validate_sort("name", &["name", "created_at"]).is_ok());
        assert!(validate_sort("created_at", &["name", "created_at"]).is_ok());
    }

    #[test]
    fn validate_sort_descending_prefix_passes() {
        assert!(validate_sort("-created_at", &["name", "created_at"]).is_ok());
        assert!(validate_sort("-name", &["name", "created_at"]).is_ok());
    }

    #[test]
    fn validate_sort_invalid_field_rejected() {
        assert!(validate_sort("invalid", &["name", "created_at"]).is_err());
    }

    #[test]
    fn validate_sort_invalid_descending_field_rejected() {
        assert!(validate_sort("-invalid", &["name", "created_at"]).is_err());
    }

    #[test]
    fn validate_sort_empty_string_rejected() {
        assert!(validate_sort("", &["name"]).is_err());
    }

    #[test]
    fn validate_sort_whitespace_only_rejected() {
        assert!(validate_sort("   ", &["name"]).is_err());
    }

    #[test]
    fn validate_sort_dash_only_rejected() {
        assert!(validate_sort("-", &["name"]).is_err());
    }

    #[test]
    fn validate_sort_trims_whitespace() {
        assert!(validate_sort("  name  ", &["name"]).is_ok());
        assert!(validate_sort("  -name  ", &["name"]).is_ok());
    }

    #[test]
    fn validate_sort_empty_whitelist_rejects_anything() {
        assert!(validate_sort("name", &[]).is_err());
    }

    // --- api_key_mask ---

    #[test]
    fn mask_long_key_keeps_first_4_and_last_3() {
        assert_eq!(api_key_mask("sk-1234567890abcdef"), "sk-1***def");
    }

    #[test]
    fn mask_empty_returns_empty() {
        assert_eq!(api_key_mask(""), "");
    }

    #[test]
    fn mask_null_in_short_input_returns_full_mask() {
        // Short input (≤7 chars) → fully masked.
        assert_eq!(api_key_mask("abc"), "***");
        assert_eq!(api_key_mask("abcd"), "***");
        assert_eq!(api_key_mask("abcdefg"), "***");
    }

    #[test]
    fn mask_exactly_8_chars_shows_4_plus_3() {
        // 8 chars: first 4 + last 3 + *** (no overlap, since
        // first 4 = chars[0..4], last 3 = chars[5..8]).
        assert_eq!(api_key_mask("abcdefgh"), "abcd***fgh");
    }

    #[test]
    fn mask_multibyte_safe() {
        // 8 code points (mix of multibyte chars).
        let key = "éàüöñêùî";
        let masked = api_key_mask(key);
        // first 4 chars + *** + last 3 chars
        let first_4: String = key.chars().take(4).collect();
        let last_3: String = key
            .chars()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        assert_eq!(masked, format!("{}***{}", first_4, last_3));
    }

    #[test]
    fn mask_does_not_leak_middle_of_long_key() {
        let key = "sk-live-0123456789-abcdef-XYZ";
        let masked = api_key_mask(key);
        assert_eq!(masked, "sk-l***XYZ");
        assert!(!masked.contains("0123456789"));
        assert!(!masked.contains("abcdef"));
    }
}
