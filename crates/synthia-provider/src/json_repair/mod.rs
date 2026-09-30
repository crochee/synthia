//! Tolerant parsing for streamed tool-call arguments.
//!
//! pi `packages/ai/src/utils/json-parse.ts` parity (R33).
//!
//! Models emit tool-call argument JSON that is *almost* valid: a raw
//! newline inside a string literal, or a backslash that starts no
//! valid JSON escape (`"\d+"` for a regex, `"C:\q"` for a path).
//! [`repair_json`] fixes exactly those two malformations inside
//! string literals and copies everything else byte for byte.
//! [`parse_tool_input_reported`] runs the strict parse first, then
//! the repair, then hands the raw text through as
//! [`Value::String`] — a tool call is never silently dropped, and
//! the caller learns which step it took.
//!
//! Every finalization site — both provider stream processors and
//! [`crate::BlockAssembler`] — goes through
//! [`parse_tool_input_logged`], so a salvaged payload is reported at
//! `WARN` instead of vanishing.

//! [`completion::complete_partial_json`] adds a structural-closing
//! pass that salvages mid-stream cut-off prefixes (Anthropic and
//! OpenAI stream tool args as raw JSON deltas; a truncated
//! `{"cmd":"ls` is not yet a valid document). Live-tail
//! projections consume the wrapper
//! [`completion::parse_tool_input_with_completion`] alongside the
//! existing [`parse_tool_input_reported`] so a `progress` stream
//! chunk can carry `args_so_far` for the partial.
//!
//! The structural close is a deliberately small surface: it
//! closes open strings / objects / arrays and drops trailing
//! commas. Escape correction, backslash doubling, and control-
//! character escaping stay in [`repair_json`].

use serde_json::Value;

pub mod completion;

/// Characters JSON allows after a backslash inside a string literal.
const VALID_ESCAPES: [char; 9] = ['"', '\\', '/', 'b', 'f', 'n', 'r', 't', 'u'];

/// Lowercase hex digits used to write `\uXXXX` control-character
/// escapes (matches `JSON.stringify`).
const HEX_DIGITS: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e',
    'f',
];

/// Characters of the salvaged payload kept in the
/// [`parse_tool_input_logged`] `WARN` line.
const LOGGED_PAYLOAD_CHARS: usize = 200;

/// How a tool-call argument string was finalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolArgsQuality {
    /// The accumulated text was valid JSON as-is.
    Strict,
    /// The strict parse failed, [`repair_json`] changed the text,
    /// and the repaired text parsed.
    Repaired,
    /// Neither the raw nor the repaired text parsed: the raw text is
    /// preserved verbatim as [`Value::String`].
    RawFallback,
}

/// Repair a payload whose string literals are malformed.
///
/// Two rewrites, both confined to string-literal content:
///
/// - a raw control character (`U+0000`–`U+001F`) becomes its JSON
///   escape — `\n`, `\r`, `\t`, `\b`, `\f`, otherwise `\uXXXX`
/// - a backslash that starts no valid escape is doubled, so the
///   character the model meant survives (`"\q"` → `"\\q"` → `\q`)
///
/// Everything else is copied byte for byte: valid JSON is returned
/// unchanged, including its exact escape spelling (`\u00e9`, `\/`,
/// surrogate pairs), and text outside string literals is never
/// rewritten. A structurally broken payload (truncated object, stray
/// token) is *not* repaired by this function.
#[must_use]
pub fn repair_json(raw: &str) -> String {
    let mut repaired = String::with_capacity(raw.len());
    let mut in_string = false;
    let mut chars = raw.chars().peekable();
    while let Some(current) = chars.next() {
        if !in_string {
            if current == '"' {
                in_string = true;
            }
            repaired.push(current);
            continue;
        }
        match current {
            '"' => {
                in_string = false;
                repaired.push(current);
            }
            '\\' => push_escape(&mut repaired, &mut chars),
            control if is_control_character(control) => {
                push_control_escape(&mut repaired, control);
            }
            other => repaired.push(other),
        }
    }
    repaired
}

/// Parse tool-call arguments, reporting how much recovery the text
/// needed.
///
/// `Strict` when the payload already parses, `Repaired` when
/// [`repair_json`] recovered it, `RawFallback` — the raw text
/// preserved as [`Value::String`] — when neither parses. Empty and
/// whitespace-only input yields `{}` with quality `Strict`: the
/// empty object is the defined "no arguments" representation a tool
/// call with no `input` must carry, not a salvage.
#[must_use]
pub fn parse_tool_input_reported(raw: &str) -> (Value, ToolArgsQuality) {
    if raw.trim().is_empty() {
        let empty = Value::Object(serde_json::Map::new());
        return (empty, ToolArgsQuality::Strict);
    }
    if let Ok(value) = serde_json::from_str(raw) {
        return (value, ToolArgsQuality::Strict);
    }
    let repaired = repair_json(raw);
    if repaired != raw
        && let Ok(value) = serde_json::from_str(&repaired)
    {
        return (value, ToolArgsQuality::Repaired);
    }
    (Value::String(raw.to_string()), ToolArgsQuality::RawFallback)
}

