// Allow `result_large_err` for the whole file: P1b added 4 hidden
// fields to every struct-form variant (frames, backtrace, source,
// and the synthetic source chain), so every `Result<_, Error>` is
// at least 128 bytes. Boxing the error would force every call site
// to `.map_err(|e| *e)` (or accept the allocation), and the existing
// API has no `Box<Error>` in the public surface. Accept the size
// cost; revisit if profiling shows it matters.
#![allow(clippy::result_large_err)]

use synthia_core::Error;

use crate::types::*;

impl CompletionRequest {
    pub fn validate(&self) -> Result<(), Error> {
        if self.model.trim().is_empty() {
            return Err(Error::validation("model must be non-empty"));
        }

        if self.messages.is_empty() {
            return Err(Error::validation(
                "messages must have at least one message",
            ));
        }

        for (i, msg) in self.messages.iter().enumerate() {
            let has_text = match &msg.content {
                Content::Single(part) => {
                    part.text().map(|t| !t.trim().is_empty()).unwrap_or(false)
                }
                Content::Multi(parts) => parts.iter().any(|p| {
                    p.text().map(|t| !t.trim().is_empty()).unwrap_or(false)
                }),
            };
            if !has_text && matches!(&msg.content, Content::Single(_)) {
                return Err(Error::validation(format!(
                    "message {} has empty content",
                    i
                )));
            }
        }

        for (i, tool) in self.tools.iter().enumerate() {
            if tool.name.trim().is_empty() {
                return Err(Error::validation(format!(
                    "tool {} has empty name",
                    i
                )));
            }
        }

        Self::validate_message_sequence(&self.messages)
    }

    fn validate_message_sequence(messages: &[Message]) -> Result<(), Error> {
        let mut last_role: Option<Role> = None;
        for msg in messages {
            if last_role == Some(Role::Tool) && msg.role != Role::User {
                return Err(Error::validation(
                    "Tool message must be followed by User message",
                ));
            }
            last_role = Some(msg.role);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn make_user_message(text: &str) -> Message {
        Message::user(text.to_string())
    }

    #[test]
    fn test_validate_empty_model() {
        let req = CompletionRequest {
            model: "".into(),
            messages: Arc::new(vec![make_user_message("hello")]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_empty_messages() {
        let req = CompletionRequest {
            model: "gpt-4".into(),
            messages: Arc::new(vec![]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_valid_request() {
        let req = CompletionRequest {
            model: "gpt-4".into(),
            messages: Arc::new(vec![make_user_message("hello")]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_validate_tool_empty_name() {
        let req = CompletionRequest {
            model: "gpt-4".into(),
            messages: Arc::new(vec![make_user_message("hello")]),
            tools: Arc::new(vec![ToolDefinition {
                name: "".into(),
                description: "test".into(),
                input_schema: serde_json::json!({}),
                cache_control: None,
            }]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_tool_message_not_followed_by_user() {
        let req = CompletionRequest {
            model: "gpt-4".into(),
            messages: Arc::new(vec![
                Message::user("hello"),
                Message {
                    role: Role::Tool,
                    content: Content::text(String::from("result")),
                    tool_call_id: Some("1".into()),
                    name: None,
                    ..Default::default()
                },
                Message {
                    role: Role::Assistant,
                    content: Content::text(String::from("ok")),
                    tool_call_id: None,
                    name: None,
                    ..Default::default()
                },
            ]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        assert!(req.validate().is_err());
    }
}
