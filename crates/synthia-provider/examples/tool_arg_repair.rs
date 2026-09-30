//! Wire hygiene: salvaged tool arguments and structured error bodies.
//!
//! Seam shown: what happens when the wire is not clean. Providers
//! stream tool-call arguments as text, and a model can emit a raw
//! control character inside a string literal, an invalid escape, or a
//! truncated tail — all of which `serde_json` rejects. Rather than
//! degrading silently, `synthia_provider::json_repair` salvages what it
//! can and *reports* how it parsed; `synthia_provider::error_body`
//! turns a provider's HTTP error body into a typed signal so retry
//! classification can tell a quota exhaustion from a rate limit.
//!
//! Run:
//!
//! ```text
//! cargo run -p synthia-provider --example tool_arg_repair
//! ```
//!
//! Look at: each raw argument blob below is invalid JSON, and each one
//! still yields the intended object with the quality it was recovered
//! at; then the same status (429) classifying two different ways
//! depending on what the body actually says.

use synthia_provider::{
    ToolArgsQuality,
    classify_provider_error_body,
    parse_provider_error_body,
    parse_tool_input_reported,
    repair_json,
};

/// Show one raw argument blob: what repair does to it, and what the
/// finalization path makes of it.
fn explain(label: &str, raw: &str) {
    let repaired = repair_json(raw);
    let (value, quality) = parse_tool_input_reported(raw);
    println!("-- {label}");
    println!("   raw      : {}", raw.escape_debug());
    println!("   repaired : {}", repaired.escape_debug());
    println!("   quality  : {quality:?}");
    println!("   value    : {value}");
}

fn main() {
    println!("== tool-call arguments ==");

    explain("valid JSON is untouched", r#"{"path":"src/lib.rs"}"#);

    // A model that emits a literal newline inside a string.
    explain(
        "raw control character inside a string",
        "{\"body\":\"line one\nline two\"}",
    );

    // `\q` is not a JSON escape; the backslash must be doubled.
    explain("invalid escape", r#"{"pattern":"C:\qemu\build"}"#);

    // A stream cut mid-string (the length-stop case): repair cannot
    // close an unterminated literal, so this is reported as a raw
    // fallback rather than silently completed — the length-stop guard
    // owns the decision to refuse such a call, and a half-written
    // argument must never be executed as if it were complete.
    explain("truncated tail", r#"{"query":"unclosed"#);

    // Nothing JSON-shaped at all: the raw text survives as the value.
    explain("not JSON", "plain text arguments");

    // The quality report is what makes a salvage visible in logs;
    // `parse_tool_input_logged` emits a WARN for the last three.
    let (value, quality) = parse_tool_input_reported("{\"a\":1}");
    assert_eq!(quality, ToolArgsQuality::Strict);
    assert_eq!(value["a"], 1);

    println!("\n== provider error bodies ==");

    let cases: [(&str, u16, &str); 5] = [
        (
            "openai quota exhausted",
            429,
            r#"{"error":{"message":"You exceeded your current quota","type":"insufficient_quota","code":"insufficient_quota"}}"#,
        ),
        (
            "openai context overflow",
            400,
            r#"{"error":{"message":"This model's maximum context length is 128000 tokens","type":"invalid_request_error","code":"context_length_exceeded"}}"#,
        ),
        (
            "anthropic overloaded",
            529,
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        ),
        (
            "anthropic permission",
            403,
            r#"{"type":"error","error":{"type":"permission_error","message":"Your API key does not have access to this model"}}"#,
        ),
        ("plain 429, no body", 429, ""),
    ];

    for (label, status, body) in cases {
        let parsed = parse_provider_error_body(status, body);
        let class = classify_provider_error_body(status, body);
        println!("-- {label} (HTTP {status})");
        println!(
            "   kind={:?} code={:?}",
            parsed.kind.as_deref(),
            parsed.code.as_deref()
        );
        println!(
            "   message={:?} (excerpt {} chars)",
            parsed.message.as_deref().unwrap_or("<none>"),
            parsed.body_excerpt.len()
        );
        println!("   retry class: {class:?}");
    }

    // The distinction that matters in production: both of these are
    // HTTP 429, and only one of them is worth retrying.
    assert_ne!(
        classify_provider_error_body(
            429,
            r#"{"error":{"type":"insufficient_quota"}}"#
        ),
        classify_provider_error_body(429, ""),
        "quota exhaustion and throttling must not share a class"
    );

    println!("\nTOOL-ARG-REPAIR: OK");
}
