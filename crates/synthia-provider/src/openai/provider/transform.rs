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
            // Fall back to the provider-configured model name when
            // the caller leaves `request.model` empty. Without this
            // the outgoing body carries `"model":""` and the
            // upstream returns an error.
            model: if request.model.is_empty() {
                self.model_config.name.clone()
            } else {
                request.model.clone()
            },
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
                    // OpenAI wire format requires the
                    // `arguments` field to be a JSON-encoded
                    // string that itself parses to a JSON
                    // OBJECT. `Value::to_string()` does the
                    // right thing for objects and arrays but
                    // silently produces invalid strings for
                    // `null`, `bool`, numbers, and arrays —
                    // those would later fail the downstream
                    // tool's argument schema with a confusing
                    // "unexpected token" error instead of a
                    // clear "tool arguments must be a JSON
                    // object" message.
                    //
                    // We coerce non-object inputs to "{}" —
                    // the same convention the streaming
                    // `parse_tool_input` falls back to when
                    // it cannot parse the delta string. The
                    // tool's argument parser will then surface
                    // a sensible "missing required field"
                    // error if any field is actually required.
                    let arguments = match &input {
                        serde_json::Value::Object(_) => input.to_string(),
                        serde_json::Value::Null
                        | serde_json::Value::Bool(_)
                        | serde_json::Value::Number(_)
                        | serde_json::Value::String(_)
                        | serde_json::Value::Array(_) => "{}".to_string(),
                    };
                    extracted_tool_calls.push(OpenAIToolUse {
                        id,
                        r#type: "function".to_string(),
                        function: OpenAIToolUseFunction { name, arguments },
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

#[cfg(test)]
mod tests {
    //! Tests for the OpenAI assistant-tool_calls path.
    //! The pre-existing `parse_tool_input` tests cover
    //! **streaming** tool argument reconstruction; this
    //! module covers the **request transformation** side —
    //! i.e. converting a `Message::assistant(ToolUse)`
    //! round-trip into the OpenAI wire format. The
    //! non-object `input` bug is the regression that
    //! motivated this module.
    use super::*;
    use crate::types::{Content, ContentPart, Message, ToolUse};

    fn provider() -> OpenAICompatibleProvider {
        OpenAICompatibleProvider::new(
            "http://localhost:0".to_string(),
            crate::ModelConfig {
                name: "test-model".to_string(),
                provider: "openai".to_string(),
                context_window: 4096,
                max_output_tokens: 1024,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            },
        )
    }

    fn assistant_with_tool_use(input: serde_json::Value) -> Message {
        Message {
            role: Role::Assistant,
            content: Content::Single(ContentPart::ToolUse(ToolUse {
                id: "call_1".to_string(),
                name: "get_weather".to_string(),
                input,
            })),
            ..Default::default()
        }
    }

    /// Happy path: object input must serialize to a JSON
    /// string that itself parses to an object. This is
    /// the documented OpenAI wire format.
    #[test]
    fn transform_tool_use_with_object_input_round_trips() {
        let p = provider();
        let msg = assistant_with_tool_use(serde_json::json!({
            "location": "Beijing"
        }));
        let out = p.transform_message(&msg);
        let tcs = out.tool_calls.expect("tool_calls present");
        assert_eq!(tcs.len(), 1);
        let parsed: serde_json::Value =
            serde_json::from_str(&tcs[0].function.arguments)
                .expect("arguments must be valid JSON");
        assert_eq!(parsed, serde_json::json!({"location": "Beijing"}));
    }

    /// Regression: `Value::Null` input previously produced
    /// the wire string `"null"` — invalid JSON arguments
    /// for OpenAI (must be a JSON object). Now coerced to
    /// `"{}"`, the same convention `parse_tool_input`
    /// uses on the streaming side.
    #[test]
    fn transform_tool_use_with_null_input_emits_empty_object_arguments() {
        let p = provider();
        let msg = assistant_with_tool_use(serde_json::Value::Null);
        let out = p.transform_message(&msg);
        let tcs = out.tool_calls.expect("tool_calls present");
        assert_eq!(tcs[0].function.arguments, "{}");
        let parsed: serde_json::Value =
            serde_json::from_str(&tcs[0].function.arguments)
                .expect("arguments must be valid JSON");
        assert!(parsed.is_object());
    }

    /// Regression: `Value::Bool` / number / string / array
    /// inputs all previously produced invalid OpenAI
    /// argument strings (`"true"`, `"42"`, `"\""foo\""`,
    /// `"[]"`). They are NOT JSON objects, and the
    /// downstream tool would fail with a confusing
    /// "unexpected token" parse error. Coerce all to
    /// `"{}"` so the tool's argument schema validation
    /// can surface a clean "missing required field" error.
    #[test]
    fn transform_tool_use_with_non_object_input_emits_empty_object_arguments() {
        for input in [
            serde_json::json!(true),
            serde_json::json!(42),
            serde_json::json!("foo"),
            serde_json::json!([]),
        ] {
            let p = provider();
            let msg = assistant_with_tool_use(input.clone());
            let out = p.transform_message(&msg);
            let tcs = out.tool_calls.expect("tool_calls present");
            assert_eq!(
                tcs[0].function.arguments, "{}",
                "non-object input {input} must coerce to \"{{}}\""
            );
        }
    }

    /// Empty object input (`{}`) is a valid edge case and
    /// must round-trip verbatim — NOT coerced (the
    /// `Value::Object(_)` arm preserves `to_string()`).
    #[test]
    fn transform_tool_use_with_empty_object_input_round_trips() {
        let p = provider();
        let msg = assistant_with_tool_use(serde_json::json!({}));
        let out = p.transform_message(&msg);
        let tcs = out.tool_calls.expect("tool_calls present");
        assert_eq!(tcs[0].function.arguments, "{}");
    }

    /// Non-tool_use content must NOT be pulled into
    /// `tool_calls` — only `ToolUse` content parts are
    /// extracted. A text-only assistant message has
    /// `tool_calls = None`.
    #[test]
    fn transform_assistant_text_only_has_no_tool_calls() {
        let p = provider();
        let msg = Message::assistant("hello");
        let out = p.transform_message(&msg);
        assert!(
            out.tool_calls.is_none(),
            "text-only assistant message must have tool_calls = None"
        );
    }

    /// `transform_request` has 5 contracts that no
    /// existing test pins. The `tool_choice` 4-way
    /// enum mapping (Auto/None/Required/Specific).
    /// The `model` empty fallback to
    /// `self.model_config.name`. Empty `tools` array
    /// becomes `tools: None`. `extra_body["reasoning_split"]`
    /// is hoisted to a top-level `reasoning_split`
    /// field and filtered out of `extra_body`.
    /// `extra_body` containing ONLY `reasoning_split`
    /// collapses to `None` (not an empty map).
    /// A regression in any of these would silently break
    /// requests at the OpenAI gateway (it rejects empty
    /// arrays, and `""` model names).
    use std::sync::Arc;

    fn empty_request() -> CompletionRequest {
        CompletionRequest {
            model: "".to_string(),
            messages: Arc::new(vec![]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        }
    }

    #[test]
    fn transform_request_tool_choice_auto_maps_to_string_auto() {
        let p = provider();
        let mut req = empty_request();
        req.tool_choice = ToolChoice::Auto;
        let out = p.transform_request(&req);
        assert_eq!(
            out.tool_choice.as_deref(),
            Some("auto"),
            "ToolChoice::Auto must serialize to `\"auto\"`"
        );
    }

    #[test]
    fn transform_request_tool_choice_none_maps_to_string_none() {
        let p = provider();
        let mut req = empty_request();
        req.tool_choice = ToolChoice::None;
        let out = p.transform_request(&req);
        assert_eq!(
            out.tool_choice.as_deref(),
            Some("none"),
            "ToolChoice::None must serialize to `\"none\"`"
        );
    }

    #[test]
    fn transform_request_tool_choice_required_maps_to_string_required() {
        let p = provider();
        let mut req = empty_request();
        req.tool_choice = ToolChoice::Required;
        let out = p.transform_request(&req);
        assert_eq!(
            out.tool_choice.as_deref(),
            Some("required"),
            "ToolChoice::Required must serialize to `\"required\"`"
        );
    }

    #[test]
    fn transform_request_tool_choice_specific_propagates_name() {
        let p = provider();
        let mut req = empty_request();
        req.tool_choice = ToolChoice::Specific {
            name: "get_weather".to_string(),
        };
        let out = p.transform_request(&req);
        assert_eq!(
            out.tool_choice.as_deref(),
            Some("get_weather"),
            "ToolChoice::Specific {{ name }} must propagate the function name verbatim"
        );
    }

    #[test]
    fn transform_request_empty_model_falls_back_to_provider_default() {
        let p = provider();
        let req = empty_request(); // model = ""
        let out = p.transform_request(&req);
        assert_eq!(
            out.model, "test-model",
            "empty request.model must fall back to provider.model_config.name"
        );
    }

    #[test]
    fn transform_request_nonempty_model_is_preserved() {
        let p = provider();
        let mut req = empty_request();
        req.model = "gpt-4-turbo".to_string();
        let out = p.transform_request(&req);
        assert_eq!(out.model, "gpt-4-turbo");
    }

    #[test]
    fn transform_request_empty_tools_collapses_to_none() {
        let p = provider();
        let req = empty_request(); // tools = vec![]
        let out = p.transform_request(&req);
        assert!(
            out.tools.is_none(),
            "empty tools vec MUST become tools=None (OpenAI rejects empty arrays)"
        );
    }

    #[test]
    fn transform_request_reasoning_split_extra_body_is_hoisted_and_filtered() {
        let p = provider();
        let mut req = empty_request();
        let mut eb = std::collections::HashMap::new();
        eb.insert("reasoning_split".to_string(), serde_json::json!(true));
        eb.insert("custom_flag".to_string(), serde_json::json!("keep-me"));
        req.extra_body = Some(eb);
        let out = p.transform_request(&req);
        // Hoisted to top-level.
        assert_eq!(
            out.reasoning_split,
            Some(true),
            "reasoning_split must be hoisted to top-level"
        );
        // Filtered out of extra_body.
        assert!(
            out.extra_body
                .as_ref()
                .map(|m| !m.contains_key("reasoning_split"))
                .unwrap_or(true),
            "reasoning_split MUST be filtered out of extra_body"
        );
        // Other keys pass through.
        assert_eq!(
            out.extra_body
                .as_ref()
                .and_then(|m| m.get("custom_flag"))
                .cloned(),
            Some(serde_json::json!("keep-me")),
            "non-reasoning_split extra_body keys must pass through"
        );
    }

    #[test]
    fn transform_request_extra_body_only_reasoning_split_collapses_to_none() {
        let p = provider();
        let mut req = empty_request();
        let mut eb = std::collections::HashMap::new();
        eb.insert("reasoning_split".to_string(), serde_json::json!(false));
        req.extra_body = Some(eb);
        let out = p.transform_request(&req);
        // reasoning_split hoisted.
        assert_eq!(out.reasoning_split, Some(false));
        // extra_body collapses to None — sending an
        // empty map would be a wire-shape error.
        assert!(
            out.extra_body.is_none(),
            "extra_body containing ONLY reasoning_split MUST collapse to None; got {:?}",
            out.extra_body
        );
    }
}
