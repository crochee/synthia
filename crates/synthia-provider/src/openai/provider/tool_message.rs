//! The tool-message transform method on
//! [`super::core::OpenAICompatibleProvider`]:
//!
//! - [`OpenAICompatibleProvider::transform_tool_message`] —
//!   builds the `OpenAIMessage` for a `role == "tool"`
//!   message. It walks the content looking for
//!   `ContentPart::ToolResult(tr)`, extracts the text body
//!   (joined with `\n`) and the first media part
//!   (image/audio from `tr.content`); `tool_call_id` is
//!   `msg.tool_call_id` or, as a fallback, the first
//!   `tr.tool_use_id` found in the content.

use super::{
    super::types::{OpenAIContentPart, OpenAIMessage},
    core::OpenAICompatibleProvider,
};

impl OpenAICompatibleProvider {
    pub(in crate::openai) fn transform_tool_message(
        &self,
        msg: &crate::types::Message,
    ) -> OpenAIMessage {
        fn extract_text_from_content(
            content: &[crate::types::ContentPart],
        ) -> String {
            let texts: Vec<String> = content
                .iter()
                .filter_map(|p| {
                    if let crate::types::ContentPart::Text(tc) = p {
                        Some(tc.text.clone())
                    } else {
                        None
                    }
                })
                .collect();
            texts.join("\n")
        }

        fn extract_media_parts(
            content: &[&crate::types::ContentPart],
        ) -> Vec<OpenAIContentPart> {
            content
                .iter()
                .filter_map(|p| match *p {
                    crate::types::ContentPart::Image(ic) => {
                        Some(OpenAIContentPart::ImageUrl {
                            url: ic.data.clone(),
                            detail: ic.detail.as_ref().map(|d| match d {
                                crate::types::ImageDetail::Low => {
                                    "low".to_string()
                                }
                                crate::types::ImageDetail::High => {
                                    "high".to_string()
                                }
                                crate::types::ImageDetail::Auto => {
                                    "auto".to_string()
                                }
                            }),
                        })
                    }
                    crate::types::ContentPart::Audio(ac) => {
                        Some(OpenAIContentPart::InputAudio {
                            url: ac.data.clone(),
                            mime: ac.mime_type.clone(),
                            format: ac.format.as_ref().map(|f| match f {
                                crate::types::AudioFormat::Wav => {
                                    "wav".to_string()
                                }
                                crate::types::AudioFormat::Mp3 => {
                                    "mp3".to_string()
                                }
                                crate::types::AudioFormat::Flac => {
                                    "flac".to_string()
                                }
                            }),
                        })
                    }
                    crate::types::ContentPart::ToolResult(tr) => {
                        let inner: Vec<&crate::types::ContentPart> =
                            tr.content.iter().collect();
                        let media = extract_media_parts(&inner);
                        if !media.is_empty() {
                            media.into_iter().next()
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .collect()
        }

        let content_str;
        let tool_use_id = match &msg.content {
            crate::types::Content::Single(part) => {
                if let crate::types::ContentPart::ToolResult(tr) = part {
                    content_str = extract_text_from_content(&tr.content);
                    Some(tr.tool_use_id.clone())
                } else {
                    content_str = String::new();
                    None
                }
            }
            crate::types::Content::Multi(parts) => {
                let mut ids = Vec::new();
                let mut contents = Vec::new();
                for p in parts {
                    if let crate::types::ContentPart::ToolResult(tr) = p {
                        ids.push(tr.tool_use_id.clone());
                        contents.push(extract_text_from_content(&tr.content));
                    }
                }
                content_str = contents.join("\n");
                ids.into_iter().next()
            }
        };

        let mut parts: Vec<OpenAIContentPart> = if !content_str.is_empty() {
            vec![OpenAIContentPart::Text { text: content_str }]
        } else {
            Vec::new()
        };

        let content_parts: Vec<&crate::types::ContentPart> = match &msg.content
        {
            crate::types::Content::Single(p) => vec![p],
            crate::types::Content::Multi(ps) => ps.iter().collect(),
        };
        parts.extend(extract_media_parts(&content_parts));

        OpenAIMessage {
            role: "tool".to_string(),
            content: if parts.is_empty() {
                Some(vec![OpenAIContentPart::Text {
                    text: String::new(),
                }])
            } else {
                Some(parts)
            },
            tool_calls: None,
            tool_call_id: msg.tool_call_id.clone().or(tool_use_id),
            name: msg.name.clone(),
            reasoning_content: None,
            reasoning: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Content,
        ContentPart,
        Message,
        ModelConfig,
        Role,
        ToolResult,
        openai::{
            provider::core::OpenAICompatibleProvider,
            types::{OpenAIContentPart, OpenAIMessage},
        },
        types::TextContent,
    };

    fn provider() -> OpenAICompatibleProvider {
        OpenAICompatibleProvider::new(
            "https://api.openai.com/v1".to_string(),
            ModelConfig {
                name: "gpt-4o".to_string(),
                provider: "openai".to_string(),
                context_window: 128_000,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            },
        )
    }

    /// Empty content with no tool_call_id MUST produce
    /// `role = "tool"` and an empty-text `parts` placeholder
    /// (NOT `None`).
    #[test]
    fn transform_empty_message_yields_role_tool_with_placeholder() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::text(""),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert_eq!(result.role, "tool");
        // Empty content_str + no media → vec with single empty Text part.
        match &result.content {
            Some(parts) => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    OpenAIContentPart::Text { text } => {
                        assert_eq!(text, "");
                    }
                    _ => panic!("expected Text, got {:?}", parts[0]),
                }
            }
            None => panic!("expected Some(parts)"),
        }
        assert!(result.tool_call_id.is_none());
        assert!(result.tool_calls.is_none());
        assert!(result.name.is_none());
    }

    /// `msg.tool_call_id` MUST win over `tr.tool_use_id` when both
    /// are present (explicit field overrides content-derived value).
    #[test]
    fn transform_msg_tool_call_id_overrides_content() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(ToolResult::new(
                "use-id-1", "ok",
            ))),
            tool_call_id: Some("explicit-id".to_string()),
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert_eq!(result.tool_call_id, Some("explicit-id".to_string()));
    }

