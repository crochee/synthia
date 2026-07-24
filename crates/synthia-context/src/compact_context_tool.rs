//! LLM-callable `compact_context` tool definition.
//!
//! Exposes the metadata (name, description, parameter schema) for the
//! context-compaction tool. The description carries a dynamic
//! `<context_tokens>X</context_tokens>` hint that the agent layer fills
//! in when tool definitions are queried.

/// Tool name exposed to the LLM.
pub const COMPACT_CONTEXT_TOOL_NAME: &str = "compact_context";

/// Static description exposed to the LLM via the `Tool::description()`
/// contract.
///
/// Returns `&'static str` (not `String`) because the [`synthia_tool::Tool`]
/// trait defines `description() -> &str`; a `&'static str` satisfies this
/// without an owned buffer on the tool struct.
///
/// The dynamic `<context_tokens>X</context_tokens>` hint surfaced by
/// [`compact_context_tool_definition`] is intentionally NOT included here
/// — that variant is used when the agent assembler rebuilds the tool
/// schema with a live token count, while this static description is the
/// one bound to the registered `CompactContextTool`.
pub fn compact_context_tool_description() -> &'static str {
    "Requests context compaction to reduce context size. Current context \
     size is indicated in the system prompt. Pass an optional 'reason' \
     parameter to explain why compaction is requested."
}

/// Build the LLM-facing [`synthia_provider::ToolDefinition`] for
/// `compact_context`, filling in the current context token count.
///
/// `current_tokens` is rounded to the nearest hundred so minor
/// fluctuations do not churn the tool schema / prompt cache key.
pub fn compact_context_tool_definition(
    current_tokens: usize,
) -> synthia_provider::ToolDefinition {
    let rounded = round_tokens_to_hundred(current_tokens);
    synthia_provider::ToolDefinition {
        name: COMPACT_CONTEXT_TOOL_NAME.to_string(),
        description: format!(
            "Compact the conversation context to free up tokens. Current \
             context size: <context_tokens>{}</context_tokens>. Optional \
             'reason' parameter explains why compaction is requested.",
            rounded
        ),
        input_schema: compact_context_tool_parameters(),
        cache_control: None,
    }
}

/// JSON Schema for the `compact_context` tool parameters.
///
/// The only parameter is an optional `reason` string.
pub fn compact_context_tool_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "reason": {
                "type": "string",
                "description": "Human-readable reason for requesting compaction"
            }
        },
        "required": []
    })
}

/// Round a token count to the nearest multiple of 100.
///
/// Used so that tiny context fluctuations do not invalidate prompt
/// caching or churn the tool definition snapshot.
pub fn round_tokens_to_hundred(tokens: usize) -> usize {
    ((tokens.saturating_add(50)) / 100) * 100
}

/// Format the summary returned by a successful `compact_context` call.
pub fn format_compact_context_summary(
    messages_compacted: usize,
    tokens_freed: usize,
) -> String {
    format!(
        "Compacted {} messages, freed {} tokens",
        messages_compacted, tokens_freed
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definition_has_expected_name_and_reason_schema() {
        let def = compact_context_tool_definition(1234);
        assert_eq!(def.name, COMPACT_CONTEXT_TOOL_NAME);

        let schema = def.input_schema;
        let properties = schema.get("properties").unwrap().as_object().unwrap();
        assert!(properties.contains_key("reason"));
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.is_empty());
    }

    #[test]
    fn description_contains_token_hint() {
        let def = compact_context_tool_definition(1234);
        assert!(
            def.description
                .contains("<context_tokens>1200</context_tokens>"),
            "description was: {}",
            def.description
        );
    }

    #[test]
    fn static_description_mentions_compaction_and_reason() {
        let desc = compact_context_tool_description();
        assert!(desc.contains("compaction"));
        assert!(desc.contains("reason"));
    }

    #[test]
    fn round_tokens_to_hundred_cases() {
        assert_eq!(round_tokens_to_hundred(0), 0);
        assert_eq!(round_tokens_to_hundred(49), 0);
        assert_eq!(round_tokens_to_hundred(50), 100);
        assert_eq!(round_tokens_to_hundred(123), 100);
        assert_eq!(round_tokens_to_hundred(150), 200);
        assert_eq!(round_tokens_to_hundred(199), 200);
        assert_eq!(round_tokens_to_hundred(200), 200);
    }

    #[test]
    fn summary_format_matches_contract() {
        let s = format_compact_context_summary(5, 1234);
        assert_eq!(s, "Compacted 5 messages, freed 1234 tokens");
    }
}
