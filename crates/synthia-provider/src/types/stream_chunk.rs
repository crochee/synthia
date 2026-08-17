//! The streaming types: [`StreamChunk`] (one event on the SSE
//! stream) and [`SamplingResult`] (the final aggregated response).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{content::ContentPart, models::TokenUsage, tool::ToolUse};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SamplingResult {
    pub text: String,
    pub tool_calls: Vec<ToolUse>,
    pub reasoning: String,
    /// Anthropic `signature_delta` value attached to the most recent
    /// reasoning block, propagated so the agent can preserve
    /// cross-turn reasoning continuity.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reasoning_signature: Option<String>,
    pub usage: TokenUsage,
    /// Provider's stop reason (e.g. Anthropic `end_turn`,
    /// `tool_use`, `max_tokens`, OpenAI `stop`,
    /// `tool_calls`, `length`). Optional because not all
    /// providers surface it (and not all responses have
    /// one, e.g. mid-stream usage-only updates).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stop_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub enum StreamChunk {
    Content(ContentPart),
    Usage(TokenUsage),
    Stop(String),
    ToolCallStart {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolCallDelta {
        id: String,
        arguments_delta: String,
    },
    ToolCallEnd {
        id: String,
    },
    IsDone {
        result: Box<SamplingResult>,
    },
}

impl From<ContentPart> for StreamChunk {
    fn from(part: ContentPart) -> Self {
        StreamChunk::Content(part)
    }
}

