use std::sync::Arc;

use tracing::info;

use crate::{
    AgentError,
    Result,
    model_router::ModelRouter,
    utils::extract_tool_uses,
};

pub struct LightweightSubagent {
    model_router: Arc<dyn ModelRouter>,
    system_prompt: String,
    max_turns: u32,
}

impl LightweightSubagent {
    pub fn new(
        model_router: Arc<dyn ModelRouter>,
        system_prompt: String,
        max_turns: u32,
    ) -> Self {
        Self {
            model_router,
            system_prompt,
            max_turns,
        }
    }

    pub async fn run(&self, prompt: &str) -> Result<String> {
        info!("Starting lightweight subagent execution");

        let mut messages =
            vec![rmcp::model::SamplingMessage::user_text(prompt)];
        let mut full_response = String::new();

        for _ in 0..self.max_turns {
            let result = self.model_router.route(&messages).await?;
            let provider = result.provider;
            let config = result.config;

            let params = rmcp::model::CreateMessageRequestParams {
                meta: None,
                task: None,
                messages: messages.clone(),
                model_preferences: None,
                system_prompt: Some(self.system_prompt.clone()),
                include_context: None,
                temperature: config.model_info().temperature,
                max_tokens: config.model_info().max_tokens,
                stop_sequences: None,
                metadata: None,
                tools: None,
                tool_choice: None,
            };

            let stream = provider
                .stream(params, tokio_util::sync::CancellationToken::new())
                .await?;
            let create_result =
                synthia_provider::collect_stream(stream).await?;

            let msg = create_result.message.clone();
            let tool_uses = extract_tool_uses(&msg);
            let text =
                crate::utils::extract_response_text(std::slice::from_ref(&msg));
            full_response.push_str(&text);
            messages.push(msg);

            match create_result.stop_reason.as_deref() {
                Some("stop") => {
                    return Ok(full_response.trim().to_string());
                }
                Some(other)
                    if !matches!(
                        other,
                        "tool_use" | "function_call" | "tool_calls"
                    ) =>
                {
                    tracing::warn!("Subagent stopped with reason: {}", other);
                    return Ok(full_response.trim().to_string());
                }
                _ => {}
            }

            if !tool_uses.is_empty() {
                for tu in tool_uses {
                    let input = serde_json::to_string(&tu.input)
                        .unwrap_or_else(|_| "{}".to_string());
                    messages.push(rmcp::model::SamplingMessage::user_text(
                        format!("Tool {} returned: {}", tu.name, input),
                    ));
                }
            }
        }

        Err(AgentError::internal(format!(
            "Subagent exceeded max turns: {}",
            self.max_turns
        )))
    }
}
