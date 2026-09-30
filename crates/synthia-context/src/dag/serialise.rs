//! Message serialisation for prompts, cheap token estimation,
//! and token-budget chunking.

use std::collections::HashMap;

use synthia_provider::{ContentPart, Message, Role};

use super::SummaryId;

pub(super) fn message_text(msg: &Message) -> String {
    let mut out = String::new();
    for part in msg.content.iter() {
        match part {
            ContentPart::Text(tc) => out.push_str(&tc.text),
            ContentPart::ToolResult(tr) => {
                for inner in &tr.content {
                    if let ContentPart::Text(tc) = inner {
                        out.push_str(&tc.text);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub(super) fn serialise_message_for_prompt(msg: &Message) -> String {
    let prefix = match msg.role {
        Role::Tool => match extract_tool_name(msg) {
            Some(name) if !name.is_empty() => format!("[Tool Result: {name}]"),
            _ => "[Tool Result]".to_string(),
        },
        role => format!("[{}]", capitalize(role.as_str())),
    };
    let text = message_text(msg);
    if text.len() > 2000 {
        format!(
            "{}: {}... ({} chars truncated)",
            prefix,
            &text[..2000],
            text.len() - 2000
        )
    } else {
        format!("{prefix}: {text}")
    }
}

fn extract_tool_name(msg: &Message) -> Option<String> {
    for part in msg.content.iter() {
        if let ContentPart::ToolResult(tr) = part {
            return tr.tool_name.clone();
        }
    }
    None
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

pub(super) fn estimate_tokens_of(s: &str) -> usize {
    s.len() / 4 + 1
}

pub(super) fn chunk_by_tokens(
    ids: &[SummaryId],
    serialised: &HashMap<SummaryId, String>,
    budget_tokens: usize,
) -> Vec<Vec<SummaryId>> {
    let budget_chars = budget_tokens * 4;
    let mut chunks: Vec<Vec<SummaryId>> = Vec::new();
    let mut current: Vec<SummaryId> = Vec::new();
    let mut chars = 0;
    for id in ids {
        let len = serialised.get(id).map(String::len).unwrap_or(0);
        if chars + len > budget_chars && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            chars = 0;
        }
        current.push(id.clone());
        chars += len;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}
