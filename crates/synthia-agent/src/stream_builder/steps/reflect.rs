use std::sync::Arc;

use synthia_core::Error;
use synthia_provider::{
    traits::ModelProvider,
    types::{CompletionRequest, Message},
};

use crate::{loop_context::LoopContext, types::Reflection};

pub struct StepReflect {
    model: String,
}

impl StepReflect {
    pub fn new(model: String) -> Self {
        Self { model }
    }

    pub async fn execute(
        &self,
        provider: Arc<dyn ModelProvider>,
        ctx: &LoopContext,
    ) -> Result<Reflection, Error> {
        let system_prompt = r#"
你是一个专门进行执行反思的助手。请分析最近的执行过程，提供结构化的反思。
严格以 JSON 格式输出，不包含任何其他文字：
{
    "summary": "执行过程的简要总结",
    "issues": ["问题1", "问题2", ...],
    "suggestions": ["建议1", "建议2", ...]
}
"#;

        let user_message = Message::user(format!(
            "请分析以下对话历史，提供反思：\n\n{:?}",
            ctx.messages
        ));

        let request = CompletionRequest {
            model: self.model.clone(),
            messages: Arc::new(vec![
                Message::system(system_prompt),
                user_message,
            ]),
            temperature: Some(0.3),
            max_tokens: Some(2000),
            ..Default::default()
        };

        let response = provider.complete(request).await?;
        let content_text =
            response.content.extract_text().ok_or_else(|| {
                Error::InvalidItem("No text content in response".to_string())
            })?;

        let json_start = content_text.find('{').ok_or_else(|| {
            Error::InvalidItem("No JSON content in response".to_string())
        })?;
        let json_end = content_text.rfind('}').ok_or_else(|| {
            Error::InvalidItem("No closing brace in response".to_string())
        })?;
        let json_str = &content_text[json_start..=json_end];

        #[derive(serde::Deserialize)]
        struct ReflectionResponse {
            summary: String,
            issues: Vec<String>,
            suggestions: Vec<String>,
        }

        let reflection_response: ReflectionResponse =
            serde_json::from_str(json_str).map_err(|e| {
                Error::InvalidItem(format!(
                    "Failed to parse reflection response: {}",
                    e
                ))
            })?;

        Ok(Reflection::new(
            ctx.iteration,
            reflection_response.summary,
            reflection_response.issues,
            reflection_response.suggestions,
        ))
    }
}
