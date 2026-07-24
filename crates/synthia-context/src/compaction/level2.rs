//! Level 2: Structured truncation.
//!
//! Truncates tool results to `input args + first line of output`,
//! preserves full user messages, and collapses plain assistant text
//! to its first line. This is the middle fidelity tier — preserves
//! enough structure for the LLM to continue reasoning, but
//! dramatically reduces token count when tool outputs are large.

use synthia_provider::{
    Content,
    ContentPart,
    Message,
    Role,
    TextContent,
    ToolUse,
};

use super::util::truncate_to_chars;
use crate::traits::{extract_message_text, extract_message_tool_uses};

/// Level 2: structured truncation. The returned `Vec<Message>` is a
/// new allocation (the input slice is not mutated in place); callers
/// that need a write-back path can persist the new messages through
/// the session writer.
pub fn compact_level2(messages: &[Message]) -> Vec<Message> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(messages.len());

    for msg in messages {
        match msg.role {
            Role::User => {
                // Preserve full user messages
                result.push(msg.clone());
            }
            Role::Assistant => {
                result.push(compact_assistant(msg));
            }
            Role::Tool => {
                if let Some(compacted) = compact_tool(msg) {
                    result.push(compacted);
                } else {
                    result.push(msg.clone());
                }
            }
            _ => {
                result.push(msg.clone());
            }
        }
    }

    result
}

// ---- Role-specific compaction helpers ----

fn compact_assistant(msg: &Message) -> Message {
    let tool_uses = extract_message_tool_uses(msg);
    let text = extract_message_text(msg);

    if !tool_uses.is_empty() {
        // Keep tool call info but truncate details
        let mut truncated_parts: Vec<ContentPart> = Vec::new();
        for tu in &tool_uses {
            let args = truncate_to_chars(&tu.input.to_string(), 80);
            truncated_parts.push(ContentPart::ToolUse(ToolUse {
                id: tu.id.clone(),
                name: tu.name.clone(),
                input: serde_json::json!({ "args_truncated": args }),
            }));
        }
        if !text.is_empty() {
            let first_line = first_n_lines(&text, 1);
            truncated_parts.push(ContentPart::Text(TextContent {
                text: first_line,
                cache_control: None,
            }));
        }
        Message {
            role: msg.role,
            content: if truncated_parts.len() == 1 {
                Content::Single(truncated_parts.remove(0))
            } else {
                Content::Multi(truncated_parts)
            },
            tool_call_id: msg.tool_call_id.clone(),
            name: msg.name.clone(),
            ..Default::default()
        }
    } else {
        // Regular assistant message: keep first line only
        let first_line = first_n_lines(&text, 1);
        Message::assistant(&first_line)
    }
}

/// Compacts a tool-role message. Returns `Some(message)` when the
/// tool result was collapsed, or `None` when the message has no
/// `ContentPart::ToolResult` part and should be passed through
/// unchanged.
fn compact_tool(msg: &Message) -> Option<Message> {
    // Extract text from either Text content or ToolResult content
    let text = extract_message_text(msg);
    let tool_result_text = msg
        .content
        .iter()
        .filter_map(|p| {
            if let ContentPart::ToolResult(tr) = p {
                Some(
                    tr.content
                        .iter()
                        .filter_map(|cp| {
                            if let ContentPart::Text(tc) = cp {
                                Some(tc.text.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            } else {
                None
            }
        })
        .next()
        .unwrap_or_default();

    let tool_result_present = msg
        .content
        .iter()
        .any(|p| matches!(p, ContentPart::ToolResult(_)));
    if !tool_result_present {
        return None;
    }

    let effective_text = if !text.is_empty() {
        text
    } else {
        tool_result_text
    };

    // Keep only first line of tool result + marker
    let first_line = if !effective_text.is_empty() {
        first_n_lines(&effective_text, 1)
    } else {
        String::from("[output truncated]")
    };
    Some(Message {
        role: msg.role,
        content: Content::Single(ContentPart::Text(TextContent {
            text: first_line,
            cache_control: None,
        })),
        tool_call_id: msg.tool_call_id.clone(),
        name: msg.name.clone(),
        ..Default::default()
    })
}

fn first_n_lines(s: &str, n: usize) -> String {
    s.lines().take(n).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use synthia_provider::{Message, Role, TextContent, ToolResult};

    use super::*;

    fn user(s: &str) -> Message {
        Message::user(s)
    }

    fn assistant(s: &str) -> Message {
        Message::assistant(s)
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(compact_level2(&[]).is_empty());
    }

    #[test]
    fn user_message_passthrough() {
        let msgs = vec![user("hello world")];
        let out = compact_level2(&msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(extract_message_text(&out[0]), "hello world");
    }

    #[test]
    fn assistant_plain_text_collapses_to_first_line() {
        let msgs = vec![assistant("first line\nsecond line\nthird")];
        let out = compact_level2(&msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(extract_message_text(&out[0]), "first line");
    }

    #[test]
    fn assistant_with_tool_use_keeps_truncated_args() {
        let msg = Message {
            role: Role::Assistant,
            content: synthia_provider::Content::Multi(vec![
                ContentPart::ToolUse(ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    // 120-char input forces truncate_to_chars(_, 80)
                    // to add the "..." suffix the test asserts on.
                    input: serde_json::json!({
                        "command": "this is a very long shell command that should easily exceed the eighty-char cap and force truncation"
                    }),
                }),
            ]),
            ..Default::default()
        };
        let out = compact_level2(&[msg]);
        assert_eq!(out.len(), 1);
        // The input JSON should be replaced with a truncated marker.
        let tool_use = out[0]
            .content
            .iter()
            .find_map(|p| {
                if let ContentPart::ToolUse(tu) = p {
                    Some(tu)
                } else {
                    None
                }
            })
            .expect("expected a ToolUse in the output");
        let args = tool_use.input.get("args_truncated").unwrap();
        let args_str = args.as_str().unwrap();
        assert!(args_str.ends_with("..."));
    }

    #[test]
    fn tool_result_collapses_to_first_line() {
        let msg = Message {
            role: Role::Tool,
            tool_call_id: Some("t1".into()),
            content: synthia_provider::Content::Multi(vec![
                ContentPart::ToolResult(ToolResult {
                    tool_use_id: "t1".into(),
                    content: vec![ContentPart::Text(TextContent {
                        text: "first line\nsecond line\nthird".into(),
                        cache_control: None,
                    })],
                    structured_content: None,
                    is_error: None,
                }),
            ]),
            ..Default::default()
        };
        let out = compact_level2(&[msg]);
        assert_eq!(out.len(), 1);
        assert_eq!(extract_message_text(&out[0]), "first line");
    }
}
