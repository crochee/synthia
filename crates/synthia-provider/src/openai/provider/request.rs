//! The request builder method on
//! [`super::core::OpenAICompatibleProvider`]:
//!
//! - [`OpenAICompatibleProvider::make_request`] —
//!   transforms the [`crate::types::CompletionRequest`]
//!   into a JSON-serialized body, then returns a
//!   `reqwest::RequestBuilder` POSTing to
//!   `{base_url}/chat/completions` with optional bearer
//!   auth. The outgoing body is logged at `tracing::debug!`
//!   level under the `synthia_provider::openai::debug`
//!   target (downgraded from `info!` to avoid leaking
//!   user prompts / system prompts / API keys into
//!   production logs at default verbosity).

use synthia_core::Error;

use super::core::OpenAICompatibleProvider;
use crate::types::CompletionRequest;

impl OpenAICompatibleProvider {
    pub(in crate::openai) async fn make_request(
        &self,
        request: &CompletionRequest,
    ) -> Result<reqwest::RequestBuilder, Error> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.transform_request(request);
        let body_json = serde_json::to_string(&body).unwrap_or_default();

        tracing::debug!(target: "synthia_provider::openai::debug",
            url = %url,
            body_len = body_json.len(),
            "OpenAI outgoing request body"
        );

        let mut req = self.client.post(&url).json(&body);

        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }

        Ok(req)
    }
}