/// Best-effort parse of a tool-use argument string into a JSON value.
///
/// Thin wrapper over [`parse_tool_input_reported`] for callers that do
/// not care how the payload was recovered: valid JSON parses, a
/// repairable payload is repaired, empty input becomes `{}`, and
/// anything else is handed back as [`Value::String`] holding the raw
/// text. Stream processors and [`crate::BlockAssembler`] use
/// [`parse_tool_input_logged`] instead, so a salvage is visible in
/// the logs.
#[must_use]
pub fn parse_tool_input(raw: &str) -> Value {
    parse_tool_input_reported(raw).0
}

/// Finalization entry point for the provider stream processors and
/// [`crate::BlockAssembler`]: [`parse_tool_input_reported`] plus a
/// `WARN` log whenever the payload needed repair or could not be
/// parsed at all. A salvaged or dropped argument list must show up in
/// production logs, not just in the resulting tool call.
#[must_use]
pub fn parse_tool_input_logged(raw: &str, tool_name: &str) -> Value {
    let (value, quality) = parse_tool_input_reported(raw);
    match quality {
        ToolArgsQuality::Strict => {}
        ToolArgsQuality::Repaired => tracing::warn!(
            target: "synthia_provider::json_repair",
            tool = tool_name,
            quality = "repaired",
            raw_len = raw.len(),
            raw = %logged_payload(raw),
            "tool-call arguments were not valid JSON; recovered by repair"
        ),
        ToolArgsQuality::RawFallback => tracing::warn!(
            target: "synthia_provider::json_repair",
            tool = tool_name,
            quality = "raw-fallback",
            raw_len = raw.len(),
            raw = %logged_payload(raw),
            "tool-call arguments are not JSON; passing the raw text"
        ),
    }
    value
}

/// Append the escape sequence for the backslash behind the cursor.
///
/// The cursor is left on the character *after* a valid escape (or on
/// the offending character when the escape is invalid, so it is
/// processed as ordinary string content next).
fn push_escape(
    out: &mut String,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) {
    let Some(next) = chars.peek().copied() else {
        // Lone trailing backslash: double it so the literal stays
        // open instead of swallowing the closing quote.
        out.push_str("\\\\");
        return;
    };
    if next == 'u' && has_hex_escape(chars) {
        out.push('\\');
        for _ in 0..5 {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
        }
        return;
    }
    if VALID_ESCAPES.contains(&next) {
        out.push('\\');
        out.push(next);
        chars.next();
        return;
    }
    out.push_str("\\\\");
}

/// True when the `u` behind the cursor is followed by four hex digits,
/// i.e. `\uXXXX` is a complete escape that must stay byte-identical.
fn has_hex_escape(chars: &std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut lookahead = chars.clone();
    lookahead.next();
    let mut seen = 0;
    for digit in lookahead.take(4) {
        if !digit.is_ascii_hexdigit() {
            return false;
        }
        seen += 1;
    }
    seen == 4
}

/// True for the characters JSON forbids raw inside a string literal.
fn is_control_character(c: char) -> bool {
    ('\u{0}'..='\u{1f}').contains(&c)
}

/// Append the JSON escape for a control character.
fn push_control_escape(out: &mut String, control: char) {
    match control {
        '\u{8}' => out.push_str("\\b"),
        '\u{c}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        other => {
            out.push_str("\\u");
            let code = other as u32;
            for shift in [12, 8, 4, 0] {
                let nibble = (code >> shift) & 0xf;
                out.push(HEX_DIGITS[nibble as usize]);
            }
        }
    }
}

