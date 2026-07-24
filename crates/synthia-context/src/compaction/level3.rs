//! Level 3: Marker-only retention.
//!
//! The lowest-fidelity compaction level: only `[call-completed: name]`
//! markers for tool call/result pairs are kept. If no tool calls
//! exist in the input, the entire conversation is collapsed to a
//! single marker counting the original message count.

use synthia_provider::{ContentPart, Message, ToolUse};

/// Level 3: marker-only retention. The returned `Vec<Message>` is a
/// new allocation containing either a single assistant message with
/// one `[call-completed: <name>]` line per tool call, or a single
/// placeholder message when the input had no tool calls.
pub fn compact_level3(messages: &[Message]) -> Vec<Message> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut markers: Vec<String> = Vec::new();

    for msg in messages {
        let tool_uses: Vec<&ToolUse> = msg
            .content
            .iter()
            .filter_map(|p| {
                if let ContentPart::ToolUse(tu) = p {
                    Some(tu)
                } else {
                    None
                }
            })
            .collect();

        for tu in &tool_uses {
            markers.push(format!("[call-completed: {}]", tu.name));
        }
    }

    if markers.is_empty() {
        // No tool calls found; return a single summary message
        return vec![Message::assistant(format!(
            "[{} messages compacted to markers]",
            messages.len()
        ))];
    }

    // Return all markers as a single message
    vec![Message::assistant(markers.join("\n"))]
}

#[cfg(test)]
mod tests {
    use synthia_provider::{Message, Role};

    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        assert!(compact_level3(&[]).is_empty());
    }

    #[test]
    fn no_tool_calls_yields_placeholder() {
        let msgs = vec![Message::user("hi"), Message::assistant("hello")];
        let out = compact_level3(&msgs);
        assert_eq!(out.len(), 1);
        let text = out[0]
            .content
            .iter()
            .filter_map(|p| {
                if let ContentPart::Text(t) = p {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(text, "[2 messages compacted to markers]");
    }

    #[test]
    fn tool_calls_become_call_completed_markers() {
        let msg = Message {
            role: Role::Assistant,
            content: synthia_provider::Content::Multi(vec![
                ContentPart::ToolUse(ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({}),
                }),
                ContentPart::ToolUse(ToolUse {
                    id: "t2".into(),
                    name: "read".into(),
                    input: serde_json::json!({}),
                }),
            ]),
            ..Default::default()
        };
        let out = compact_level3(&[msg]);
        assert_eq!(out.len(), 1);
        let text = out[0]
            .content
            .iter()
            .filter_map(|p| {
                if let ContentPart::Text(t) = p {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("[call-completed: bash]"));
        assert!(text.contains("[call-completed: read]"));
    }
}
