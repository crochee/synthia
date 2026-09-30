//! Sensitive information encryption / redaction primitives.
//!
//! Canonical location for [`Sensitive`] and [`SensitiveData`]. Previously
//! lived in `synthia-telemetry`; moved here because it is a cross-cutting
//! domain capability unrelated to OpenTelemetry pipelines.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub trait SensitiveData: Send + Sync {
    fn sanitized(&self) -> String {
        "***".to_string()
    }

    fn sensitive_fields() -> Vec<&'static str> {
        vec![]
    }
}

pub struct Sensitive<T>(pub T);

impl<T: Clone> Clone for Sensitive<T> {
    fn clone(&self) -> Self {
        Sensitive(self.0.clone())
    }
}

impl<T> Sensitive<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn into_inner(self) -> T {
        self.0
    }

    pub fn inner(&self) -> &T {
        &self.0
    }
}

impl<T: SensitiveData> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sensitive({})", self.0.sanitized())
    }
}

impl<T: SensitiveData> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.sanitized())
    }
}

impl<T: SensitiveData> Serialize for Sensitive<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.sanitized())
    }
}

impl<'de, T> Deserialize<'de> for Sensitive<T>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = s.parse::<T>().map_err(serde::de::Error::custom)?;
        Ok(Sensitive(value))
    }
}

impl<T: SensitiveData> SensitiveData for Sensitive<T> {
    fn sanitized(&self) -> String {
        self.0.sanitized()
    }
}

impl SensitiveData for str {
    fn sanitized(&self) -> String {
        "***".to_string()
    }
}

impl SensitiveData for String {
    fn sanitized(&self) -> String {
        "***".to_string()
    }
}

impl<T: SensitiveData> SensitiveData for Option<T> {
    fn sanitized(&self) -> String {
        match self {
            Some(v) => v.sanitized(),
            None => "None".to_string(),
        }
    }
}

impl<T: SensitiveData> SensitiveData for Vec<T> {
    fn sanitized(&self) -> String {
        format!("[{} items]", self.len())
    }
}

/// Partially redact a string for display, keeping enough of the
/// prefix and suffix that an operator can identify a credential
/// in a log or UI without leaking the secret.
///
/// The contract is fixed at **first 4 + `***` + last 3** chars —
/// the most common pattern for API keys (Anthropic, OpenAI,
/// etc., all carry a recognisable prefix like `sk-` or
/// `sk-ant-` that the first 4 chars preserve). For other
/// shapes (e.g. AWS access keys with a longer prefix, or a
/// service that wants a different marker), use
/// [`redact_partial_with`] with explicit `keep_first` /
/// `keep_last` parameters.
///
/// # Edge cases
///
/// - Empty input → empty output.
/// - Input of 7 or fewer characters → fully masked
///   (`"***"`), because showing any prefix of a 7-char
///   string would leak the entire secret.
/// - Input of 8 characters (the minimum that yields a
///   non-empty middle) → first 4 + `***` + last 3, with the
///   middle "1 char" replaced by `***`.
/// - Multibyte safe: operates on Unicode code points
///   (`char`), not bytes, so a 4-codepoint prefix that
///   happens to span 12 UTF-8 bytes is still exactly 4
///   code points in the output.
#[must_use]
pub fn redact_partial(s: &str) -> String {
    redact_partial_with(s, 4, 3)
}

