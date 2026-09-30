//! The searchable text of a folded surface message (shaped like
//! `synthia_provider::Message`): user and assistant text,
//! reasoning text, and the *names* of tools the assistant
//! called. Tool **results** are deliberately excluded — they are
//! machine output (file contents, command dumps), so indexing
//! them would let a query match a session that merely happened
//! to `cat` the word.

pub(super) fn message_text(message: &serde_json::Value) -> String {
    let mut out = String::new();
    // `Content` is `Single(ContentPart)` | `Multi(Vec<ContentPart>)`, and
    // each part is `{"Text": {…}}` / `{"ToolUse": {…}}` / `{"Reasoning": {…}}`
    // (serde's external tagging) — the same shapes the fold emits.
    let Some(content) = message.get("content").and_then(|c| c.as_object())
    else {
        return out;
    };
    for (variant, payload) in content {
        match variant.as_str() {
            "Single" => collect_part_text(payload, &mut out),
            "Multi" => {
                for part in payload.as_array().into_iter().flatten() {
                    collect_part_text(part, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

/// Append one `ContentPart`'s searchable text.
///
/// Handles both serializations a log can hold: the internally tagged wire
/// form (`{"type": "text", "text": …}"`) and the externally tagged form a
/// consumer might build by hand (`{"Text": {"text": …}}`). Tool *results*
/// and images contribute nothing.
pub(super) fn collect_part_text(part: &serde_json::Value, out: &mut String) {
    let Some(map) = part.as_object() else {
        return;
    };
    let field_for = |kind: &str| match kind {
        "text" | "Text" | "reasoning" | "Reasoning" => Some("text"),
        "tool_use" | "ToolUse" => Some("name"),
        _ => None,
    };
    if let Some(kind) = map.get("type").and_then(serde_json::Value::as_str) {
        if let Some(field) = field_for(kind)
            && let Some(text) = map.get(field).and_then(|t| t.as_str())
        {
            out.push_str(text);
            out.push(' ');
        }
        return;
    }
    for (variant, payload) in map {
        if let Some(field) = field_for(variant)
            && let Some(text) = payload.get(field).and_then(|t| t.as_str())
        {
            out.push_str(text);
            out.push(' ');
        }
    }
}
