//! Structured provider HTTP error bodies.
//!
//! pi `packages/ai/src/utils/error-body.ts` parity (R33).
//!
//! Both adapters store the raw HTTP body of a failed request in
//! [`synthia_core::Error::RequestFailed`], which already keeps the
//! status and the body together. [`parse_provider_error_body`]
//! unwraps the shapes providers actually send — OpenAI's
//! `{"error": {"type", "code", "message"}}`, the flat
//! `{"type", "message"}` some gateways return, and a non-JSON body —
//! so [`crate::retry::classify_provider_error_body`] can read the
//! provider's own `type` / `code` instead of guessing from substring
//! markers, and so a caller can show the provider's message instead
//! of a raw JSON blob.

use serde_json::Value;

/// Maximum number of characters kept in
/// [`ProviderErrorBody::body_excerpt`] (pi
/// `MAX_PROVIDER_ERROR_BODY_CHARS`): a proxy's HTML error page must
/// not become the whole error message.
pub const MAX_PROVIDER_ERROR_BODY_CHARS: usize = 4000;

/// The signal a provider HTTP error body carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderErrorBody {
    /// HTTP status the body arrived with.
    pub status: Option<u16>,
    /// Provider error type: OpenAI `error.type`
    /// (`insufficient_quota`), Anthropic `error.type`
    /// (`overloaded_error`), or a flat `type`.
    pub kind: Option<String>,
    /// Provider error code: OpenAI `error.code`
    /// (`context_length_exceeded`, `invalid_api_key`) or a flat
    /// `code`.
    pub code: Option<String>,
    /// Human-readable message (`error.message`, a flat `message`, or
    /// a bare `error` string).
    pub message: Option<String>,
    /// The raw body, trimmed and capped at
    /// [`MAX_PROVIDER_ERROR_BODY_CHARS`]. Always populated — this is
    /// what is left to show when nothing structured could be read.
    pub body_excerpt: String,
}

/// Parse the raw body of a failed HTTP response.
///
/// The error object is probed at `body["error"]` first (OpenAI and
/// Anthropic nest it) and at the top level otherwise, so all the real
/// shapes resolve:
///
/// - `{"error": {"type": "insufficient_quota", "code": ..., "message": ...}}`
/// - `{"type": "rate_limit_error", "message": "..."}`
/// - a non-JSON body (gateway HTML, a bare sentence) → excerpt only
///
/// A body that is not a JSON object (array, number, bare string) also
/// yields the excerpt only. `status` is always `Some(status)`: the
/// caller knows the response status even when the body is
/// unreadable.
#[must_use]
pub fn parse_provider_error_body(status: u16, raw: &str) -> ProviderErrorBody {
    let trimmed = raw.trim();
    let body = ProviderErrorBody {
        status: Some(status),
        body_excerpt: truncate_excerpt(trimmed),
        ..ProviderErrorBody::default()
    };
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return body;
    };
    let Some(object) = value.as_object() else {
        return body;
    };
    let fields = object
        .get("error")
        .and_then(Value::as_object)
        .unwrap_or(object);
    ProviderErrorBody {
        kind: string_field(fields.get("type")),
        code: string_field(fields.get("code")),
        message: string_field(fields.get("message"))
            .or_else(|| string_field(object.get("error"))),
        ..body
    }
}

