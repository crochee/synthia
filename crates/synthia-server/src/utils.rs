//! Common utility functions
//!
//! Provides shared helper functions used across the server.

use synthia_provider::{ContentPart, Message, Role};

use crate::skill::SkillInfo;

/// Create a user message from text content
pub fn create_user_message(text: String) -> Message {
    Message::user(text)
}

/// Extract text content from a Message
pub fn extract_text(msg: &Message) -> Option<String> {
    msg.content.extract_text()
}

/// Extract text content from Content
pub fn extract_text_content(content: &[ContentPart]) -> String {
    content
        .iter()
        .filter_map(|c| c.text().map(|t| t.to_string()))
        .collect::<Vec<_>>()
        .join("")
}

/// Extract skill names and descriptions from the tool description.
/// The description lists skills in format: "- skill-name: description"
pub fn extract_skills_from_description(description: &str) -> Vec<SkillInfo> {
    description
        .lines()
        .filter_map(|line| {
            if line.starts_with("- ") {
                let colon_pos = line.find(':')?;
                let name = line[2..colon_pos].trim().to_string();
                let desc = line[colon_pos + 1..].trim().to_string();
                if !name.is_empty() {
                    return Some(SkillInfo {
                        name,
                        description: desc,
                    });
                }
            }
            None
        })
        .collect()
}

/// Format role for API responses
pub fn format_role(role: &Role) -> &'static str {
    role.as_str()
}
