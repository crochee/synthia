//! The request builder method on
//! [`super::core::AnthropicProvider`]:
//!
//! - [`AnthropicProvider::make_request`] — transforms the
//!   [`crate::types::CompletionRequest`] into a
//!   JSON-serialized body, then returns a
//!   `reqwest::RequestBuilder` POSTing to
//!   `{base_url}/v1/messages` with the
//!   `anthropic-version`, `content-type`, and
//!   `anthropic-beta` headers plus optional `x-api-key`
//!   auth. The outgoing body is logged at
//!   `tracing::debug!` level under the
//!   `synthia_provider::anthropic::debug` target
//!   (downgraded from `info!` to avoid leaking user
//!   prompts / system prompts / API keys into production
//!   logs at default verbosity).

use synthia_core::Error;

use super::core::AnthropicProvider;
use crate::types::CompletionRequest;

impl AnthropicProvider {
    pub(in crate::anthropic) async fn make_request(
        &self,
        request: &CompletionRequest,
    ) -> Result<reqwest::RequestBuilder, Error> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = self.transform_request(request);
        let body_json = serde_json::to_string(&body).unwrap_or_default();

        tracing::debug!(target: "synthia_provider::anthropic::debug",
            url = %url,
            body_len = body_json.len(),
            "Anthropic outgoing request body"
        );

        let mut req = self
            .client
            .post(url)
            .json(&body)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("anthropic-beta", "prompt-caching-2024-07-31");

        if let Some(ref key) = self.api_key {
            req = req.header("x-api-key", key);
        }

        Ok(req)
    }
}
