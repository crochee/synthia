//! The 3 top-level transform methods on
//! [`super::core::OpenAICompatibleProvider`]:
//!
//! - [`OpenAICompatibleProvider::transform_request`] — the
//!   top-level dispatcher that takes a
//!   [`crate::types::CompletionRequest`] and produces a
//!   fully-formed [`super::types::OpenAIRequest`] (model +
//!   messages + tools + tool_choice + temperature +
//!   max_tokens + extra_body passthrough + reasoning_split).
//! - [`OpenAICompatibleProvider::transform_message`] — the
//!   default wrapper around
//!   `transform_message_with_options` (passes
//!   `TransformOptions::default()`).
//! - [`OpenAICompatibleProvider::transform_message_with_options`]
//!   — maps a [`crate::types::Message`] to a
//!   [`super::types::OpenAIMessage`]. For tool messages it
//!   delegates to
//!   [`super::tool_message::OpenAICompatibleProvider::transform_tool_message`];
//!   for assistant messages it extracts `tool_calls` from
//!   the content (moving them out of `content` into
//!   `tool_calls`).

use super::{
    super::types::{
        OpenAIFunction,
        OpenAIMessage,
        OpenAIRequest,
        OpenAITool,
        OpenAIToolUse,
        OpenAIToolUseFunction,
    },
    core::OpenAICompatibleProvider,
    types::TransformOptions,
};
use crate::types::{CompletionRequest, Role, ToolChoice};

impl OpenAICompatibleProvider {
    pub(in crate::openai) fn transform_request(
        &self,
        request: &CompletionRequest,
    ) -> OpenAIRequest {
        let messages: Vec<OpenAIMessage> = request
            .messages
            .iter()
            .map(|msg| self.transform_message(msg))
            .collect();

        let tools: Vec<OpenAITool> = request
            .tools
            .iter()
            .map(|t| OpenAITool {
                r#type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect();

        // Flatten extra_body into top-level fields (matching Python SDK behavior)
        let extra_reasoning_split =
            request.extra_body.as_ref().and_then(|eb| {
                eb.get("reasoning_split")
                    .and_then(serde_json::Value::as_bool)
            });

        let extra_body_passthrough: Option<
            std::collections::HashMap<String, serde_json::Value>,
        > = request
            .extra_body
            .as_ref()
            .map(|eb| {
                eb.iter()
                    .filter(|(k, _)| *k != "reasoning_split")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .filter(
                |m: &std::collections::HashMap<String, serde_json::Value>| {
                    !m.is_empty()
                },
            );

        OpenAIRequest {
            model: request.model.clone(),
            messages,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_choice: Some(match request.tool_choice {
                ToolChoice::Auto => "auto".to_string(),
                ToolChoice::None => "none".to_string(),
                ToolChoice::Required => "required".to_string(),
                ToolChoice::Specific { ref name } => name.clone(),
            }),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
            store: false,
            extra_body: extra_body_passthrough,
            reasoning_split: extra_reasoning_split,
        }
    }

    pub(super) fn transform_message(
        &self,
        msg: &crate::types::Message,
    ) -> OpenAIMessage {
        self.transform_message_with_options(msg, TransformOptions::default())
    }

    pub(in crate::openai) fn transform_message_with_options(
        &self,
        msg: &crate::types::Message,
        _opts: TransformOptions,
    ) -> OpenAIMessage {
        let role = match msg.role {
            Role::System => "system".to_string(),
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
            Role::Tool => "tool".to_string(),
        };

        // For tool messages, extract content and media
        if role == "tool" {
            return self.transform_tool_message(msg);
        }

        let content = self.transform_content(&msg.content);

        // For assistant messages, extract tool_calls from content
        let mut extracted_tool_calls: Vec<OpenAIToolUse> = Vec::new();
        let new_content: Vec<super::super::types::OpenAIContentPart> = content
            .into_iter()
            .filter_map(|part| {
                if let super::super::types::OpenAIContentPart::ToolUse {
                    id,
                    name,
                    input,
                } = part
                {
                    extracted_tool_calls.push(OpenAIToolUse {
                        id,
                        r#type: "function".to_string(),
                        function: OpenAIToolUseFunction {
                            name,
                            arguments: input.to_string(),
                        },
                    });
                    None
                } else {
                    Some(part)
                }
            })
            .collect();

        let tool_calls = if extracted_tool_calls.is_empty() {
            None
        } else {
            Some(extracted_tool_calls)
        };

        OpenAIMessage {
            role,
            content: Some(new_content),
            tool_calls,
            tool_call_id: msg.tool_call_id.clone(),
            name: msg.name.clone(),
            reasoning_content: None,
            reasoning: None,
        }
    }
}
