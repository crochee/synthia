//! Mid-stream prefix completion for tool-call argument deltas.
//!
//! Anthropic and OpenAI stream tool-arg deltas as raw strings
//! (`{"location":`, then `"Beijing"`, then `"}`). pi
//! `packages/ai/src/utils/json-parse.ts::parseStreamingJson` (via the
//! `partial-json` package) salvages those cut-off prefixes; this
//! module reimplements the same shape in-tree so synthia stays
//! runtime-neutral with no new dependency.
//!
//! [`complete_partial_json`] walks the input tracking object/array
//! depth and string state, then emits the smallest textual
//! completion that closes any open structure and drops a trailing
//! comma. [`parse_tool_input_with_completion`] wraps it into the
//! existing `(Value, ToolArgsQuality)` contract used by
//! [`super::parse_tool_input_reported`], keeping `Strict` /
//! `Repaired` / `RawFallback` unchanged — the completion path
//! surfaces as `Strict` because the completed text is valid JSON.
//!
//! What this does NOT do (deliberately, to keep the completion
//! surface tiny):
//!
//! - no escape correction or backslash doubling (that lives in
//!   [`super::repair_json`]).
//! - no comment insertion (`/* … */`, `//`).
//! - no key rewrites — an object whose most recent key has no
//!   value (`: ` followed by nothing) is *not* salvageable by
//!   completion; the function returns `None` instead of inserting
//!   a placeholder value, because guessing `null` / `""` would
//!   silently change the tool-call's argument shape.
//! - no trailing-whitespace stripping beyond what is needed to
//!   detect a trailing comma / colon at the closing boundary.

use serde_json::Value;

/// One open structural frame on the parse stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Object,
    Array,
}

/// Try to close a partial JSON document so it parses.
///
/// Returns `Some(completed)` when `raw` is recognisably the
/// prefix of a JSON document that needs structural closing:
/// e.g. `{"cmd":"ls` → `{"cmd":"ls"}`,
/// `{"a":1, "b":[1,` → `{"a":1, "b":[1]}`.
///
/// Returns `None` in three cases:
///
/// - `raw` is empty.
/// - `raw` already parses as JSON (it is already complete).
/// - `raw` cannot be salvaged by structural closing alone:
///   gibberish, an unterminated object key (`{"a":`), or a
///   stray token that no closing pass can recover.
#[must_use]
pub fn complete_partial_json(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if serde_json::from_str::<Value>(raw).is_ok() {
        return None;
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut in_string = false;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\\' {
                // Consume the escaped character so the closing quote
                // we observe later is the one that ends the string,
                // not an escaped quote inside it.
                chars.next();
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => in_string = true,
                '{' => stack.push(Frame::Object),
                '[' => stack.push(Frame::Array),
                '}' | ']' => {
                    stack.pop();
                }
                _ => {}
            }
        }
    }

    let mut out = String::with_capacity(raw.len() + stack.len() + 1);
    out.push_str(raw);
    if in_string {
        close_open_string(&mut out);
    }
    while let Some(frame) = stack.pop() {
        if !close_frame(&mut out, frame) {
            return None;
        }
    }
    if serde_json::from_str::<Value>(&out).is_ok() {
        Some(out)
    } else {
        None
    }
}

/// Drop a trailing comma from the in-progress output. Trims
/// trailing whitespace first so `"a":1, ` is treated the same as
/// `"a":1,`. Returns `true` if a comma was dropped.
fn drop_trailing_comma(out: &mut String) -> bool {
    while out.chars().next_back().is_some_and(char::is_whitespace) {
        out.pop();
    }
    if out.ends_with(',') {
        out.pop();
        return true;
    }
    false
}

/// Close an open string literal by appending a quote. If the
/// string ends with a lone trailing backslash (an escape
/// sequence with no escaped character), the backslash is
/// doubled so it stays a literal `\`, then the closing quote is
/// appended — otherwise the appended quote would be parsed as
/// the char the unterminated escape was waiting for and the
fn close_open_string(out: &mut String) {
    if out.ends_with('\\') {
        out.push('\\');
    }
    out.push('"');
}

