use std::sync::Arc;

use synthia_provider::{ContentPart, Message, Role, TextContent};

use crate::prompt::PromptContext;

/// Input to the agent run loop. Can contain text, images, or a mix of content parts.
#[derive(Clone, Debug)]
pub struct AgentInput {
    pub content: Vec<ContentPart>,
    pub history: Vec<Message>,
    /// Optional per-dispatch prompt manifest (tools + skills +
    /// peer agents). When `Some`,
    /// [`crate::agent::ReActAgent::run`] uses this in
    /// `prepare()` instead of the agent's own
    /// `prompt_context`. `None` keeps back-compat with callers
    /// that populate the agent's manifest once at construction.
    ///
    /// Stored behind `Arc<PromptContext>` so per-dispatch
    /// reassembly in `prepare()` does not deep-clone the skills
    /// + peer-agent lists on every turn.
    pub prompt_context: Option<Arc<PromptContext>>,
}

impl AgentInput {
    /// Create an input from plain text.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentPart::Text(TextContent {
                text: text.into(),
                cache_control: None,
            })],
            history: Vec::new(),
            prompt_context: None,
        }
    }

    /// Create an input from multiple content parts.
    pub fn multi(parts: Vec<ContentPart>) -> Self {
        Self {
            content: parts,
            history: Vec::new(),
            prompt_context: None,
        }
    }

    /// Create an input that resumes a session by replaying the given
    /// prior conversation messages and ending with a fresh user
    /// prompt.
    pub fn history(history: Vec<Message>, prompt: impl Into<String>) -> Self {
        Self {
            content: vec![ContentPart::Text(TextContent {
                text: prompt.into(),
                cache_control: None,
            })],
            history,
            prompt_context: None,
        }
    }

    /// Attach a per-dispatch prompt manifest. The next call to
    /// [`crate::agent::Agent::run`] will see this manifest in
    /// `prepare()` and use it to assemble the system prompt.
    /// Callers are expected to pass an `Arc<PromptContext>` so
    /// the manifest can be shared across dispatches without
    /// deep-cloning the underlying skills + peer-agent lists.
    pub fn with_prompt_context(mut self, ctx: Arc<PromptContext>) -> Self {
        self.prompt_context = Some(ctx);
        self
    }

    /// Convert this input into a [`Message`] with [`Role::User`].
    pub fn to_message(&self) -> Message {
        let content = if self.content.len() == 1 {
            synthia_provider::Content::Single(self.content[0].clone())
        } else {
            synthia_provider::Content::Multi(self.content.clone())
        };
        Message {
            role: Role::User,
            content,
            tool_call_id: None,
            name: None,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_input() {
        let input = AgentInput::text("hello");
        assert_eq!(input.content.len(), 1);
        match &input.content[0] {
            ContentPart::Text(tc) => assert_eq!(tc.text, "hello"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_multi_input() {
        let parts = vec![
            ContentPart::Text(TextContent {
                text: "hello".to_string(),
                cache_control: None,
            }),
            ContentPart::Image(synthia_provider::ImageContent {
                data: "img".to_string(),
                mime_type: "image/png".to_string(),
                detail: None,
            }),
        ];
        let input = AgentInput::multi(parts);
        assert_eq!(input.content.len(), 2);
    }

    #[test]
    fn test_to_message_single() {
        let input = AgentInput::text("hello");
        let msg = input.to_message();
        assert_eq!(msg.role, Role::User);
        assert!(matches!(msg.content, synthia_provider::Content::Single(_)));
    }

    #[test]
    fn test_to_message_multi() {
        let input = AgentInput::multi(vec![
            ContentPart::Text(TextContent {
                text: "describe".to_string(),
                cache_control: None,
            }),
            ContentPart::Image(synthia_provider::ImageContent {
                data: "img".to_string(),
                mime_type: "image/png".to_string(),
                detail: None,
            }),
        ]);
        let msg = input.to_message();
        assert!(matches!(msg.content, synthia_provider::Content::Multi(_)));
    }
}
