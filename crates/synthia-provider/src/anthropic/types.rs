//! Wire types for the Anthropic Messages API.
//!
//! All fields mirror the upstream JSON shape. Visibility is crate-private
//! because callers should go through [`super::provider::AnthropicProvider`].

use serde::{Deserialize, Serialize};

/// Anthropic `cache_control` hint. Serialized as `{"type": "ephemeral"}` by default.
/// Attached to the last tool / last content block / last system block to mark
/// cache prefix boundary.
///
/// `cache_namespace` carries the [`crate::cache_mark::CacheScope`] string so
/// that two different users with otherwise identical prompts produce distinct
/// `cache_control` JSON (per the cross-session cache leakage prevention
/// requirement). Anthropic ignores unknown fields server-side, but the field
/// is emitted so client-side cache-key derivation and observability can
/// namespace by user. It is only populated when the scope is non-default
/// (`CacheScope::default()` would otherwise produce byte-identical output for
/// the anonymous path).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct CacheControl {
    #[serde(rename = "type")]
    pub(super) r#type: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(super) ttl_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(super) cache_namespace: Option<String>,
}

impl Default for CacheControl {
    fn default() -> Self {
        Self {
            r#type: "ephemeral".to_string(),
            ttl_seconds: None,
            cache_namespace: None,
        }
    }
}

/// One block of a structured system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AnthropicSystemBlock {
    #[serde(rename = "type", default = "default_system_block_type")]
    pub(super) r#type: String,
    pub(super) text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(super) cache_control: Option<CacheControl>,
}

fn default_system_block_type() -> String {
    "text".to_string()
}

/// Anthropic system field. `Text` variant preserves pre-change serialization
/// (plain JSON string). `Structured` variant enables `cache_control` attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum AnthropicSystem {
    Text(String),
    Structured(Vec<AnthropicSystemBlock>),
}

#[derive(Debug, Serialize)]
pub(super) struct AnthropicRequest {
    pub(super) model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) system: Option<AnthropicSystem>,
    pub(super) messages: Vec<AnthropicMessage>,
    pub(super) max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f64>,
    pub(super) stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct AnthropicMessage {
    pub(super) role: String,
    pub(super) content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub(super) enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(deserialize_with = "deserialize_tool_result_content")]
        content: Vec<AnthropicToolResultContent>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "image")]
    Image { source: AnthropicImageSource },
    #[serde(rename = "audio")]
    Audio { source: AnthropicAudioSource },
    #[serde(rename = "document")]
    Document { source: AnthropicDocumentSource },
    #[serde(rename = "thinking")]
    ThinkingBlock {
        thinking: String,
        /// Anthropic `signature` value attached to a thinking block
        /// when the prior assistant turn included extended thinking.
        /// Required to preserve reasoning continuity across turns.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        signature: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct AnthropicToolResultContent {
    pub(super) r#type: String,
    pub(super) text: String,
}