/// Close the innermost still-open frame.
///
/// Returns `false` when the frame cannot be closed because the
/// most recent token is a colon (an object key with no value) —
/// insertion of a placeholder would silently change the tool
/// call's argument shape. Trailing commas are dropped via
/// [`drop_trailing_comma`] before the closing bracket is
/// appended.
fn close_frame(out: &mut String, frame: Frame) -> bool {
    drop_trailing_comma(out);
    match out.chars().next_back() {
        Some(':') => false,
        _ => {
            out.push(matching_close(frame));
            true
        }
    }
}

fn matching_close(frame: Frame) -> char {
    match frame {
        Frame::Object => '}',
        Frame::Array => ']',
    }
}

/// Parse tool-call arguments, also trying [`complete_partial_json`]
/// as a final salvage step.
///
/// Behaviour matches [`super::parse_tool_input_reported`] for the
/// `Strict` / `Repaired` paths; the third arm runs the prefix
/// completion, returning `Strict` because the completed text
/// itself parses as valid JSON. `RawFallback` is preserved so the
/// R33 agent wiring is unaffected — only the prefix-salvage case
/// behaves differently (and only when a structural close is
/// possible).
#[must_use]
pub fn parse_tool_input_with_completion(
    raw: &str,
) -> (Value, super::ToolArgsQuality) {
    if raw.trim().is_empty() {
        let empty = Value::Object(serde_json::Map::new());
        return (empty, super::ToolArgsQuality::Strict);
    }
    if let Ok(value) = serde_json::from_str(raw) {
        return (value, super::ToolArgsQuality::Strict);
    }
    let repaired = super::repair_json(raw);
    if repaired != raw
        && let Ok(value) = serde_json::from_str(&repaired)
    {
        return (value, super::ToolArgsQuality::Repaired);
    }
    if let Some(completed) = complete_partial_json(raw)
        && let Ok(value) = serde_json::from_str(&completed)
    {
        return (value, super::ToolArgsQuality::Strict);
    }
    (
        Value::String(raw.to_string()),
        super::ToolArgsQuality::RawFallback,
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Salvage a truncated string value: the model emitted the
    /// opening quote of the value but the closing quote was cut.
    /// Anthropic-style `{"cmd":"ls` becomes `{"cmd":"ls"}`.
    #[test]
    fn completes_truncated_string_value() {
        let completed =
            complete_partial_json(r#"{"cmd":"ls"#).expect("completes");
        assert_eq!(completed, r#"{"cmd":"ls"}"#);
        let value: Value = serde_json::from_str(&completed).unwrap();
        assert_eq!(value, json!({"cmd": "ls"}));
    }

    /// Two open structures are closed innermost-first; the
    /// trailing comma after `1` is dropped before the array
    /// close, then the object close follows.
    #[test]
    fn completes_nested_object_with_open_array() {
        let completed =
            complete_partial_json(r#"{"a":1, "b":[1,"#).expect("completes");
        assert_eq!(completed, r#"{"a":1, "b":[1]}"#);
        let value: Value = serde_json::from_str(&completed).unwrap();
        assert_eq!(value, json!({"a": 1, "b": [1]}));
    }

    /// Trailing comma inside an array is dropped on the way to
    /// the closing bracket.
    #[test]
    fn drops_trailing_comma_inside_array() {
        let completed =
            complete_partial_json(r#"{"list":[1,2,"#).expect("completes");
        assert_eq!(completed, r#"{"list":[1,2]}"#);
        let value: Value = serde_json::from_str(&completed).unwrap();
        assert_eq!(value, json!({"list": [1, 2]}));
    }

    /// A bare top-level unterminated string is salvageable
    /// because it is recognisably the prefix of a JSON string.
    #[test]
    fn completes_bare_unterminated_string() {
        let completed =
            complete_partial_json(r#""unterminated"#).expect("completes");
        assert_eq!(completed, r#""unterminated""#);
        let value: Value = serde_json::from_str(&completed).unwrap();
        assert_eq!(value, json!("unterminated"));
    }

    /// An already-balanced document is returned as `None` —
    /// completion is for prefixes, not redundant re-parses.
    #[test]
    fn balanced_input_returns_none() {
        for raw in [r#"{"a":1}"#, "[]", r#""hi""#] {
            assert!(
                complete_partial_json(raw).is_none(),
                "balanced input must not be completed: {raw}"
            );
        }
    }

    /// Gibberish is not a prefix of any JSON document; structural
    /// closing cannot make it parse.
    #[test]
    fn gibberish_returns_none() {
        for raw in ["gibberish", "}{", "abc 123", "] ["] {
            assert!(
                complete_partial_json(raw).is_none(),
                "gibberish must not be completed: {raw}"
            );
        }
    }

    /// An object whose most recent key has no value cannot be
    /// salvaged by structural closing: inserting a placeholder
    /// value would silently change the tool call's argument
    /// shape. Completion returns `None`.
    #[test]
    fn unclosed_object_key_returns_none() {
        for raw in [r#"{"a":"#, r#"{"a": , "#] {
            assert!(
                complete_partial_json(raw).is_none(),
                "open-key object must not be completed: {raw}"
            );
        }
    }
    /// Backslash escapes inside a string must not flip the
    /// in-string state, otherwise an escaped quote would be
    /// mistaken for the terminator. A lone trailing backslash
    /// with no escaped char is doubled so the string still
    /// closes with a literal `\` rather than swallowing the
    /// appended closing quote.
    #[test]
    fn backslash_inside_string_does_not_close_it() {
        let completed =
            complete_partial_json(r#"{"a":"he\"#).expect("completes");
        assert_eq!(completed, r#"{"a":"he\\"}"#);
        let value: Value = serde_json::from_str(&completed).unwrap();
        assert_eq!(value, json!({"a": "he\\"}));
    }
    /// prefix that the strict + repair paths would have left as
    /// `RawFallback`. The completed value is reported as
    /// `Strict` because the completed text is valid JSON.
    #[test]
    fn parse_with_completion_salvages_truncated_prefix() {
        let (value, quality) =
            parse_tool_input_with_completion(r#"{"cmd":"ls"#);
        assert_eq!(quality, super::super::ToolArgsQuality::Strict);
        assert_eq!(value, json!({"cmd": "ls"}));
    }

    /// `RawFallback` is still reachable: gibberish is not a
    /// prefix of any JSON document, and `parse_tool_input_with_completion`
    /// preserves the existing fallback contract.
    #[test]
    fn parse_with_completion_preserves_raw_fallback_for_gibberish() {
        let (value, quality) = parse_tool_input_with_completion("gibberish");
        assert_eq!(quality, super::super::ToolArgsQuality::RawFallback);
        assert_eq!(value, Value::String("gibberish".to_string()));
    }

    /// An already-strict payload must still come back as
    /// `Strict` with no structural rewriting.
    #[test]
    fn parse_with_completion_keeps_strict_for_valid_json() {
        let (value, quality) =
            parse_tool_input_with_completion(r#"{"cmd":"ls"}"#);
        assert_eq!(quality, super::super::ToolArgsQuality::Strict);
        assert_eq!(value, json!({"cmd": "ls"}));
    }

    /// `Repaired` is still reachable through the wrapper: the
    /// repair step (control-character escape / backslash double)
    /// runs before completion.
    #[test]
    fn parse_with_completion_keeps_repaired_for_string_internal_fixes() {
        let (value, quality) =
            parse_tool_input_with_completion("{\"a\": \"line\nbreak\"}");
        assert_eq!(quality, super::super::ToolArgsQuality::Repaired);
        assert_eq!(value, json!({"a": "line\nbreak"}));
    }
}
