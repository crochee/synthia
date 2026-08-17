//! OpenAI delta response types for SSE streaming deserialization.

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaResponse {
    pub choices: Vec<OpenAIDeltaChoice>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaChoice {
    pub delta: Option<OpenAIDelta>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    /// Usage attached to a final empty choice (OpenAI `stream_options.include_usage`).
    #[serde(default)]
    pub usage: Option<OpenAIDeltaUsage>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDelta {
    pub content: Option<String>,
    #[serde(default, rename = "reasoning_content")]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAIDeltaToolUse>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaToolUse {
    pub id: Option<String>,
    pub function: OpenAIDeltaToolUseFunction,
    #[serde(default)]
    pub index: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaToolUseFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- OpenAIDeltaResponse ----------------------------------------

    /// `OpenAIDeltaResponse` MUST deserialize a multi-choice
    /// streaming chunk.
    #[test]
    fn response_parses_multiple_choices() {
        let json = r#"{
            "choices": [
                {"delta": {"content": "hello"}, "finish_reason": null},
                {"delta": {"content": "world"}, "finish_reason": null}
            ]
        }"#;
        let r: OpenAIDeltaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.choices.len(), 2);
        assert_eq!(
            r.choices[0].delta.as_ref().unwrap().content,
            Some("hello".to_string())
        );
        assert_eq!(
            r.choices[1].delta.as_ref().unwrap().content,
            Some("world".to_string())
        );
    }

    /// `OpenAIDeltaResponse` MUST deserialize an empty
    /// `choices` array (e.g. usage-only final chunks).
    #[test]
    fn response_parses_empty_choices() {
        let json = r#"{"choices": []}"#;
        let r: OpenAIDeltaResponse = serde_json::from_str(json).unwrap();
        assert!(r.choices.is_empty());
    }

    /// `OpenAIDeltaResponse` MUST reject payload missing the
    /// `choices` field (it's strictly required).
    #[test]
    fn response_rejects_missing_choices() {
        let json = r#"{}"#;
        let result: Result<OpenAIDeltaResponse, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // -- OpenAIDeltaChoice -------------------------------------------

    /// `OpenAIDeltaChoice` MUST accept `delta = null`
    /// (when the streaming choice carries only usage / finish_reason).
    #[test]
    fn choice_accepts_null_delta() {
        let json = r#"{"delta": null, "finish_reason": "stop"}"#;
        let c: OpenAIDeltaChoice = serde_json::from_str(json).unwrap();
        assert!(c.delta.is_none());
        assert_eq!(c.finish_reason, Some("stop".to_string()));
    }

    /// `OpenAIDeltaChoice::usage` MUST deserialize a final
    /// empty-delta usage chunk.
    #[test]
    fn choice_deserializes_final_usage() {
        let json = r#"{
            "delta": {},
            "finish_reason": "stop",
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        }"#;
        let c: OpenAIDeltaChoice = serde_json::from_str(json).unwrap();
        let u = c.usage.expect("usage must be Some");
        assert_eq!(u.prompt_tokens, 100);
        assert_eq!(u.completion_tokens, 50);
        assert_eq!(u.total_tokens, 150);
    }

    /// `OpenAIDeltaChoice::usage` MUST default to None when
    /// the field is omitted (no `include_usage` opt-in).
    #[test]
    fn choice_omits_usage_field_defaults_none() {
        let json = r#"{"delta": {"content": "x"}, "finish_reason": null}"#;
        let c: OpenAIDeltaChoice = serde_json::from_str(json).unwrap();
        assert!(c.usage.is_none());
    }

    // -- OpenAIDelta -------------------------------------------------

    /// `OpenAIDelta` MUST deserialize content only (the typical
    /// text streaming chunk).
    #[test]
    fn delta_deserializes_content_only() {
        let json = r#"{"content": "hello"}"#;
        let d: OpenAIDelta = serde_json::from_str(json).unwrap();
        assert_eq!(d.content, Some("hello".to_string()));
        assert!(d.reasoning_content.is_none());
        assert!(d.tool_calls.is_none());
    }

    /// `OpenAIDelta` MUST deserialize reasoning_content
    /// under the renamed key `reasoning_content`.
    #[test]
    fn delta_deserializes_reasoning_content() {
        // The wire format is `reasoning_content` (snake_case),
        // mapped to `reasoning_content` via
        // `#[serde(rename = "reasoning_content")]`.
        let json = r#"{"reasoning_content": "thinking..."}"#;
        let d: OpenAIDelta = serde_json::from_str(json).unwrap();
        assert_eq!(d.reasoning_content, Some("thinking...".to_string()));
    }

    /// `OpenAIDelta` MUST deserialize tool_calls with the
    /// OpenAI choice-indexed schema.
    #[test]
    fn delta_deserializes_tool_calls() {
        let json = r#"{
            "tool_calls": [
                {
                    "id": "call-1",
                    "function": {"name": "bash", "arguments": "{\"cmd\":\"ls\"}"},
                    "index": 0
                }
            ]
        }"#;
        let d: OpenAIDelta = serde_json::from_str(json).unwrap();
        let tcs = d.tool_calls.unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, Some("call-1".to_string()));
        assert_eq!(tcs[0].function.name, Some("bash".to_string()));
        assert_eq!(
            tcs[0].function.arguments,
            Some("{\"cmd\":\"ls\"}".to_string())
        );
        assert_eq!(tcs[0].index, Some(0));
    }

    /// `OpenAIDelta` MUST accept an empty object (no fields).
    #[test]
    fn delta_accepts_empty_object() {
        let json = r#"{}"#;
        let d: OpenAIDelta = serde_json::from_str(json).unwrap();
        assert!(d.content.is_none());
        assert!(d.reasoning_content.is_none());
        assert!(d.tool_calls.is_none());
    }

    // -- OpenAIDeltaUsage --------------------------------------------

    /// `OpenAIDeltaUsage` MUST deserialize all 3 token fields.
    /// (`OpenAIDeltaUsage` is deserialize-only; serde_json
    /// round-trip is not applicable.)
    #[test]
    fn usage_deserializes_all_three_fields() {
        let json = r#"{
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        }"#;
        let u: OpenAIDeltaUsage = serde_json::from_str(json).unwrap();
        assert_eq!(u.prompt_tokens, 100);
        assert_eq!(u.completion_tokens, 50);
        assert_eq!(u.total_tokens, 150);
    }

    /// `OpenAIDeltaUsage` MUST reject payload missing any
    /// required field (all 3 are required).
    #[test]
    fn usage_rejects_missing_required_fields() {
        let json = r#"{"prompt_tokens": 100, "completion_tokens": 50}"#;
        let result: Result<OpenAIDeltaUsage, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // -- OpenAIDeltaToolUse ------------------------------------------

    /// `OpenAIDeltaToolUse` MUST accept `id = null` (when
    /// the model emits subsequent delta chunks of the same
    /// tool call — id is only set on the first chunk).
    #[test]
    fn tool_use_accepts_null_id() {
        let json = r#"{"id": null, "function": {}, "index": 0}"#;
        let t: OpenAIDeltaToolUse = serde_json::from_str(json).unwrap();
        assert!(t.id.is_none());
        assert!(t.function.name.is_none());
        assert!(t.function.arguments.is_none());
        assert_eq!(t.index, Some(0));
    }

    /// `OpenAIDeltaToolUse` MUST accept omitted `id` and
    /// `index` (default to None).
    #[test]
    fn tool_use_optional_fields_default() {
        let json = r#"{"function": {"name": "bash"}}"#;
        let t: OpenAIDeltaToolUse = serde_json::from_str(json).unwrap();
        assert!(t.id.is_none());
        assert_eq!(t.function.name, Some("bash".to_string()));
        assert_eq!(t.index, None);
    }

    /// `OpenAIDeltaToolUse` MUST reject payload missing the
    /// strictly-required `function` field.
    #[test]
    fn tool_use_rejects_missing_function() {
        let json = r#"{"id": "x", "index": 0}"#;
        let result: Result<OpenAIDeltaToolUse, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // -- OpenAIDeltaToolUseFunction ----------------------------------

    /// `OpenAIDeltaToolUseFunction` MUST accept both `name`
    /// and `arguments` as null (initial tool-call delta).
    #[test]
    fn function_accepts_null_name_and_arguments() {
        let json = r#"{"name": null, "arguments": null}"#;
        let f: OpenAIDeltaToolUseFunction = serde_json::from_str(json).unwrap();
        assert!(f.name.is_none());
        assert!(f.arguments.is_none());
    }

    /// `OpenAIDeltaToolUseFunction` MUST round-trip both
    /// fields.
    #[test]
    fn function_round_trips_both_fields() {
        let json = r#"{"name": "bash", "arguments": "{\"cmd\":\"ls\"}"}"#;
        let f: OpenAIDeltaToolUseFunction = serde_json::from_str(json).unwrap();
        assert_eq!(f.name, Some("bash".to_string()));
        assert_eq!(f.arguments, Some("{\"cmd\":\"ls\"}".to_string()));
    }
}
