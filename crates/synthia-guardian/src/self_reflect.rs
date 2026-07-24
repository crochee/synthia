//! Guardian self-reflection tool definition and review logic.
//!
//! Exposes the `self_reflect` tool metadata (name, description,
//! parameter schema) so that upper layers can register it as an
//! LLM-callable tool. The actual review is performed by
//! [`run_self_reflect`], which performs an independent context review
//! and returns structured feedback.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use synthia_provider::{
    traits::ModelProvider,
    types::{CompletionRequest, Message},
};

/// Tool name exposed to the LLM.
pub const SELF_REFLECT_TOOL_NAME: &str = "self_reflect";

/// Tool description exposed to the LLM.
///
/// Mentions an independent context review and structured feedback as
/// required by the agent design principles.
pub fn self_reflect_tool_description() -> &'static str {
    "Requests an independent context review from the Guardian layer and \
     returns structured feedback (summary, issues, suggestions) about the \
     session's progress."
}

/// JSON Schema for the `self_reflect` tool parameters.
///
/// The tool accepts no required parameters; callers may invoke it with an
/// empty object.
pub fn self_reflect_tool_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "required": []
    })
}

/// Structured result of a Guardian self-reflection review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelfReflectResult {
    pub summary: String,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

/// Run an independent context review over `conversation`.
///
/// The review is performed by the configured `provider` using a
/// deterministic prompt. The provider response is parsed as JSON matching
/// [`SelfReflectResult`].
pub async fn run_self_reflect(
    conversation: &[Message],
    provider: &Arc<dyn ModelProvider>,
    model: &str,
) -> anyhow::Result<SelfReflectResult> {
    let system_prompt = r#"You are an independent context review assistant.
Analyze the provided conversation history and produce structured feedback.
Strictly output valid JSON with no additional text:
{
    "summary": "Brief summary of the session so far",
    "issues": ["issue 1", "issue 2"],
    "suggestions": ["suggestion 1", "suggestion 2"]
}"#;

    let user_message = Message::user(format!(
        "Please review the following conversation context and provide \
         structured feedback:\n\n{:?}",
        conversation
    ));

    let request = CompletionRequest {
        model: model.to_string(),
        messages: Arc::new(vec![Message::system(system_prompt), user_message]),
        temperature: Some(0.3),
        max_tokens: Some(2000),
        ..Default::default()
    };

    let response = provider.complete(request).await?;
    let text = response.content.extract_text().unwrap_or_default();

    let json_start = text.find('{').ok_or_else(|| {
        anyhow::anyhow!("No JSON object found in self-reflection response")
    })?;
    let json_end = text.rfind('}').ok_or_else(|| {
        anyhow::anyhow!("No closing brace found in self-reflection response")
    })?;
    let json_str = &text[json_start..=json_end];

    serde_json::from_str(json_str).map_err(|e| {
        anyhow::anyhow!("Failed to parse self-reflection response: {}", e)
    })
}