impl From<String> for StreamChunk {
    fn from(stop: String) -> Self {
        StreamChunk::Stop(stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{content::TextContent, models::TokenUsage};

    // -- SamplingResult -----------------------------------------------

    /// `SamplingResult::default()` MUST
    /// produce a fully empty result so
    /// callers can `+=` accumulate chunks
    /// without initialization.
    #[test]
    fn sampling_result_default_yields_all_empty_fields() {
        let r = SamplingResult::default();
        assert_eq!(r.text, "");
        assert!(r.tool_calls.is_empty());
        assert_eq!(r.reasoning, "");
        assert!(r.reasoning_signature.is_none());
        assert!(r.stop_reason.is_none());
        assert_eq!(r.usage.prompt_tokens, 0);
        assert_eq!(r.usage.completion_tokens, 0);
        assert_eq!(r.usage.total_tokens, 0);
    }

    /// `SamplingResult` MUST round-trip
    /// every field verbatim through JSON,
    /// including the optional
    /// `reasoning_signature` and
    /// `stop_reason`.
    #[test]
    fn sampling_result_round_trips_all_fields_through_json() {
        let r = SamplingResult {
            text: "hello".to_string(),
            tool_calls: vec![ToolUse {
                id: "c1".to_string(),
                name: "bash".to_string(),
                input: serde_json::json!({"cmd": "ls"}),
            }],
            reasoning: "I should run ls".to_string(),
            reasoning_signature: Some("sig-xyz".to_string()),
            usage: TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
                cache_read_tokens: None,
                cache_write_tokens: None,
                cached_prompt_tokens: None,
            },
            stop_reason: Some("end_turn".to_string()),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"reasoning_signature\""), "got: {json}");
        assert!(json.contains("\"stop_reason\""), "got: {json}");

        let parsed: SamplingResult =
            serde_json::from_str(&json).expect("round-trip parse");
        assert_eq!(parsed.text, "hello");
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "bash");
        assert_eq!(parsed.reasoning, "I should run ls");
        assert_eq!(parsed.reasoning_signature, Some("sig-xyz".to_string()));
        assert_eq!(parsed.usage.total_tokens, 150);
        assert_eq!(parsed.stop_reason, Some("end_turn".to_string()));
    }

    /// `SamplingResult` with both optional
    /// fields set to None MUST omit them
    /// in the serialized JSON
    /// (`skip_serializing_if`).
    #[test]
    fn sampling_result_omits_optional_fields_when_none() {
        let r = SamplingResult::default();
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("reasoning_signature"),
            "absent field MUST NOT appear: {json}"
        );
        assert!(
            !json.contains("stop_reason"),
            "absent field MUST NOT appear: {json}"
        );
    }

    /// Old `SamplingResult` payloads
    /// without `reasoning_signature` or
    /// `stop_reason` MUST still deserialize
    /// (forward-compat via
    /// `#[serde(default)]`).
    #[test]
    fn sampling_result_old_payload_without_optional_fields_deserializes() {
        let old_json = r#"{
            "text": "hello",
            "tool_calls": [],
            "reasoning": "",
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 2,
                "total_tokens": 3
            }
        }"#;
        let parsed: SamplingResult =
            serde_json::from_str(old_json).expect("parse old payload");
        assert_eq!(parsed.text, "hello");
        assert!(parsed.reasoning_signature.is_none());
        assert!(parsed.stop_reason.is_none());
        assert_eq!(parsed.usage.total_tokens, 3);
    }

    // -- From<ContentPart> + From<String> conversions -----------------

    /// `ContentPart → StreamChunk`
    /// conversion MUST wrap the part in
    /// `StreamChunk::Content`.
    #[test]
    fn from_content_part_produces_content_variant() {
        let part = ContentPart::Text(TextContent {
            text: "hello".to_string(),
            cache_control: None,
        });
        let chunk: StreamChunk = part.into();
        match chunk {
            StreamChunk::Content(c) => match c {
                ContentPart::Text(t) => assert_eq!(t.text, "hello"),
                _ => panic!("expected Text"),
            },
            _ => panic!("expected Content variant"),
        }
    }

    /// `String → StreamChunk` conversion
    /// MUST produce `StreamChunk::Stop`.
    #[test]
    fn from_string_produces_stop_variant() {
        let chunk: StreamChunk = "end_turn".to_string().into();
        match chunk {
            StreamChunk::Stop(s) => assert_eq!(s, "end_turn"),
            _ => panic!("expected Stop variant"),
        }
    }

    // -- StreamChunk variant field propagation ------------------------

    /// `ToolCallStart` MUST carry id +
    /// name + arguments verbatim.
    #[test]
    fn tool_call_start_carries_all_three_fields() {
        let chunk = StreamChunk::ToolCallStart {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"cmd": "ls"}),
        };
        match chunk {
            StreamChunk::ToolCallStart {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(name, "bash");
                assert_eq!(arguments, serde_json::json!({"cmd": "ls"}));
            }
            _ => panic!("expected ToolCallStart"),
        }
    }

    /// `ToolCallDelta` MUST carry id +
    /// arguments_delta verbatim.
    #[test]
    fn tool_call_delta_carries_id_and_arguments_delta() {
        let chunk = StreamChunk::ToolCallDelta {
            id: "call-1".to_string(),
            arguments_delta: "\"ls".to_string(),
        };
        match chunk {
            StreamChunk::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(arguments_delta, "\"ls");
            }
            _ => panic!("expected ToolCallDelta"),
        }
    }

    /// `ToolCallEnd` MUST carry id
    /// verbatim.
    #[test]
    fn tool_call_end_carries_id_verbatim() {
        let chunk = StreamChunk::ToolCallEnd {
            id: "call-1".to_string(),
        };
        match chunk {
            StreamChunk::ToolCallEnd { id } => {
                assert_eq!(id, "call-1");
            }
            _ => panic!("expected ToolCallEnd"),
        }
    }

    /// `IsDone` MUST carry the full
    /// `SamplingResult` boxed.
    #[test]
    fn is_done_carries_full_sampling_result() {
        let r = SamplingResult {
            text: "ok".to_string(),
            tool_calls: vec![],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cache_read_tokens: None,
                cache_write_tokens: None,
                cached_prompt_tokens: None,
            },
            stop_reason: Some("end_turn".to_string()),
        };
        let chunk = StreamChunk::IsDone {
            result: Box::new(r),
        };
        match chunk {
            StreamChunk::IsDone { result } => {
                assert_eq!(result.text, "ok");
                assert_eq!(result.usage.total_tokens, 15);
                assert_eq!(result.stop_reason, Some("end_turn".to_string()));
            }
            _ => panic!("expected IsDone"),
        }
    }

    /// `Usage(TokenUsage)` variant MUST
    /// carry token usage verbatim (e.g.
    /// for mid-stream usage-only updates).
    #[test]
    fn usage_chunk_carries_token_usage_verbatim() {
        let chunk = StreamChunk::Usage(TokenUsage {
            prompt_tokens: 7,
            completion_tokens: 3,
            total_tokens: 10,
            cache_read_tokens: None,
            cache_write_tokens: None,
            cached_prompt_tokens: None,
        });
        match chunk {
            StreamChunk::Usage(u) => {
                assert_eq!(u.prompt_tokens, 7);
                assert_eq!(u.total_tokens, 10);
            }
            _ => panic!("expected Usage variant"),
        }
    }
}
