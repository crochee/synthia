//! Convert raw `&[Message]` from the provider into `Vec<TranscriptEntry>`.
//!
//! Filters out messages whose extracted content is whitespace-only (or
//! pure tool-use / tool-result / non-text content). The `is_tool` flag
//! is set when the role is `"tool"`, used by the prompt builder to
//! route tool-message token budget separately from user/assistant
//! message budget.

use synthia_provider::{Content, ContentPart, Message, Role};

use super::types::TranscriptEntry;

/// 从采样消息中收集对话记录条目
pub fn collect_transcript_entries(
    messages: &[Message],
) -> Vec<TranscriptEntry> {
    messages
        .iter()
        .filter_map(|msg| {
            let role = match &msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
                Role::Tool => "tool",
            }
            .to_string();

            let content = extract_message_content(msg);

            if content.trim().is_empty() {
                None
            } else {
                Some(TranscriptEntry {
                    role,
                    content,
                    is_tool: false,
                })
            }
        })
        .collect()
}

/// 提取消息内容
fn extract_message_content(msg: &Message) -> String {
    match &msg.content {
        Content::Single(ContentPart::Text(t)) => t.text.clone(),
        Content::Single(ContentPart::ToolUse(_)) => String::new(),
        Content::Single(ContentPart::ToolResult(_)) => String::new(),
        Content::Single(ContentPart::Image(_)) => "[Image]".into(),
        Content::Single(ContentPart::Audio(_)) => "[Audio]".into(),
        Content::Single(ContentPart::Reasoning(t)) => t.text.clone(),
        Content::Single(ContentPart::Resource(_)) => "[Resource]".into(),
        Content::Multi(contents) => contents
            .iter()
            .filter_map(|c| {
                if let ContentPart::Text(text) = c {
                    Some(text.text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
