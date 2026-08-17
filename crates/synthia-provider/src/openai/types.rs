//! Wire types for the OpenAI Chat Completions / Embeddings API.
//!
//! Visibility is crate-private because callers should go through
//! [`super::provider::OpenAICompatibleProvider`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(super) struct OpenAIRequest {
    pub(super) model: String,
    pub(super) messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<usize>,
    pub(super) stream: bool,
    pub(super) store: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) extra_body:
        Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning_split: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct OpenAIMessage {
    pub(super) role: String,
    #[serde(serialize_with = "serialize_content")]
    #[serde(deserialize_with = "deserialize_content")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) content: Option<Vec<OpenAIContentPart>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_calls: Option<Vec<OpenAIToolUse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
    #[serde(default, rename = "reasoning_content")]
    pub(super) reasoning_content: Option<String>,
    #[serde(default, rename = "reasoning")]
    pub(super) reasoning: Option<String>,
}

pub(super) fn serialize_content<S>(
    content: &Option<Vec<OpenAIContentPart>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match content {
        None => serializer.serialize_none(),
        Some(vec) if vec.is_empty() => serializer.serialize_none(),
        Some(vec)
            if vec.len() == 1
                && let OpenAIContentPart::Text { text } = &vec[0] =>
        {
            serializer.serialize_str(text)
        }
        Some(vec) => vec.serialize(serializer),
    }
}

pub(super) fn deserialize_content<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<OpenAIContentPart>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum ContentStringOrArray {
        Null(()),
        String(String),
        Array(Vec<OpenAIContentPart>),
    }

    let content = ContentStringOrArray::deserialize(deserializer)?;
    match content {
        ContentStringOrArray::Null(()) => Ok(None),
        ContentStringOrArray::String(s) => {
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(vec![OpenAIContentPart::Text { text: s }]))
            }
        }
        ContentStringOrArray::Array(arr) => Ok(Some(arr)),
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(super) enum OpenAIContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    #[serde(rename = "input_audio")]
    InputAudio {
        url: String,
        mime: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
    #[serde(rename = "tool_call")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        id: String,
        content: String,
        is_error: bool,
    },
    #[serde(rename = "reasoning")]
    Reasoning { text: String },
}

