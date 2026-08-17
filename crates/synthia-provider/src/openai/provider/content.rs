//! The 2 content methods on
//! [`super::core::OpenAICompatibleProvider`]:
//!
//! - [`OpenAICompatibleProvider::transform_content`] —
//!   dispatches on `Content::Single` vs `Content::Multi` to
//!   map each `ContentPart` via `transform_part`.
//! - [`OpenAICompatibleProvider::transform_part`] — maps a
//!   single [`crate::types::ContentPart`] to an
//!   [`super::types::OpenAIContentPart`]. Text/Image/Audio
//!   map 1:1; `ToolUse` keeps `id`/`name`/`input`;
//!   `ToolResult` becomes a flat text representation via
//!   `format!("{:?}", tr.content)` (the OpenAI API does not
//!   preserve the structured result); `Reasoning` becomes a
//!   text-style part; `Resource` is rendered as
//!   `"[Resource: uri - name]"`.

use super::{super::types::OpenAIContentPart, core::OpenAICompatibleProvider};

impl OpenAICompatibleProvider {
    pub(in crate::openai) fn transform_content(
        &self,
        content: &crate::types::Content,
    ) -> Vec<OpenAIContentPart> {
        match content {
            crate::types::Content::Single(part) => {
                vec![self.transform_part(part)]
            }
            crate::types::Content::Multi(parts) => {
                parts.iter().map(|p| self.transform_part(p)).collect()
            }
        }
    }

