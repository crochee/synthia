//! Model call implementation with retry logic

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use backoff::ExponentialBackoff;
use futures::stream::{BoxStream, StreamExt};
use rmcp::model::{
    CreateMessageRequestParams,
    CreateMessageResult,
    SamplingMessage,
    Tool,
};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::Agent;
use crate::{AgentError, Result, hooks::HookEvent};

impl Agent {
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        skip(self, system_prompt, conversation, tools, backoff, cancel_token),
        fields(
            message_count = conversation.len(),
            tool_count = tools.len(),
        )
    )]
    pub(super) async fn call_model_with_retry(
        &self,
        system_prompt: Option<String>,
        conversation: &[SamplingMessage],
        tools: &[Tool],
        backoff: ExponentialBackoff,
        cancel_token: &CancellationToken,
    ) -> Result<BoxStream<'_, Result<CreateMessageResult>>> {
        let routing = self.deps.router.route(conversation).await?;
        let model_info = routing.config.model_info();

        self.deps
            .hooks
            .emit(&HookEvent::BeforeLLMCall {
                model: model_info.name.clone(),
                message_count: conversation.len(),
            })
            .await;

        let provider = routing.provider;
        let hook_registry = Arc::clone(&self.deps.hooks);
        let model_name = model_info.name.clone();
        let tokens_used = Arc::new(AtomicU64::new(0));

        let result = backoff::future::retry(backoff, move || {
            let provider = Arc::clone(&provider);
            let model_info = model_info.clone();
            let system_prompt = system_prompt.clone();
            let conversation = conversation.to_owned();
            let tools = tools.to_owned();

            async move {
                let params = CreateMessageRequestParams {
                    meta: None,
                    task: None,
                    messages: conversation,
                    model_preferences: Some(rmcp::model::ModelPreferences {
                        hints: Some(vec![rmcp::model::ModelHint {
                            name: Some(model_info.name.clone()),
                        }]),
                        cost_priority: None,
                        intelligence_priority: None,
                        speed_priority: None,
                    }),
                    system_prompt,
                    include_context: None,
                    temperature: model_info.temperature,
                    max_tokens: model_info.max_tokens,
                    stop_sequences: None,
                    metadata: None,
                    tools: Some(tools),
                    tool_choice: None,
                };

                provider
                    .stream(params, cancel_token.clone())
                    .await
                    .map_err(|e| {
                        let is_transient = matches!(
                            &e,
                            synthia_provider::ProviderError::HttpError(_)
                                | synthia_provider::ProviderError::RateLimitError(_)
                                | synthia_provider::ProviderError::Timeout
                                | synthia_provider::ProviderError::ApiError(_)
                        );
                        let agent_error = AgentError::from(e);

                        if is_transient {
                            tracing::warn!("Model call failed (retrying): {}", agent_error);
                            backoff::Error::transient(agent_error)
                        } else {
                            tracing::error!("Model call failed (permanent error): {}", agent_error);
                            backoff::Error::permanent(agent_error)
                        }
                    })
            }
        })
        .await;

        match result {
            Ok(stream) => Ok(Box::pin(Self::wrap_stream_with_hooks(
                stream,
                hook_registry,
                model_name,
                tokens_used,
            ))),
            Err(e) => {
                tracing::error!("Model call failed after retries: {}", e);
                hook_registry
                    .emit(&HookEvent::AfterLLMCall {
                        model: model_name,
                        tokens_used: None,
                        success: false,
                    })
                    .await;
                Err(e)
            }
        }
    }

    fn wrap_stream_with_hooks(
        stream: impl futures::Stream<
            Item = Result<CreateMessageResult, synthia_provider::ProviderError>,
        > + Send
        + 'static,
        hook_registry: Arc<crate::hooks::HookRegistry>,
        model_name: String,
        tokens_used: Arc<AtomicU64>,
    ) -> impl futures::Stream<Item = Result<CreateMessageResult>> {
        let tokens_clone = Arc::clone(&tokens_used);
        let converted = stream.map(move |item| {
            if let Ok(CreateMessageResult { message, .. }) = &item {
                let text_len = Self::estimate_tokens(&message.content);
                tokens_clone.fetch_add(text_len, Ordering::Relaxed);
            }
            item.map_err(AgentError::from)
        });

        async_stream::stream! {
            tokio::pin!(converted);
            let mut success = true;
            while let Some(item) = converted.next().await {
                if item.is_err() { success = false; }
                yield item;
            }

            hook_registry.emit(&HookEvent::AfterLLMCall {
                model: model_name,
                tokens_used: Some(tokens_used.load(Ordering::Relaxed)),
                success,
            }).await;
        }
    }

    fn estimate_tokens(
        content: &rmcp::model::SamplingContent<
            rmcp::model::SamplingMessageContent,
        >,
    ) -> u64 {
        use rmcp::model::{SamplingContent, SamplingMessageContent};

        let text_len: usize = match content {
            SamplingContent::Single(SamplingMessageContent::Text(t)) => {
                t.text.len()
            }
            SamplingContent::Multiple(contents) => contents
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.len()))
                .sum(),
            _ => 0,
        };
        text_len as u64 / 4
    }
}
