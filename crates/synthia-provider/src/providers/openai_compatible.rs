//! OpenAI compatible provider implementation for local models

use rmcp::model::CreateMessageRequestParams;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    MessageStream,
    ModelProvider,
    ProviderError,
    Result,
    formats::openai_compatible::{
        create_request,
        response_to_streaming_message,
    },
    providers::BaseProvider,
};

/// OpenAI compatible provider implementation for local models
#[derive(Debug, Clone)]
pub struct OpenAICompatibleProvider {
    base: BaseProvider,
    completions: String,
}

impl Default for OpenAICompatibleProvider {
    fn default() -> Self {
        #[allow(clippy::unwrap_used)]
        let base =
            BaseProvider::new("http://localhost:11434/v1", None).unwrap();
        Self {
            base,
            completions: "/chat/completions".to_string(),
        }
    }
}

impl OpenAICompatibleProvider {
    pub fn with_api_key(mut self, api_key: &str) -> Self {
        self.base = self.base.with_api_key(api_key);
        self
    }

    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base = self.base.with_base_url(base_url);
        self
    }
}

#[async_trait::async_trait]
impl ModelProvider for OpenAICompatibleProvider {
    #[instrument(skip(self, params, cancel_token), fields(endpoint = %self.base.base_url))]
    async fn stream(
        &self,
        params: CreateMessageRequestParams,
        cancel_token: CancellationToken,
    ) -> Result<MessageStream> {
        let endpoint = self
            .base
            .build_url(&self.base.base_url, &self.completions)
            .map_err(|e| ProviderError::api(e.to_string()))?;

        tracing::info!("OpenAI compatible endpoint: {:?}", endpoint);
        let payload = create_request(&params, true)?;
        tracing::info!("OpenAI compatible request: {}", payload);
        let response = self
            .base
            .with_retry_cancellable(
                || {
                    let client = std::sync::Arc::<reqwest::Client>::clone(
                        &self.base.client,
                    );
                    let endpoint = endpoint.clone();
                    let payload = payload.clone();
                    let api_key = self.base.api_key.clone();
                    let cancel_token = cancel_token.clone();

                    Box::pin(async move {
                        let mut builder = client
                            .post(endpoint)
                            .header("Content-Type", "application/json");

                        if let Some(api_key) = &api_key {
                            builder = builder.header(
                                "Authorization",
                                format!("Bearer {api_key}"),
                            );
                        }

                        BaseProvider::send_with_cancel(
                            builder.json(&payload),
                            cancel_token,
                        )
                        .await
                    })
                },
                cancel_token.clone(),
            )
            .await?;

        let response = self
            .base
            .handle_response_status_with_context_check(response, true)
            .await?;

        let lines_stream = BaseProvider::create_cancellable_lines_stream(
            response,
            cancel_token,
        );
        let message_stream = response_to_streaming_message(lines_stream);
        Ok(Box::pin(message_stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_context_length_exceeded() {
        assert!(BaseProvider::check_context_length_exceeded(
            "Your input is too long"
        ));
        assert!(BaseProvider::check_context_length_exceeded(
            "Context length exceeded"
        ));
        assert!(!BaseProvider::check_context_length_exceeded("Hello world"));
    }
}