/// The head of the salvaged payload, for the `WARN` line: long enough
/// to recognize the call, short enough not to flood the log.
fn logged_payload(raw: &str) -> String {
    if raw.chars().count() <= LOGGED_PAYLOAD_CHARS {
        return raw.to_string();
    }
    let kept: String = raw.chars().take(LOGGED_PAYLOAD_CHARS).collect();
    format!("{kept}...")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Valid JSON must come back byte-identical — the repair step runs
    /// only after a strict parse has already failed, and it must not
    /// rewrite escape spellings or non-ASCII content it did not
    /// target.
    #[test]
    fn repair_json_returns_valid_json_byte_identical() {
        for raw in [
            r#"{"cmd": "ls", "path": "/tmp"}"#,
            r#"{"regex": "\\d+\\s", "newline": "a\nb"}"#,
            r#"{"unicode": "\u00e9", "slash": "\/", "quote": "\""}"#,
            r#"{"emoji": "😀", "nested": {"a": [1, 2, {"b": null}]}}"#,
            r#"[]"#,
            r#"{"empty": {}}"#,
        ] {
            assert_eq!(
                repair_json(raw),
                raw,
                "valid JSON must be returned unchanged: {raw}"
            );
        }
    }

    /// A literal newline / tab inside a string literal is malformed
    /// JSON, and the escape written by the repair must carry the same
    /// value the model meant.
    #[test]
    fn repair_json_escapes_raw_control_characters() {
        let raw = "{\"text\": \"line one\nline two\ttabbed\"}";
        assert!(
            serde_json::from_str::<Value>(raw).is_err(),
            "raw control characters must be invalid input"
        );
        let repaired = repair_json(raw);
        assert_eq!(repaired, r#"{"text": "line one\nline two\ttabbed"}"#);
        let value: Value =
            serde_json::from_str(&repaired).expect("repaired JSON parses");
        assert_eq!(value["text"], json!("line one\nline two\ttabbed"));
    }

    /// Control characters without a short escape form are written as
    /// `\uXXXX`.
    #[test]
    fn repair_json_writes_unicode_escapes_for_unmapped_controls() {
        let raw = "{\"a\": \"one\u{1}two\u{1f}\"}";
        let repaired = repair_json(raw);
        assert_eq!(repaired, r#"{"a": "one\u0001two\u001f"}"#);
        let value: Value =
            serde_json::from_str(&repaired).expect("repaired JSON parses");
        assert_eq!(value["a"], json!("one\u{1}two\u{1f}"));
    }

    /// A backslash before an invalid escape character is doubled, so
    /// the intended character survives instead of failing the parse.
    #[test]
    fn repair_json_doubles_invalid_escape_backslashes() {
        let raw = r#"{"pattern": "\q\d+"}"#;
        assert!(serde_json::from_str::<Value>(raw).is_err());
        let repaired = repair_json(raw);
        assert_eq!(repaired, r#"{"pattern": "\\q\\d+"}"#);
        let value: Value =
            serde_json::from_str(&repaired).expect("repaired JSON parses");
        assert_eq!(value["pattern"], json!(r"\q\d+"));
    }

    /// Backslashes outside string literals are not the repair's
    /// business: structural breakage stays broken so the caller can
    /// fall back to the raw text instead of believing a mangled
    /// payload.
    #[test]
    fn repair_json_leaves_text_outside_strings_untouched() {
        let raw = r#"{"a": 1} trailing \q"#;
        assert_eq!(repair_json(raw), raw);
        let truncated = r#"{"cmd": "ls""#;
        assert_eq!(repair_json(truncated), truncated);
    }

    /// `Strict` is reachable for valid JSON, `Repaired` for both
    /// salvable malformations, and each recovers the intended value.
    #[test]
    fn reported_quality_distinguishes_strict_from_repaired() {
        let (value, quality) = parse_tool_input_reported(r#"{"cmd": "ls"}"#);
        assert_eq!(quality, ToolArgsQuality::Strict);
        assert_eq!(value, json!({"cmd": "ls"}));

        let (value, quality) =
            parse_tool_input_reported("{\"cmd\": \"echo one\necho two\"}");
        assert_eq!(quality, ToolArgsQuality::Repaired);
        assert_eq!(value["cmd"], json!("echo one\necho two"));

        let (value, quality) =
            parse_tool_input_reported(r#"{"pattern": "\q"}"#);
        assert_eq!(quality, ToolArgsQuality::Repaired);
        assert_eq!(value["pattern"], json!(r"\q"));
    }

    /// `RawFallback` is reachable and preserves the raw text verbatim
    /// (the same contract the pre-R33 fallback had, now reported).
    #[test]
    fn raw_fallback_preserves_the_raw_text() {
        for raw in [r#"{"cmd": "ls""#, "not json at all"] {
            let (value, quality) = parse_tool_input_reported(raw);
            assert_eq!(quality, ToolArgsQuality::RawFallback);
            assert_eq!(value, Value::String(raw.to_string()));
        }
    }

    /// Empty / whitespace-only argument text is the "no arguments"
    /// case, not a salvage.
    #[test]
    fn empty_input_yields_empty_object_with_strict_quality() {
        for raw in ["", "   ", "\t\n  "] {
            let (value, quality) = parse_tool_input_reported(raw);
            assert_eq!(quality, ToolArgsQuality::Strict);
            assert!(value.is_object());
            assert!(value.as_object().expect("object").is_empty());
        }
    }

    /// The public wrapper keeps the contract its callers already rely
    /// on: valid JSON is parsed, and only parse failures fall back.
    #[test]
    fn wrapper_keeps_the_legacy_fallbacks() {
        assert_eq!(parse_tool_input("42"), json!(42));
        assert_eq!(parse_tool_input(r#""hello""#), json!("hello"));
        assert_eq!(parse_tool_input("[1, 2, 3]"), json!([1, 2, 3]));
        assert_eq!(
            parse_tool_input("not json at all"),
            json!("not json at all")
        );
        assert!(parse_tool_input("  ").is_object());
    }

    /// The `WARN` line carries the tool name and the salvaged payload,
    /// truncated so a pathological argument list cannot flood the log.
    #[test]
    fn logged_payload_truncates_long_argument_text() {
        let short = r#"{"cmd": "ls"}"#;
        assert_eq!(logged_payload(short), short);
        let long = "x".repeat(LOGGED_PAYLOAD_CHARS + 5);
        let logged = logged_payload(&long);
        assert_eq!(logged.chars().count(), LOGGED_PAYLOAD_CHARS + 3);
        assert!(logged.ends_with("..."));
    }
}
