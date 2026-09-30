//! [`StructuredOutputTool`] — schema-validated final-answer capture.
//!
//! pi-subagents `src/structured-output.ts` parity (R12-3). The
//! caller supplies a JSON Schema; the tool's `parameters()` IS
//! that schema, so the provider fills the fields as the tool
//! call arguments. `call()` validates the arguments against the
//! same schema (via [`synthia_core::validate_against_schema`])
//! and either:
//!
//! - returns the canonical JSON payload as the tool result
//!   (success — the structured output is captured), or
//! - returns an `is_error` result enumerating every violation
//!   with dotted paths so the model can retry with corrected
//!   fields in the same turn.
//!
//! This is the "synthetic tool that validates the LLM's payload"
//! pattern: the provider's tool-call arguments become the
//! structured output, no response_format machinery needed.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use synthia_core::validate_against_schema;

use crate::{Context, Tool, ToolOutput, traits::ExecutionMode};

/// Tool name convention (pi-subagents uses `StructuredOutput`).
pub const STRUCTURED_OUTPUT_TOOL_NAME: &str = "structured_output";

/// Build the schema-validated structured-output tool for
/// `schema`. The schema must be a JSON-Schema object
/// (`{"type": "object", "properties": ..., "required": ...}`);
/// anything else still works but validates permissively.
pub fn structured_output_tool(schema: Value) -> Arc<StructuredOutputTool> {
    Arc::new(StructuredOutputTool {
        schema,
        description: "Submit the final structured answer. Fill every \
                      field exactly as the schema requires; validation \
                      errors list the fields to fix."
            .to_string(),
    })
}

/// The synthetic structured-output tool. `parameters()` returns
/// the caller's schema verbatim (the provider enumerates the
/// fields from it); `call()` re-validates and captures.
pub struct StructuredOutputTool {
    schema: Value,
    description: String,
}

#[async_trait]
impl Tool for StructuredOutputTool {
    fn name(&self) -> &str {
        STRUCTURED_OUTPUT_TOOL_NAME
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.schema.clone()
    }

    fn mode(&self) -> ExecutionMode {
        // No side effects; safe to run parallel with others.
        ExecutionMode::Parallel
    }

    async fn call(&self, input: Value, _ctx: &Context) -> ToolOutput {
        match validate_against_schema(&self.schema, &input) {
            Ok(()) => {
                // Captured: echo the canonical payload back as the
                // tool result. The parent loop commits it as the
                // turn's structured output.
                ToolOutput::text(input.to_string())
            }
            Err(violations) => {
                let details: Vec<String> =
                    violations.iter().map(ToString::to_string).collect();
                ToolOutput::error(format!(
                    "structured output failed validation:\n  - {}",
                    details.join("\n  - ")
                ))
            }
        }
    }
}

// `stream` inherits the default (single Result), which is the
// right shape for a one-shot validation tool.

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ctx() -> Context {
        Context::default()
    }

    #[test]
    fn tool_name_is_the_convention() {
        let tool = structured_output_tool(json!({}));
        assert_eq!(tool.name(), "structured_output");
    }

    #[test]
    fn parameters_returns_the_caller_schema_verbatim() {
        let schema = json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"]
        });
        let tool = structured_output_tool(schema.clone());
        assert_eq!(tool.parameters(), schema);
    }

    #[tokio::test]
    async fn valid_payload_is_captured_as_json_text() {
        let tool = structured_output_tool(json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"]
        }));
        let out = tool.call(json!({"answer": "42"}), &ctx()).await;
        assert!(out.is_text());
        let parsed: Value = serde_json::from_str(
            &out.content[0]
                .text()
                .map(str::to_string)
                .unwrap_or_default(),
        )
        .unwrap();
        assert_eq!(parsed["answer"], "42");
    }

    #[tokio::test]
    async fn invalid_payload_is_an_error_with_dotted_paths() {
        let tool = structured_output_tool(json!({
            "type": "object",
            "properties": {
                "answer": {"type": "string"},
                "confidence": {"type": "number"}
            },
            "required": ["answer"]
        }));
        let out = tool.call(json!({"confidence": "high"}), &ctx()).await;
        assert_eq!(out.is_error, Some(true));
        let text = out
            .content
            .iter()
            .filter_map(|p| p.text().map(str::to_string))
            .collect::<String>();
        // Missing required field + wrong-typed field both named.
        assert!(text.contains("answer"), "must name the missing field");
        assert!(text.contains("confidence"), "must name the mistyped field");
    }

    #[tokio::test]
    async fn mode_is_parallel() {
        let tool = structured_output_tool(json!({}));
        assert!(matches!(tool.mode(), ExecutionMode::Parallel));
    }
}
