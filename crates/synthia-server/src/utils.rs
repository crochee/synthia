//! Common utility functions
//!
//! Provides shared helper functions used across the server.

use rmcp::model::{
    Content,
    RawTextContent,
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
};

use crate::skill::SkillInfo;

/// Create a user message from text content
pub fn create_user_message(text: String) -> SamplingMessage {
    SamplingMessage {
        role: Role::User,
        content: SamplingContent::Single(SamplingMessageContent::Text(
            RawTextContent { text, meta: None },
        )),
        meta: None,
    }
}

/// Extract text content from a SamplingMessage
pub fn extract_text(msg: &SamplingMessage) -> Option<String> {
    match &msg.content {
        SamplingContent::Single(c) => {
            if let SamplingMessageContent::Text(t) = c {
                Some(t.text.clone())
            } else {
                None
            }
        }
        SamplingContent::Multiple(cs) => {
            let texts: Vec<String> = cs
                .iter()
                .filter_map(|c| {
                    if let SamplingMessageContent::Text(t) = c {
                        Some(t.text.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join(""))
            }
        }
    }
}

/// Extract text content from CallToolResult content
pub fn extract_text_content(content: &[Content]) -> String {
    content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
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
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}