pub(super) fn deserialize_tool_result_content<'de, D>(
    deserializer: D,
) -> Result<Vec<AnthropicToolResultContent>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum ContentOrString {
        Array(Vec<AnthropicToolResultContent>),
        String(String),
    }

    let content = ContentOrString::deserialize(deserializer)?;
    match content {
        ContentOrString::Array(arr) => Ok(arr),
        ContentOrString::String(s) => Ok(vec![AnthropicToolResultContent {
            r#type: "text".to_string(),
            text: s,
        }]),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct AnthropicImageSource {
    pub(super) r#type: String,
    pub(super) media_type: String,
    pub(super) data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct AnthropicAudioSource {
    pub(super) r#type: String,
    pub(super) media_type: String,
    pub(super) data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) format: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct AnthropicDocumentSource {
    pub(super) r#type: String,
    pub(super) media_type: String,
    pub(super) data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct AnthropicTool {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(super) cache_control: Option<CacheControl>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct AnthropicResponse {
    pub(super) id: String,
    #[serde(default)]
    pub(super) model: String,
    pub(super) content: Vec<AnthropicContentBlock>,
    pub(super) usage: AnthropicUsage,
    #[serde(default)]
    pub(super) stop_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct AnthropicUsage {
    pub(super) input_tokens: usize,
    pub(super) output_tokens: usize,
    #[serde(default)]
    pub(super) cache_read_input_tokens: Option<usize>,
    #[serde(default)]
    pub(super) cache_creation_input_tokens: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_control_default_serializes_to_ephemeral() {
        let cc = CacheControl::default();
        let json = serde_json::to_value(&cc).unwrap();
        assert_eq!(json, serde_json::json!({"type": "ephemeral"}));
    }

    #[test]
    fn cache_control_with_ttl_serializes_with_ttl_seconds() {
        let cc = CacheControl {
            r#type: "ephemeral".to_string(),
            ttl_seconds: Some(3600),
            cache_namespace: None,
        };
        let json = serde_json::to_value(&cc).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "ephemeral", "ttl_seconds": 3600})
        );
    }

    #[test]
    fn cache_control_with_namespace_serializes_cache_namespace() {
        let cc = CacheControl {
            r#type: "ephemeral".to_string(),
            ttl_seconds: None,
            cache_namespace: Some("u=alice;s=s1".to_string()),
        };
        let json = serde_json::to_value(&cc).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "ephemeral",
                "cache_namespace": "u=alice;s=s1"
            })
        );
    }

    #[test]
    fn anthropic_system_text_serializes_as_plain_string() {
        let sys = AnthropicSystem::Text("You are helpful.".to_string());
        let json = serde_json::to_value(&sys).unwrap();
        assert_eq!(json, serde_json::json!("You are helpful."));
    }

    #[test]
    fn anthropic_system_structured_serializes_as_array_with_cache_control() {
        let sys = AnthropicSystem::Structured(vec![AnthropicSystemBlock {
            r#type: "text".to_string(),
            text: "You are helpful.".to_string(),
            cache_control: Some(CacheControl::default()),
        }]);
        let json = serde_json::to_value(&sys).unwrap();
        assert_eq!(
            json,
            serde_json::json!([
                {"type": "text", "text": "You are helpful.", "cache_control": {"type": "ephemeral"}}
            ])
        );
    }

    #[test]
    fn anthropic_tool_with_cache_control_serializes() {
        let tool = AnthropicTool {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            cache_control: Some(CacheControl::default()),
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "name": "read_file",
                "description": "Read a file",
                "input_schema": {"type": "object"},
                "cache_control": {"type": "ephemeral"}
            })
        );
    }

    #[test]
    fn anthropic_tool_without_cache_control_omits_field() {
        let tool = AnthropicTool {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            cache_control: None,
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert!(json.get("cache_control").is_none());
    }

    #[test]
    fn anthropic_content_block_text_with_cache_control_serializes() {
        let block = AnthropicContentBlock::Text {
            text: "hello".to_string(),
            cache_control: Some(CacheControl::default()),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "text",
                "text": "hello",
                "cache_control": {"type": "ephemeral"}
            })
        );
    }

    #[test]
    fn anthropic_content_block_text_without_cache_control_omits_field() {
        let block = AnthropicContentBlock::Text {
            text: "hello".to_string(),
            cache_control: None,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert!(json.get("cache_control").is_none());
    }

    #[test]
    fn anthropic_request_system_text_serializes_as_plain_string() {
        let req = AnthropicRequest {
            model: "claude-3".to_string(),
            system: Some(AnthropicSystem::Text("You are helpful.".to_string())),
            messages: vec![],
            max_tokens: 100,
            tools: None,
            temperature: None,
            stream: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["system"], serde_json::json!("You are helpful."));
    }

    /// Task 1.6: when a prior assistant turn emitted extended thinking
    /// with a signature, the next request must echo the signature on
    /// the `thinking` block. Serialize the wire form that the Anthropic
    /// API expects and assert the `signature` field is preserved.
    #[test]
    fn anthropic_thinking_block_with_signature_round_trips() {
        let block = AnthropicContentBlock::ThinkingBlock {
            thinking: "Let me think about this carefully.".to_string(),
            signature: Some("sig_round_trip_123".to_string()),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], serde_json::json!("thinking"));
        assert_eq!(
            json["thinking"],
            serde_json::json!("Let me think about this carefully.")
        );
        assert_eq!(json["signature"], serde_json::json!("sig_round_trip_123"));

        // Deserialize the same JSON and confirm we recover the value.
        let parsed: AnthropicContentBlock =
            serde_json::from_value(json).unwrap();
        if let AnthropicContentBlock::ThinkingBlock {
            thinking,
            signature,
        } = parsed
        {
            assert_eq!(thinking, "Let me think about this carefully.");
            assert_eq!(signature.as_deref(), Some("sig_round_trip_123"));
        } else {
            panic!("expected ThinkingBlock variant");
        }
    }

    /// Without a signature the wire form must omit the field entirely
    /// (Anthropic rejects an empty-string signature).
    #[test]
    fn anthropic_thinking_block_without_signature_omits_field() {
        let block = AnthropicContentBlock::ThinkingBlock {
            thinking: "Just thinking out loud.".to_string(),
            signature: None,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert!(
            json.get("signature").is_none(),
            "No signature must omit the field; got: {json}"
        );
    }
}