    /// When `msg.tool_call_id` is None, the first
    /// `tr.tool_use_id` from a `Content::Multi` MUST be used.
    #[test]
    fn transform_falls_back_to_first_tool_use_id() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Multi(vec![
                ContentPart::ToolResult(ToolResult::new("first-id", "r1")),
                ContentPart::ToolResult(ToolResult::new("second-id", "r2")),
            ]),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert_eq!(result.tool_call_id, Some("first-id".to_string()));
    }

    /// `Content::Single(ToolResult)` MUST produce a Text part
    /// with the joined text body.
    #[test]
    fn transform_single_tool_result_joins_text_with_newline() {
        let p = provider();
        let mut tr = ToolResult::new("id", "");
        tr.content = vec![
            ContentPart::Text(TextContent {
                text: "line1".to_string(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: "line2".to_string(),
                cache_control: None,
            }),
        ];
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(tr)),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        let parts = result.content.expect("content");
        // First part is the joined text.
        match &parts[0] {
            OpenAIContentPart::Text { text } => {
                assert_eq!(text, "line1\nline2");
            }
            _ => panic!("expected Text, got {:?}", parts[0]),
        }
    }

    /// `Content::Multi` with multiple `ToolResult`s MUST join
    /// their text bodies with `\n`.
    #[test]
    fn transform_multi_tool_results_join_with_newline() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Multi(vec![
                ContentPart::ToolResult(ToolResult::new("a", "alpha")),
                ContentPart::ToolResult(ToolResult::new("b", "beta")),
            ]),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        let parts = result.content.expect("content");
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            OpenAIContentPart::Text { text } => {
                assert_eq!(text, "alpha\nbeta");
            }
            _ => panic!("expected Text"),
        }
    }

    /// `msg.name` MUST be forwarded verbatim to the wire message.
    #[test]
    fn transform_forwards_name() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::text(""),
            tool_call_id: None,
            name: Some("bash".to_string()),
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert_eq!(result.name, Some("bash".to_string()));
    }

    /// `OpenAIMessage` from `transform_tool_message` MUST have
    /// `tool_calls = None` (only assistant messages can issue
    /// tool_calls; tool messages are results).
    #[test]
    fn transform_tool_calls_is_none() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(ToolResult::new(
                "id", "x",
            ))),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert!(result.tool_calls.is_none());
    }

    /// Non-ToolResult content MUST produce an empty Text part
    /// (no `tool_use_id` fallback).
    #[test]
    fn transform_non_tool_result_content_yields_empty_text() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::Text(TextContent {
                text: "hello".to_string(),
                cache_control: None,
            })),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        let parts = result.content.expect("content");
        // No tool_use_id extracted from Text → fallback to msg.tool_call_id (None).
        assert_eq!(parts.len(), 1);
        assert_eq!(result.tool_call_id, None);
    }

    /// The output MUST be serializable to JSON (the OpenAI
    /// wire format is JSON; if it can't be serialized, the
    /// provider is broken).
    #[test]
    fn transform_output_is_json_serializable() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(ToolResult::new(
                "id", "ok",
            ))),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result: OpenAIMessage = p.transform_tool_message(&msg);
        let json = serde_json::to_string(&result).expect("serialize");
        // Pinned wire-format markers.
        assert!(json.contains("\"role\":\"tool\""), "got: {json}");
        assert!(json.contains("\"content\""), "got: {json}");
        assert!(json.contains("\"tool_call_id\":\"id\""), "got: {json}");
    }
}
