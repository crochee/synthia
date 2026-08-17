//! The completion request / response types + [`ToolChoice`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    content::{Content, ContentPart},
    message::Message,
    models::TokenUsage,
    tool::ToolDefinition,
};
use crate::cache_policy::CachePolicy;

/// `messages` and `tools` are `Arc<Vec<T>>` so that reference equality
/// (`Arc::ptr_eq`) can be used to short-circuit cache policy
/// re-application and to signal that the prompt cache prefix is
/// unchanged between calls. `Arc<Vec<T>>` derefs to `Vec<T>`, so read
/// accesses (`request.tools.iter()`, `request.tools[i]`, etc.) work
/// unchanged and the wire format (JSON array) is identical to
/// `Vec<T>`.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Arc<Vec<Message>>,
    pub tools: Arc<Vec<ToolDefinition>>,
    pub tool_choice: ToolChoice,
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
    pub stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub extra_body:
        Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_policy: Option<CachePolicy>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub enum ToolChoice {
    #[default]
    Auto,
    None,
    Required,
    Specific {
        name: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub model: String,
    pub content: Content,
    pub usage: TokenUsage,
    pub cached: bool,
    /// Provider's stop reason (e.g. Anthropic `end_turn`,
    /// `tool_use`, `max_tokens`, OpenAI `stop`,
    /// `tool_calls`, `length`). Optional because not all
    /// providers surface it and not all paths read it.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stop_reason: Option<String>,
}

impl Default for CompletionResponse {
    fn default() -> Self {
        Self {
            id: String::new(),
            model: String::new(),
            content: Content::Single(ContentPart::Text(
                super::content::TextContent {
                    text: String::new(),
                    cache_control: None,
                },
            )),
            usage: TokenUsage::default(),
            cached: false,
            stop_reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        cache_policy::CachePolicy,
        types::content::{Content, ContentPart, TextContent},
    };

    // -- ToolChoice ----------------------------------------------------

    /// `ToolChoice::default()` MUST be
    /// `Auto` (defer to provider; do not
    /// require or forbid tool calls).
    #[test]
    fn tool_choice_default_is_auto() {
        assert!(matches!(ToolChoice::default(), ToolChoice::Auto));
    }

    /// `ToolChoice` MUST round-trip each
    /// variant through JSON without loss.
    #[test]
    fn tool_choice_round_trips_all_four_variants() {
        for choice in [
            ToolChoice::Auto,
            ToolChoice::None,
            ToolChoice::Required,
            ToolChoice::Specific {
                name: "bash".to_string(),
            },
        ] {
            let json = serde_json::to_string(&choice).unwrap();
            let parsed: ToolChoice =
                serde_json::from_str(&json).expect("round-trip parse");
            // Use Debug for comparison since
            // ToolChoice is not PartialEq.
            assert_eq!(format!("{:?}", choice), format!("{:?}", parsed));
        }
    }

    /// `ToolChoice` MUST serialize
    /// `Specific { name }` as a tagged
    /// struct with the name field — pin
    /// so a refactor that drops the
    /// `name` field breaks loudly.
    #[test]
    fn tool_choice_specific_serializes_name_field() {
        let c = ToolChoice::Specific {
            name: "calculator".to_string(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"name\":\"calculator\""), "got: {json}");
    }

    // -- CompletionRequest -------------------------------------------

    /// `CompletionRequest::default()`
    /// MUST yield empty `model`, empty
    /// `Arc<Vec>` containers, default
    /// `ToolChoice::Auto`, and absent
    /// optional fields.
    #[test]
    fn completion_request_default_yields_all_empty_fields() {
        let r = CompletionRequest::default();
        assert_eq!(r.model, "");
        assert_eq!(r.messages.len(), 0);
        assert_eq!(r.tools.len(), 0);
        assert!(matches!(r.tool_choice, ToolChoice::Auto));
        assert!(r.temperature.is_none());
        assert!(r.max_tokens.is_none());
        assert!(r.stop_sequences.is_empty());
        assert!(r.extra_body.is_none());
        assert!(r.cache_policy.is_none());
    }

    /// `CompletionRequest` MUST round-trip
    /// every field verbatim including the
    /// optional `extra_body` and
    /// `cache_policy`.
    #[test]
    fn completion_request_round_trips_all_fields_through_json() {
        let mut extra = HashMap::new();
        extra.insert(
            "anthropic_beta".to_string(),
            serde_json::json!(["prompt-caching-2024-07-31"]),
        );
        let r = CompletionRequest {
            model: "claude-opus-4-7".to_string(),
            messages: Arc::new(vec![]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Required,
            temperature: Some(0.7),
            max_tokens: Some(1024),
            stop_sequences: vec!["STOP".to_string()],
            extra_body: Some(extra),
            cache_policy: Some(CachePolicy::default()),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"extra_body\""), "got: {json}");
        assert!(json.contains("\"cache_policy\""), "got: {json}");

        let parsed: CompletionRequest =
            serde_json::from_str(&json).expect("round-trip parse");
        assert_eq!(parsed.model, "claude-opus-4-7");
        assert_eq!(parsed.messages.len(), 0);
        assert_eq!(parsed.tools.len(), 0);
        assert!(matches!(parsed.tool_choice, ToolChoice::Required));
        assert_eq!(parsed.temperature, Some(0.7));
        assert_eq!(parsed.max_tokens, Some(1024));
        assert_eq!(parsed.stop_sequences, vec!["STOP".to_string()]);
        assert!(parsed.extra_body.is_some());
        assert!(parsed.cache_policy.is_some());
    }

    /// `CompletionRequest` with both
    /// optional fields set to None MUST
    /// omit them in the serialized JSON
    /// (`skip_serializing_if`).
    #[test]
    fn completion_request_omits_optional_fields_when_none() {
        let r = CompletionRequest::default();
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("extra_body"),
            "absent extra_body MUST NOT appear: {json}"
        );
        assert!(
            !json.contains("cache_policy"),
            "absent cache_policy MUST NOT appear: {json}"
        );
    }

    /// Old `CompletionRequest` payloads
    /// without `extra_body` or
    /// `cache_policy` MUST still deserialize
    /// (forward-compat via
    /// `#[serde(default)]`).
    #[test]
    fn completion_request_old_payload_without_optional_fields_deserializes() {
        let old_json = r#"{
            "model": "gpt-5",
            "messages": [],
            "tools": [],
            "tool_choice": "Auto",
            "temperature": null,
            "max_tokens": null,
            "stop_sequences": []
        }"#;
        let parsed: CompletionRequest =
            serde_json::from_str(old_json).expect("parse old payload");
        assert_eq!(parsed.model, "gpt-5");
        assert!(parsed.extra_body.is_none());
        assert!(parsed.cache_policy.is_none());
    }

    /// `Arc<Vec<T>>` MUST serialize as a
    /// JSON array (same wire format as
    /// `Vec<T>`). Pin so a refactor that
    /// accidentally changes the
    /// serialization breaks compatibility.
    #[test]
    fn arc_vec_serializes_as_json_array() {
        let r = CompletionRequest {
            model: "m".to_string(),
            messages: Arc::new(vec![]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            ..Default::default()
        };
        let json = serde_json::to_string(&r).unwrap();
        // messages + tools MUST serialize
        // as JSON arrays, not as Arc-internal
        // representations.
        assert!(json.contains("\"messages\":[]"), "got: {json}");
        assert!(json.contains("\"tools\":[]"), "got: {json}");
    }

    // -- CompletionResponse ------------------------------------------

    /// `CompletionResponse::default()`
    /// MUST yield empty id/model/text,
    /// zero token usage, cached=false,
    /// stop_reason=None.
    #[test]
    fn completion_response_default_yields_all_empty_fields() {
        let r = CompletionResponse::default();
        assert_eq!(r.id, "");
        assert_eq!(r.model, "");
        assert!(!r.cached);
        assert!(r.stop_reason.is_none());
        assert_eq!(r.usage.prompt_tokens, 0);
        assert_eq!(r.usage.completion_tokens, 0);
        assert_eq!(r.usage.total_tokens, 0);
    }

    /// `CompletionResponse` MUST round-trip
    /// every field verbatim including
    /// `stop_reason`.
    #[test]
    fn completion_response_round_trips_all_fields_through_json() {
        let r = CompletionResponse {
            id: "resp-1".to_string(),
            model: "claude-opus-4-7".to_string(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: "hi".to_string(),
                cache_control: None,
            })),
            usage: TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
                cache_read_tokens: None,
                cache_write_tokens: None,
                cached_prompt_tokens: None,
            },
            cached: true,
            stop_reason: Some("end_turn".to_string()),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"stop_reason\""), "got: {json}");
        assert!(json.contains("\"cached\":true"), "got: {json}");

        let parsed: CompletionResponse =
            serde_json::from_str(&json).expect("round-trip parse");
        assert_eq!(parsed.id, "resp-1");
        assert_eq!(parsed.model, "claude-opus-4-7");
        assert!(parsed.cached);
        assert_eq!(parsed.stop_reason, Some("end_turn".to_string()));
        assert_eq!(parsed.usage.total_tokens, 150);
    }

    /// `CompletionResponse` with
    /// `stop_reason = None` MUST omit the
    /// field in the serialized JSON
    /// (`skip_serializing_if`).
    #[test]
    fn completion_response_omits_stop_reason_when_none() {
        let r = CompletionResponse::default();
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("stop_reason"),
            "absent stop_reason MUST NOT appear: {json}"
        );
    }

    /// Old `CompletionResponse` payloads
    /// without `stop_reason` MUST still
    /// deserialize (forward-compat via
    /// `#[serde(default)]`).
    #[test]
    fn completion_response_old_payload_without_stop_reason_deserializes() {
        let old_json = r#"{
            "id": "resp-2",
            "model": "m",
            "content": {"Single": {"type": "text", "text": ""}},
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 2,
                "total_tokens": 3
            },
            "cached": false
        }"#;
        let parsed: CompletionResponse =
            serde_json::from_str(old_json).expect("parse old payload");
        assert_eq!(parsed.id, "resp-2");
        assert!(parsed.stop_reason.is_none());
    }
}