/// Read a string field. Numbers are stringified so a provider that
/// sends `"code": 400` still yields a code.
fn string_field(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Trim-aware cap: short bodies pass through unchanged, longer ones
/// keep the first [`MAX_PROVIDER_ERROR_BODY_CHARS`] characters
/// (cut on a character boundary) plus the number of dropped
/// characters.
fn truncate_excerpt(text: &str) -> String {
    let total = text.chars().count();
    if total <= MAX_PROVIDER_ERROR_BODY_CHARS {
        return text.to_string();
    }
    let kept: String =
        text.chars().take(MAX_PROVIDER_ERROR_BODY_CHARS).collect();
    format!(
        "{kept}... [truncated {} chars]",
        total - MAX_PROVIDER_ERROR_BODY_CHARS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OpenAI's nested shape: `error.type` / `error.code` /
    /// `error.message` are the provider's own signal.
    #[test]
    fn openai_shaped_body_yields_type_code_and_message() {
        let raw = concat!(
            r#"{"error":{"message":"You exceeded your current quota, "#,
            r#"please check your plan and billing details.","#,
            r#""type":"insufficient_quota","param":null,"#,
            r#""code":"insufficient_quota"}}"#
        );
        let body = parse_provider_error_body(429, raw);
        assert_eq!(body.status, Some(429));
        assert_eq!(body.kind.as_deref(), Some("insufficient_quota"));
        assert_eq!(body.code.as_deref(), Some("insufficient_quota"));
        assert_eq!(
            body.message.as_deref(),
            Some(
                "You exceeded your current quota, please check your plan \
                 and billing details."
            )
        );
        assert_eq!(body.body_excerpt, raw);
    }

    /// Anthropic's nested shape keeps `error.type` as the kind even
    /// though the outer envelope also has a `type` of `"error"`.
    #[test]
    fn anthropic_shaped_body_prefers_the_nested_error_object() {
        let raw = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let body = parse_provider_error_body(529, raw);
        assert_eq!(body.kind.as_deref(), Some("overloaded_error"));
        assert_eq!(body.message.as_deref(), Some("Overloaded"));
        assert_eq!(body.code, None);
    }

    /// The flat `{"type", "message"}` gateway shape reads the top
    /// level, and a bare `"error"` string is the message.
    #[test]
    fn flat_body_shapes_yield_the_flat_fields() {
        let flat = parse_provider_error_body(
            429,
            r#"{"type":"rate_limit_error","message":"Rate limit reached"}"#,
        );
        assert_eq!(flat.kind.as_deref(), Some("rate_limit_error"));
        assert_eq!(flat.message.as_deref(), Some("Rate limit reached"));

        let stringified =
            parse_provider_error_body(403, r#"{"error":"Access denied"}"#);
        assert_eq!(stringified.kind, None);
        assert_eq!(stringified.message.as_deref(), Some("Access denied"));
    }

    /// A non-JSON body carries no structured signal, but the excerpt
    /// preserves what the server said.
    #[test]
    fn non_json_body_yields_excerpt_only() {
        let body = parse_provider_error_body(502, "<html>bad gateway</html>");
        assert_eq!(body.status, Some(502));
        assert_eq!(body.kind, None);
        assert_eq!(body.code, None);
        assert_eq!(body.message, None);
        assert_eq!(body.body_excerpt, "<html>bad gateway</html>");
    }

    /// A JSON body that is not an object is treated like any other
    /// unreadable body rather than guessing at its meaning.
    #[test]
    fn non_object_json_body_yields_excerpt_only() {
        let body = parse_provider_error_body(400, "[1, 2, 3]");
        assert_eq!(body.kind, None);
        assert_eq!(body.message, None);
        assert_eq!(body.body_excerpt, "[1, 2, 3]");
    }

    /// The excerpt is capped so a proxy's multi-megabyte body cannot
    /// be embedded in an error message; the tail reports the drop.
    #[test]
    fn body_excerpt_is_capped_and_reports_the_dropped_tail() {
        let raw = "é".repeat(MAX_PROVIDER_ERROR_BODY_CHARS + 7);
        let body = parse_provider_error_body(500, &raw);
        assert_eq!(
            body.body_excerpt,
            format!("{}... [truncated 7 chars]", "é".repeat(4000))
        );
    }

    /// A body at exactly the cap is short enough to keep whole.
    #[test]
    fn body_excerpt_keeps_a_body_at_the_cap_unchanged() {
        let raw = "x".repeat(MAX_PROVIDER_ERROR_BODY_CHARS);
        let body = parse_provider_error_body(500, &raw);
        assert_eq!(body.body_excerpt, raw);
    }

    /// Surrounding whitespace is not part of the signal; the trimmed
    /// body is what the excerpt (and the classifiers) see.
    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let body = parse_provider_error_body(429, " \n {\"type\":\"x\"} \n ");
        assert_eq!(body.kind.as_deref(), Some("x"));
        assert_eq!(body.body_excerpt, "{\"type\":\"x\"}");
    }

    /// A numeric code (some gateways stringify it) still yields a
    /// code, while a non-scalar field is ignored.
    #[test]
    fn numeric_code_is_stringified_and_objects_are_ignored() {
        let body = parse_provider_error_body(
            400,
            r#"{"error":{"code":400,"type":{"nested":true}}}"#,
        );
        assert_eq!(body.code.as_deref(), Some("400"));
        assert_eq!(body.kind, None);
        assert_eq!(body.message, None);
    }
}