/// Partially redact `s` for display, keeping `keep_first`
/// characters at the start and `keep_last` at the end. The
/// middle is replaced by `***`. The same edge cases as
/// [`redact_partial`] apply, with one twist: when
/// `keep_first + keep_last` is greater than the input
/// length, the function falls back to full-mask (`"***"`)
/// to avoid leaking the entire secret.
#[must_use]
pub fn redact_partial_with(
    s: &str,
    keep_first: usize,
    keep_last: usize,
) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= keep_first + keep_last {
        return "***".to_string();
    }
    let first: String = chars.iter().take(keep_first).collect();
    let last: String = chars
        .iter()
        .rev()
        .take(keep_last)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{first}***{last}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_string_sanitized() {
        let s = "secret-api-key".to_string();
        assert_eq!(s.sanitized(), "***");
    }

    #[test]
    fn test_sensitive_str_sanitized() {
        let s = "another-secret";
        assert_eq!(s.sanitized(), "***");
    }

    #[test]
    fn test_sensitive_new_and_inner() {
        let s = Sensitive::new("test-value".to_string());
        assert_eq!(s.inner(), "test-value");
    }

    #[test]
    fn test_sensitive_into_inner() {
        let s = Sensitive::new("real-api-key".to_string());
        assert_eq!(s.into_inner(), "real-api-key");
    }

    #[test]
    fn test_sensitive_debug() {
        let s = Sensitive("sk-proj-abc123def456".to_string());
        let debug = format!("{:?}", s);
        assert_eq!(debug, "Sensitive(***)");
    }

    #[test]
    fn test_sensitive_display() {
        let s = Sensitive("my-api-key-value".to_string());
        let display = format!("{}", s);
        assert_eq!(display, "***");
    }

    #[test]
    fn test_sensitive_serialize() {
        let s = Sensitive("secret-key-12345".to_string());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"***\"");
    }

    #[test]
    fn test_option_some_sanitized() {
        let opt: Option<Sensitive<String>> =
            Some(Sensitive::new("secret".to_string()));
        assert_eq!(opt.sanitized(), "***");
    }

    #[test]
    fn test_option_none_sanitized() {
        let opt: Option<Sensitive<String>> = None;
        assert_eq!(opt.sanitized(), "None");
    }

    #[test]
    fn test_vec_sanitized() {
        let vec: Vec<Sensitive<String>> = vec![
            Sensitive::new("key1".to_string()),
            Sensitive::new("key2".to_string()),
        ];
        assert_eq!(vec.sanitized(), "[2 items]");
    }

    #[test]
    fn test_empty_vec_sanitized() {
        let vec: Vec<Sensitive<String>> = vec![];
        assert_eq!(vec.sanitized(), "[0 items]");
    }

    #[test]
    fn test_sensitive_fields_default() {
        struct TestStruct;
        impl SensitiveData for TestStruct {}
        assert_eq!(TestStruct::sensitive_fields(), Vec::<&'static str>::new());
    }

    #[test]
    fn test_sensitive_deserialize() {
        let json = "\"my-secret-key\"";
        let s: Sensitive<String> = serde_json::from_str(json).unwrap();
        assert_eq!(s.into_inner(), "my-secret-key");
    }

    // ===== redact_partial / redact_partial_with =====

    /// `redact_partial` is the workspace canonical helper for
    /// showing an API key in a log or UI without leaking the
    /// secret. Contract: first 4 + `***` + last 3, with a
    /// ≤7-char full-mask fallback.
    #[test]
    fn redact_partial_long_key_keeps_first_4_and_last_3() {
        assert_eq!(redact_partial("sk-1234567890abcdef"), "sk-1***def");
    }

    /// Empty input → empty output (not `"***"`).
    #[test]
    fn redact_partial_empty_returns_empty() {
        assert_eq!(redact_partial(""), "");
    }

    /// ≤7 chars: showing any prefix of a 7-char string would
    /// leak the entire secret. The function must full-mask
    /// in this case.
    #[test]
    fn redact_partial_short_input_is_fully_masked() {
        assert_eq!(redact_partial("abc"), "***");
        assert_eq!(redact_partial("abcd"), "***");
        assert_eq!(redact_partial("abcdefg"), "***");
    }

    /// 8 chars: first 4 + `***` + last 3, with the middle
    /// "1 char" replaced by `***`. Pin this so a refactor
    /// doesn't widen the contract past the 4+3 split.
    #[test]
    fn redact_partial_exactly_8_chars_shows_4_plus_3() {
        assert_eq!(redact_partial("abcdefgh"), "abcd***fgh");
    }

    /// Multibyte safe: 4-codepoint prefix that happens to
    /// span 12 UTF-8 bytes is still exactly 4 code points in
    /// the output. Pin this so a refactor that switches to
    /// byte-indexing doesn't corrupt non-ASCII keys.
    #[test]
    fn redact_partial_is_multibyte_safe() {
        let key = "éàüöñêùî"; // 8 codepoints
        let masked = redact_partial(key);
        let first_4: String = key.chars().take(4).collect();
        let last_3: String = key
            .chars()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        assert_eq!(masked, format!("{first_4}***{last_3}"));
    }

    /// The middle of a long key must not leak through the
    /// `***` marker. Pin the negative case.
    #[test]
    fn redact_partial_does_not_leak_middle_of_long_key() {
        let key = "sk-live-0123456789-abcdef-XYZ";
        let masked = redact_partial(key);
        assert_eq!(masked, "sk-l***XYZ");
        assert!(!masked.contains("0123456789"));
        assert!(!masked.contains("abcdef"));
    }

    /// `redact_partial_with` is the parameterised version:
    /// a deployment that wants a different prefix/suffix
    /// length (e.g. AWS access keys with a 4-char prefix
    /// and a 4-char suffix) can ask for it without
    /// re-implementing the loop.
    #[test]
    fn redact_partial_with_respects_keep_first_and_keep_last() {
        // 10 chars, keep_first=4, keep_last=4 → first 4 + *** + last 4,
        // the middle 2 chars (e,f) replaced by ***.
        assert_eq!(redact_partial_with("abcdefghij", 4, 4), "abcd***ghij");
    }

    /// `redact_partial_with` falls back to full-mask when
    /// `keep_first + keep_last` would equal or exceed the
    /// input length (no useful "middle" can be shown).
    #[test]
    fn redact_partial_with_falls_back_to_full_mask_when_input_is_short() {
        assert_eq!(redact_partial_with("abc", 4, 3), "***");
        assert_eq!(redact_partial_with("abcdefg", 4, 3), "***");
    }
}