#[derive(Debug, Serialize)]
pub(super) struct OpenAITool {
    pub(super) r#type: String,
    pub(super) function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenAIFunction {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct OpenAIToolUse {
    pub(super) id: String,
    pub(super) r#type: String,
    pub(super) function: OpenAIToolUseFunction,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct OpenAIToolUseFunction {
    pub(super) name: String,
    pub(super) arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct OpenAIResponse {
    pub(super) id: String,
    pub(super) model: String,
    pub(super) choices: Vec<OpenAIChoice>,
    pub(super) usage: OpenAIUsage,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct OpenAIChoice {
    pub(super) message: OpenAIMessage,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct OpenAIUsage {
    pub(super) prompt_tokens: usize,
    pub(super) completion_tokens: usize,
    pub(super) total_tokens: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenAIEmbeddingRequest {
    pub(super) model: String,
    pub(super) input: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAIEmbeddingResponse {
    pub(super) data: Vec<OpenAIEmbeddingData>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAIEmbeddingData {
    pub(super) embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    // -- OpenAIContentPart ----------------------------------------

    /// `OpenAIContentPart::Text` MUST serialize with `"type": "text"`.
    #[test]
    fn content_part_text_serializes_with_type_tag() {
        let part = OpenAIContentPart::Text {
            text: "hi".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""text":"hi""#));
    }

    /// `OpenAIContentPart::ImageUrl` MUST use snake_case wire tag.
    #[test]
    fn content_part_image_url_serializes_with_type_tag() {
        let part = OpenAIContentPart::ImageUrl {
            url: "https://x/i.png".to_string(),
            detail: Some("high".to_string()),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains(r#""type":"image_url""#));
        assert!(json.contains(r#""url":"https://x/i.png""#));
        assert!(json.contains(r#""detail":"high""#));
    }

    /// `OpenAIContentPart::ImageUrl` MUST omit `detail` when None.
    #[test]
    fn content_part_image_url_omits_none_detail() {
        let part = OpenAIContentPart::ImageUrl {
            url: "https://x/i.png".to_string(),
            detail: None,
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(!json.contains("detail"));
    }

    /// `OpenAIContentPart::InputAudio` MUST serialize correctly.
    #[test]
    fn content_part_input_audio_serializes() {
        let part = OpenAIContentPart::InputAudio {
            url: "https://x/a.mp3".to_string(),
            mime: "audio/mpeg".to_string(),
            format: Some("mp3".to_string()),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains(r#""type":"input_audio""#));
        assert!(json.contains(r#""mime":"audio/mpeg""#));
    }

    /// `OpenAIContentPart::ToolUse` MUST serialize correctly.
    #[test]
    fn content_part_tool_use_serializes() {
        let part = OpenAIContentPart::ToolUse {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            input: json!({"path": "/x"}),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains(r#""type":"tool_call""#));
        assert!(json.contains(r#""name":"read_file""#));
    }

    /// `OpenAIContentPart::ToolResult` MUST serialize correctly.
    #[test]
    fn content_part_tool_result_serializes() {
        let part = OpenAIContentPart::ToolResult {
            id: "call_1".to_string(),
            content: "ok".to_string(),
            is_error: false,
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains(r#""type":"tool_result""#));
        assert!(json.contains(r#""is_error":false"#));
    }

    /// `OpenAIContentPart::Reasoning` MUST serialize correctly.
    #[test]
    fn content_part_reasoning_serializes() {
        let part = OpenAIContentPart::Reasoning {
            text: "thinking...".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains(r#""type":"reasoning""#));
    }

    // -- OpenAIMessage content serialize --------------------------

    /// `serialize_content` MUST flatten single Text parts to a bare
    /// string (NOT an array).
    #[test]
    fn serialize_content_single_text_flattens() {
        let msg = OpenAIMessage {
            role: "user".to_string(),
            content: Some(vec![OpenAIContentPart::Text {
                text: "hi".to_string(),
            }]),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
            reasoning: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""content":"hi""#));
        assert!(!json.contains(r#""content":[{"#));
    }

    /// `serialize_content` MUST serialize multi-part content as
    /// an array.
    #[test]
    fn serialize_content_multi_part_is_array() {
        let msg = OpenAIMessage {
            role: "user".to_string(),
            content: Some(vec![
                OpenAIContentPart::Text {
                    text: "a".to_string(),
                },
                OpenAIContentPart::Text {
                    text: "b".to_string(),
                },
            ]),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
            reasoning: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""content":[{"#));
    }

    /// `serialize_content` MUST omit the `content` key when None.
    #[test]
    fn serialize_content_none_is_omitted() {
        let msg = OpenAIMessage {
            role: "user".to_string(),
            content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
            reasoning: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        // Pin the actual behavior: skip_serializing_if drops the
        // None content entirely (not even "content": null).
        assert!(!json.contains(r#""content""#));
    }

    /// `serialize_content` MUST treat empty Vec as `null`.
    #[test]
    fn serialize_content_empty_vec_is_null() {
        let msg = OpenAIMessage {
            role: "user".to_string(),
            content: Some(vec![]),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
            reasoning: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""content":null"#));
    }

    // -- OpenAIMessage content deserialize ------------------------

    /// `deserialize_content` MUST accept a bare string and wrap in
    /// a single Text part.
    #[test]
    fn deserialize_content_string_wraps_text() {
        let json = r#"{"role":"user","content":"hi"}"#;
        let msg: OpenAIMessage = serde_json::from_str(json).unwrap();
        let parts = msg.content.unwrap();
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            OpenAIContentPart::Text { text } => assert_eq!(text, "hi"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    /// `deserialize_content` MUST treat empty string as `None`.
    #[test]
    fn deserialize_content_empty_string_is_none() {
        let json = r#"{"role":"user","content":""}"#;
        let msg: OpenAIMessage = serde_json::from_str(json).unwrap();
        assert!(msg.content.is_none());
    }

    /// `deserialize_content` MUST accept explicit null as `None`.
    #[test]
    fn deserialize_content_null_is_none() {
        let json = r#"{"role":"user","content":null}"#;
        let msg: OpenAIMessage = serde_json::from_str(json).unwrap();
        assert!(msg.content.is_none());
    }

    /// `deserialize_content` MUST accept an array.
    #[test]
    fn deserialize_content_array_passthrough() {
        let json = r#"{"role":"user","content":[{"type":"text","text":"a"}]}"#;
        let msg: OpenAIMessage = serde_json::from_str(json).unwrap();
        let parts = msg.content.unwrap();
        assert_eq!(parts.len(), 1);
    }

    // -- OpenAITool + Function -------------------------------------

    /// `OpenAITool` MUST serialize with `type = "function"`.
    #[test]
    fn openai_tool_serializes_with_function_type() {
        let tool = OpenAITool {
            r#type: "function".to_string(),
            function: OpenAIFunction {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                parameters: json!({"type": "object"}),
            },
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains(r#""type":"function""#));
        assert!(json.contains(r#""name":"read_file""#));
    }

    // -- OpenAIToolUse ----------------------------------------------

    /// `OpenAIToolUse` MUST round-trip through JSON.
    #[test]
    fn openai_tool_use_round_trips() {
        let original = OpenAIToolUse {
            id: "call_1".to_string(),
            r#type: "function".to_string(),
            function: OpenAIToolUseFunction {
                name: "bash".to_string(),
                arguments: r#"{"cmd":"ls"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: OpenAIToolUse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "call_1");
        assert_eq!(parsed.function.name, "bash");
    }

    // -- OpenAIUsage + Response -------------------------------------

    /// `OpenAIUsage` MUST round-trip through JSON.
    #[test]
    fn openai_usage_round_trips() {
        let original = OpenAIUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: OpenAIUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.prompt_tokens, 10);
        assert_eq!(parsed.completion_tokens, 20);
        assert_eq!(parsed.total_tokens, 30);
    }

    /// `OpenAIResponse` MUST round-trip through JSON with all fields.
    #[test]
    fn openai_response_round_trips() {
        let original = OpenAIResponse {
            id: "resp_1".to_string(),
            model: "gpt-4".to_string(),
            choices: vec![OpenAIChoice {
                message: OpenAIMessage {
                    role: "assistant".to_string(),
                    content: Some(vec![OpenAIContentPart::Text {
                        text: "hi".to_string(),
                    }]),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                    reasoning: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: OpenAIUsage {
                prompt_tokens: 5,
                completion_tokens: 2,
                total_tokens: 7,
            },
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: OpenAIResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "resp_1");
        assert_eq!(parsed.choices.len(), 1);
        assert_eq!(parsed.choices[0].finish_reason, Some("stop".to_string()));
    }

    // -- OpenAIEmbeddingRequest/Response ---------------------------

    /// `OpenAIEmbeddingRequest` MUST serialize correctly.
    #[test]
    fn openai_embedding_request_serializes() {
        let req = OpenAIEmbeddingRequest {
            model: "text-embedding-3-small".to_string(),
            input: vec!["hello".to_string()],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""model":"text-embedding-3-small""#));
        assert!(json.contains(r#""input":["hello"]"#));
    }

    /// `OpenAIEmbeddingData` MUST deserialize f32 arrays.
    #[test]
    fn openai_embedding_data_deserializes() {
        let json = r#"{"embedding":[0.1,0.2,0.3]}"#;
        let data: OpenAIEmbeddingData = serde_json::from_str(json).unwrap();
        assert_eq!(data.embedding.len(), 3);
        assert!((data.embedding[0] - 0.1).abs() < 1e-6);
    }

    /// `OpenAIEmbeddingResponse` MUST deserialize multiple data points.
    #[test]
    fn openai_embedding_response_deserializes_multiple() {
        let json =
            r#"{"data":[{"embedding":[1.0,2.0]},{"embedding":[3.0,4.0]}]}"#;
        let parsed: OpenAIEmbeddingResponse =
            serde_json::from_str(json).unwrap();
        assert_eq!(parsed.data.len(), 2);
        assert_eq!(parsed.data[0].embedding, vec![1.0, 2.0]);
        assert_eq!(parsed.data[1].embedding, vec![3.0, 4.0]);
    }

    // -- OpenAIRequest ----------------------------------------------

    /// `OpenAIRequest` MUST always emit `stream` and `store` fields
    /// (no skip_serializing_if on them).
    #[test]
    fn openai_request_emits_stream_and_store() {
        let req = OpenAIRequest {
            model: "gpt-4".to_string(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            temperature: None,
            max_tokens: None,
            stream: false,
            store: false,
            extra_body: None,
            reasoning_split: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""stream":false"#));
        assert!(json.contains(r#""store":false"#));
    }

    /// `OpenAIRequest` MUST omit `None` optional fields.
    #[test]
    fn openai_request_omits_none_optional_fields() {
        let req = OpenAIRequest {
            model: "gpt-4".to_string(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            temperature: None,
            max_tokens: None,
            stream: false,
            store: false,
            extra_body: None,
            reasoning_split: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("tools"));
        assert!(!json.contains("tool_choice"));
        assert!(!json.contains("temperature"));
        assert!(!json.contains("max_tokens"));
        assert!(!json.contains("extra_body"));
        assert!(!json.contains("reasoning_split"));
    }
}