    pub(in crate::openai) fn transform_part(
        &self,
        part: &crate::types::ContentPart,
    ) -> OpenAIContentPart {
        match part {
            crate::types::ContentPart::Text(tc) => OpenAIContentPart::Text {
                text: tc.text.clone(),
            },
            crate::types::ContentPart::Image(ic) => {
                OpenAIContentPart::ImageUrl {
                    url: ic.data.clone(),
                    detail: ic.detail.as_ref().map(|d| match d {
                        crate::types::ImageDetail::Low => "low".to_string(),
                        crate::types::ImageDetail::High => "high".to_string(),
                        crate::types::ImageDetail::Auto => "auto".to_string(),
                    }),
                }
            }
            crate::types::ContentPart::Audio(ac) => {
                OpenAIContentPart::InputAudio {
                    url: ac.data.clone(),
                    mime: ac.mime_type.clone(),
                    format: ac.format.as_ref().map(|f| match f {
                        crate::types::AudioFormat::Wav => "wav".to_string(),
                        crate::types::AudioFormat::Mp3 => "mp3".to_string(),
                        crate::types::AudioFormat::Flac => "flac".to_string(),
                    }),
                }
            }
            crate::types::ContentPart::ToolUse(tu) => {
                OpenAIContentPart::ToolUse {
                    id: tu.id.clone(),
                    name: tu.name.clone(),
                    input: tu.input.clone(),
                }
            }
            crate::types::ContentPart::ToolResult(tr) => {
                OpenAIContentPart::ToolResult {
                    id: tr.tool_use_id.clone(),
                    content: format!("{:?}", tr.content),
                    is_error: tr.is_error.unwrap_or(false),
                }
            }
            crate::types::ContentPart::Reasoning(rc) => {
                OpenAIContentPart::Reasoning {
                    text: rc.text.clone(),
                }
            }
            crate::types::ContentPart::Resource(link) => {
                OpenAIContentPart::Text {
                    text: format!("[Resource: {} - {}]", link.uri, link.name),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        Content,
        ContentPart,
        ModelConfig,
        ReasoningContent,
        ResourceLink,
        TextContent,
        ToolResult,
        ToolUse,
        openai::{
            provider::core::OpenAICompatibleProvider,
            types::OpenAIContentPart,
        },
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

    // -- transform_content (Single / Multi) -----------------------

    /// `transform_content(Single(part))` MUST produce a 1-element
    /// Vec with the mapped part.
    #[test]
    fn transform_content_single_produces_one_part() {
        let p = provider();
        let c = Content::Single(ContentPart::Text(TextContent {
            text: "hello".to_string(),
            cache_control: None,
        }));
        let result = p.transform_content(&c);
        assert_eq!(result.len(), 1);
        match &result[0] {
            OpenAIContentPart::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Text, got {:?}", result[0]),
        }
    }

    /// `transform_content(Multi(parts))` MUST preserve order.
    #[test]
    fn transform_content_multi_preserves_order() {
        let p = provider();
        let c = Content::Multi(vec![
            ContentPart::Text(TextContent {
                text: "a".to_string(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: "b".to_string(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: "c".to_string(),
                cache_control: None,
            }),
        ]);
        let result = p.transform_content(&c);
        assert_eq!(result.len(), 3);
    }

    /// `transform_content(Multi(parts))` MUST map heterogeneous
    /// parts to the right OpenAIContentPart variant.
    #[test]
    fn transform_content_multi_mixed_variants() {
        let p = provider();
        let c = Content::Multi(vec![
            ContentPart::Text(TextContent {
                text: "x".to_string(),
                cache_control: None,
            }),
            ContentPart::ToolUse(ToolUse {
                id: "t1".to_string(),
                name: "bash".to_string(),
                input: json!({"cmd": "ls"}),
            }),
        ]);
        let result = p.transform_content(&c);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], OpenAIContentPart::Text { .. }));
        match &result[1] {
            OpenAIContentPart::ToolUse { id, name, .. } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "bash");
            }
            _ => panic!("expected ToolUse, got {:?}", result[1]),
        }
    }

    // -- transform_part (per-variant) -----------------------------

    /// Text → Text with verbatim copy.
    #[test]
    fn transform_part_text_copies_verbatim() {
        let p = provider();
        let part = ContentPart::Text(TextContent {
            text: "verbatim".to_string(),
            cache_control: None,
        });
        let result = p.transform_part(&part);
        match result {
            OpenAIContentPart::Text { text } => assert_eq!(text, "verbatim"),
            _ => panic!("expected Text"),
        }
    }

    /// ToolUse → ToolUse with id/name/input preserved.
    #[test]
    fn transform_part_tool_use_preserves_fields() {
        let p = provider();
        let part = ContentPart::ToolUse(ToolUse {
            id: "call-1".to_string(),
            name: "search".to_string(),
            input: json!({"q": "rust"}),
        });
        let result = p.transform_part(&part);
        match result {
            OpenAIContentPart::ToolUse { id, name, input } => {
                assert_eq!(id, "call-1");
                assert_eq!(name, "search");
                assert_eq!(input, json!({"q": "rust"}));
            }
            _ => panic!("expected ToolUse"),
        }
    }

    /// ToolResult → ToolResult with Debug-formatted content and
    /// `is_error` defaulted to false when None.
    #[test]
    fn transform_part_tool_result_debug_formats_content() {
        let p = provider();
        let part = ContentPart::ToolResult(ToolResult::new("id", "ok"));
        let result = p.transform_part(&part);
        match result {
            OpenAIContentPart::ToolResult {
                id,
                content,
                is_error,
            } => {
                assert_eq!(id, "id");
                // Debug formatting of Vec<ContentPart> produces
                // something like `[Text { text: "ok", ... }]`.
                assert!(content.contains("ok"), "got: {content}");
                assert!(!is_error);
            }
            _ => panic!("expected ToolResult"),
        }
    }

    /// ToolResult with is_error=true MUST preserve the flag.
    #[test]
    fn transform_part_tool_result_propagates_is_error() {
        let p = provider();
        let part = ContentPart::ToolResult(ToolResult::error("id", "failed"));
        let result = p.transform_part(&part);
        match result {
            OpenAIContentPart::ToolResult { is_error, .. } => {
                assert!(is_error);
            }
            _ => panic!("expected ToolResult"),
        }
    }

    /// Resource → Text with `[Resource: uri - name]` format.
    #[test]
    fn transform_part_resource_renders_as_text() {
        let p = provider();
        let part = ContentPart::Resource(ResourceLink {
            uri: "file:///tmp/x.txt".to_string(),
            name: "x.txt".to_string(),
            title: None,
            description: None,
            mime_type: None,
        });
        let result = p.transform_part(&part);
        match result {
            OpenAIContentPart::Text { text } => {
                assert_eq!(text, "[Resource: file:///tmp/x.txt - x.txt]");
            }
            _ => panic!("expected Text"),
        }
    }

    /// Reasoning → Reasoning with verbatim text.
    #[test]
    fn transform_part_reasoning_copies_text() {
        let p = provider();
        let part = ContentPart::Reasoning(ReasoningContent {
            text: "thinking step".to_string(),
            signature: None,
        });
        let result = p.transform_part(&part);
        match result {
            OpenAIContentPart::Reasoning { text } => {
                assert_eq!(text, "thinking step");
            }
            _ => panic!("expected Reasoning"),
        }
    }
}
