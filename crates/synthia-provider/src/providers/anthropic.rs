//! Anthropic provider implementation

use rmcp::model::CreateMessageRequestParams;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    MessageStream,
    ModelProvider,
    ProviderError,
    Result,
    formats::anthropic::create_request,
    providers::BaseProvider,
};

/// Anthropic provider implementation
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    base: BaseProvider,
    completions: String,
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        #[allow(clippy::unwrap_used)]
        let base =
            BaseProvider::new("https://api.anthropic.com/v1", None).unwrap();
        Self {
            base,
            completions: "/messages".to_string(),
        }
    }
}

impl AnthropicProvider {
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
impl ModelProvider for AnthropicProvider {
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

        let body = create_request(&params, true)?;

        let response = self
            .base
            .with_retry_cancellable(
                || {
                    let client = std::sync::Arc::<reqwest::Client>::clone(
                        &self.base.client,
                    );
                    let endpoint = endpoint.clone();
                    let body = body.clone();
                    let api_key = self.base.api_key.clone();
                    let cancel_token = cancel_token.clone();

                    Box::pin(async move {
                        let mut request_builder = client
                            .post(endpoint)
                            .header("Content-Type", "application/json")
                            .header("Anthropic-Version", "2023-06-01")
                            .json(&body);

                        if let Some(api_key) = &api_key {
                            request_builder =
                                request_builder.header("x-api-key", api_key);
                        }

                        BaseProvider::send_with_cancel(
                            request_builder,
                            cancel_token,
                        )
                        .await
                    })
                },
                cancel_token.clone(),
            )
            .await?;

        let response = self.base.handle_response_status(response).await?;

        let lines_stream = BaseProvider::create_cancellable_lines_stream(
            response,
            cancel_token,
        );
        let message_stream =
            crate::formats::anthropic::anthropic_to_message_stream(
                lines_stream,
            );
        Ok(Box::pin(message_stream))
    }
}
