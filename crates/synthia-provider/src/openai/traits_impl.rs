//! `impl ModelProvider for OpenAICompatibleProvider`.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_core::{CancelToken, Error, RegistryItem};

use super::{
    provider::OpenAICompatibleProvider,
    types::{OpenAIEmbeddingRequest, OpenAIEmbeddingResponse, OpenAIResponse},
};
use crate::{
    openai_streaming::OpenAIStreamProcessor,
    streaming::{DEFAULT_STREAM_IDLE_TIMEOUT, pump_sse},
    traits::ModelProvider,
    types::{
        CompletionRequest,
        CompletionResponse,
        ModelConfig,
        ProviderConfig,
        SamplingResult,
        StreamChunk,
    },
};

#[async_trait]
impl ModelProvider for OpenAICompatibleProvider {
    async fn initialize(
        &mut self,
        config: ProviderConfig,
    ) -> Result<(), Error> {
        self.api_key = Some(config.api_key.into_inner());
        if let Some(base_url) = config.base_url {
            self.base_url = base_url;
        }
        Ok(())
    }

    fn name(&self) -> &str {
        // Return model name, not provider identifier
        &self.model_config.name
    }

    fn model_config(&self) -> ModelConfig {
        self.model_config.clone()
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        use crate::retry::{
            RetryClass,
            classify_provider_error_body,
            parse_retry_after,
            retry_with_classification,
        };

        // `llm.call` span (OTel semantic conventions for GenAI).
        // All fields populated later are declared as `Empty` at the
        // callsite — `Span::record(field, value)` is a silent no-op if
        // the field was not declared in the `span!` macro (lesson from
        // Task 7). `gen_ai.system` uses `model_config.provider` (not
        // `self.name()`, which returns the model name for OpenAI).
        #[cfg(feature = "otel")]
        let llm_span = tracing::span!(
            target: "synthia.llm",
            tracing::Level::INFO,
            "llm.call",
            gen_ai.system = %self.model_config.provider,
            gen_ai.request.model = %request.model,
            gen_ai.response.finish_reason = tracing::field::Empty,
            gen_ai.usage.input_tokens = tracing::field::Empty,
            gen_ai.usage.output_tokens = tracing::field::Empty,
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        #[cfg(feature = "otel")]
        let _llm_guard = llm_span.enter();

        let response = match retry_with_classification(|| {
            let req = request.clone();
            async move {
                let response = self
                    .make_request(&req)
                    .await?
                    .send()
                    .await
                    .map_err(Error::from)?;
                let status = response.status();
                if status.as_u16() == 429 {
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_retry_after);
                    // OpenAI reports an exhausted account quota with
                    // HTTP 429 as well; read the body so the
                    // classifier can tell it from a request-rate
                    // limit (quota is not retried).
                    let body = response.text().await.unwrap_or_default();
                    if classify_provider_error_body(429, &body)
                        == RetryClass::Quota
                    {
                        return Err(Error::request_failed(429, body));
                    }
                    return Err(Error::rate_limited(retry_after));
                }
                if !status.is_success() {
                    let message = response.text().await.unwrap_or_default();
                    return Err(Error::request_failed(
                        status.as_u16(),
                        message,
                    ));
                }
                let resp = response
                    .json::<OpenAIResponse>()
                    .await
                    .map_err(Error::from)?;
                let resp_json =
                    serde_json::to_string(&resp).unwrap_or_default();
                tracing::info!(target: "synthia_provider::openai::debug",
                    response = %resp_json,
                    response_len = resp_json.len(),
                    "OpenAI incoming response body"
                );
                // Parse + the empty-terminal guard run INSIDE the
                // retry loop: a degenerate empty completion is a
                // retryable failure class (dsh `EMPTY_RESPONSE`
                // parity), not a successful empty turn.
                self.parse_response(&resp).ensure_non_empty()
            }
        })
        .await
        {
            Ok(r) => r,
            Err(e) => {
                #[cfg(feature = "otel")]
                {
                    llm_span.record("exception.type", e.to_string());
                    llm_span.record("exception.message", e.to_string());
                    llm_span.record("otel.status_code", "ERROR");
                }
                return Err(e);
            }
        };

        // Record success attributes from the parsed response. The
        // values are lifted verbatim from the raw OpenAI body by
        // `parse_response`.
        #[cfg(feature = "otel")]
        {
            if let Some(reason) = response.stop_reason.as_deref() {
                llm_span.record("gen_ai.response.finish_reason", reason);
            }
            llm_span.record(
                "gen_ai.usage.input_tokens",
                response.usage.prompt_tokens,
            );
            llm_span.record(
                "gen_ai.usage.output_tokens",
                response.usage.completion_tokens,
            );
        }

        Ok(response)
    }

    async fn complete_with_stream(
        &self,
        request: CompletionRequest,
        cancel_token: Option<Arc<dyn CancelToken>>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        use crate::retry::retry_with_classification;

        // 1) Establish the stream. This phase — and only this phase — is
        //    retryable, with the same per-class budget `complete` uses:
        //    the body has not been read yet, so a 429/5xx/transport
        //    failure can be re-sent. The SSE pump below is deliberately
        //    *outside* the retry: once deltas have been handed to
        //    `on_delta`, re-sending would emit them twice, and the
        //    consumer has no way to un-see them.
        // Cloned per attempt: the closure is `FnMut` and the async block
        // moves what it captures, so the token is shared rather than
        // borrowed. An `Arc` clone is cheap and this runs once a request.
        let cancel_probe = cancel_token.clone();
        let resp = retry_with_classification(|| {
            let req = request.clone();
            let cancel_probe = cancel_probe.clone();
            async move {
                // See the Anthropic adapter: consulted before every
                // attempt so a cancelled run is not held in backoff.
                if cancel_probe.as_ref().is_some_and(|t| t.is_cancelled()) {
                    return Err(Error::stream_aborted(
                        "cancelled before the streaming request was sent",
                    ));
                }
                let url = format!("{}/chat/completions", self.base_url);
                let mut body = self.transform_request(&req);
                body.stream = true;
                // Ask for token usage on the final chunk (OpenAI
                // extension).
                if body.extra_body.is_none() {
                    body.extra_body = Some(std::collections::HashMap::new());
                }
                if let Some(extra) = body.extra_body.as_mut()
                    && !extra.contains_key("stream_options")
                {
                    extra.insert(
                        "stream_options".to_string(),
                        serde_json::json!({"include_usage": true}),
                    );
                }

                let mut outbound = self.client.post(&url).json(&body);
                if let Some(key) = &self.api_key {
                    outbound = outbound.bearer_auth(key);
                }
                let resp = outbound
                    .send()
                    .await
                    .map_err(|e| Error::stream_http_failure(e.to_string()))?;
                let status = resp.status();
                if status.as_u16() == 429 {
                    let retry_after = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(crate::retry::parse_retry_after);
                    // Same 429 split as `complete`: an exhausted account
                    // quota outranks the status because it is not
                    // retryable.
                    let body = resp.text().await.unwrap_or_default();
                    if crate::retry::classify_provider_error_body(429, &body)
                        == crate::retry::RetryClass::Quota
                    {
                        return Err(Error::request_failed(429, body));
                    }
                    return Err(Error::rate_limited(retry_after));
                }
                if !status.is_success() {
                    let message = resp.text().await.unwrap_or_default();
                    return Err(Error::request_failed(
                        status.as_u16(),
                        message,
                    ));
                }
                Ok(resp)
            }
        })
        .await?;

        // 2) Pull SSE bytes through the shared pump
        //    (`crate::streaming::pump_sse`): caller cancellation with a
        //    5s body-drain grace, plus the per-read idle watchdog that
        //    bounds every outstanding byte-stream read.
        let mut processor = OpenAIStreamProcessor::new();
        let mut final_sampling: Option<SamplingResult> = None;
        {
            let mut on_line = |line: &str| {
                // OpenAI uses both `data: ` (with space) and the bare
                // sentinel `data:[DONE]`. Tolerate either.
                let Some(data) =
                    line.strip_prefix("data:").map(str::trim_start)
                else {
                    return;
                };
                for chunk in processor.process_line(data) {
                    if let StreamChunk::IsDone { result } = &chunk {
                        final_sampling = Some((**result).clone());
                    }
                    on_delta(chunk);
                }
            };
            pump_sse(
                resp.bytes_stream(),
                cancel_token,
                DEFAULT_STREAM_IDLE_TIMEOUT,
                &mut on_line,
            )
            .await?;
        }

        // 3) Finalize: reconstruct a CompletionResponse so callers that
        //    still want a "response" struct get the assembled view. A
        //    terminal stop that produced no content at all is rejected
        //    as an empty completion (dsh `EMPTY_RESPONSE` parity).
        final_sampling
            .unwrap_or_default()
            .into_completion_response(request.model)
    }

    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/embeddings", self.base_url);
        let body = OpenAIEmbeddingRequest {
            model: self.model_config.name.clone(),
            input: texts,
        };
        let body_json = serde_json::to_string(&body).unwrap_or_default();

        tracing::debug!(target: "synthia_provider::openai::debug",
            url = %url,
            body_len = body_json.len(),
            "OpenAI embedding request"
        );

        let mut req = self.client.post(&url).json(&body);

        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }

        let response = req.send().await.map_err(Error::from)?;
        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(Error::request_failed(status.as_u16(), message));
        }

        let embedding_resp: OpenAIEmbeddingResponse = response
            .json::<OpenAIEmbeddingResponse>()
            .await
            .map_err(Error::from)?;

        let embeddings = embedding_resp
            .data
            .into_iter()
            .map(|d| {
                d.embedding
                    .into_iter()
                    .map(|v| v as f64)
                    .collect::<Vec<_>>()
            })
            .collect();

        Ok(embeddings)
    }
}

impl RegistryItem for OpenAICompatibleProvider {
    fn name(&self) -> &str {
        <Self as ModelProvider>::name(self)
    }

    fn description(&self) -> &str {
        "OpenAI-compatible model provider"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ModelConfig, ProviderConfig};

    fn provider_with_model(name: &str) -> OpenAICompatibleProvider {
        OpenAICompatibleProvider::new(
            "https://api.openai.com".to_string(),
            ModelConfig {
                name: name.into(),
                provider: "openai".into(),
                context_window: 128_000,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            },
        )
    }

    // -- RegistryItem trait ----------------------------------------

    /// `RegistryItem::name(self)`
    /// MUST delegate to
    /// `<Self as ModelProvider>::name(self)`
    /// (which returns the model
    /// name, e.g. `"gpt-4o"`).
    #[test]
    fn registry_item_name_delegates_to_model_provider() {
        let p = provider_with_model("gpt-4o");
        assert_eq!(
            <OpenAICompatibleProvider as RegistryItem>::name(&p),
            "gpt-4o"
        );
    }

    /// `RegistryItem::description(self)`
    /// MUST return the static
    /// description string.
    #[test]
    fn registry_item_description_is_static() {
        let p = provider_with_model("gpt-4o");
        let d = <OpenAICompatibleProvider as RegistryItem>::description(&p);
        assert!(!d.is_empty(), "description MUST be non-empty");
    }

    // -- ModelProvider trait (non-async methods) -------------------

    /// `ModelProvider::name(self)`
    /// MUST return the **model
    /// name** (NOT the provider
    /// identifier). This is the
    /// CRITICAL quirk vs Anthropic
    /// (which returns `"anthropic"`).
    /// If refactored to return
    /// provider id, routing keys
    /// would silently change.
    #[test]
    fn model_provider_name_returns_model_name_not_provider_id() {
        let p = provider_with_model("gpt-4o");
        assert_eq!(
            crate::traits::ModelProvider::name(&p),
            "gpt-4o",
            "OpenAI MUST return model name, NOT 'openai'"
        );
        // explicitly distinguish from Anthropic quirk
        assert_ne!(
            crate::traits::ModelProvider::name(&p),
            "openai",
            "OpenAI does NOT return provider id like Anthropic does"
        );
    }

    /// `ModelProvider::name(self)`
    /// MUST update when the
    /// underlying `model_config`
    /// changes (e.g. after a
    /// `with_model_name` call).
    #[test]
    fn model_provider_name_reflects_config_change() {
        let mut p = provider_with_model("gpt-4o");
        p.model_config.name = "gpt-4-turbo".to_string();
        assert_eq!(crate::traits::ModelProvider::name(&p), "gpt-4-turbo");
    }

    /// `ModelProvider::model_config(self)`
    /// MUST return a clone of the
    /// internally-stored
    /// `ModelConfig`.
    #[test]
    fn model_config_is_cloned_verbatim() {
        let p = provider_with_model("gpt-4o");
        let m1 = p.model_config();
        let m2 = p.model_config();
        assert_eq!(m1.name, m2.name);
        assert_eq!(m1.context_window, 128_000);
        assert_eq!(m1.max_output_tokens, 16_384);
    }

    /// `ModelProvider::initialize(mut self, config)`
    /// MUST store the API key from
    /// `ProviderConfig::api_key`.
    #[tokio::test]
    async fn initialize_stores_api_key() {
        let mut p = provider_with_model("gpt-4o");
        let cfg = ProviderConfig {
            api_key: synthia_core::Sensitive::new("sk-openai-test".into()),
            base_url: None,
            timeout_ms: None,
            max_retries: None,
        };
        p.initialize(cfg).await.unwrap();
        assert_eq!(p.api_key.as_deref(), Some("sk-openai-test"));
    }

    /// `ModelProvider::initialize`
    /// MUST update `base_url`
    /// when provided (OpenAI is
    /// multi-provider, so config
    /// can override the default
    /// base URL).
    #[tokio::test]
    async fn initialize_updates_base_url_when_provided() {
        let mut p = provider_with_model("gpt-4o");
        let cfg = ProviderConfig {
            api_key: synthia_core::Sensitive::new("k".into()),
            base_url: Some("https://custom.openai-proxy.example.com".into()),
            timeout_ms: None,
            max_retries: None,
        };
        p.initialize(cfg).await.unwrap();
        assert_eq!(p.base_url, "https://custom.openai-proxy.example.com");
    }

    /// `ModelProvider::initialize`
    /// MUST preserve the existing
    /// `base_url` when not provided
    /// (no overwrite to empty
    /// string).
    #[tokio::test]
    async fn initialize_preserves_base_url_when_not_provided() {
        let mut p = provider_with_model("gpt-4o");
        let original = p.base_url.clone();
        let cfg = ProviderConfig {
            api_key: synthia_core::Sensitive::new("k".into()),
            base_url: None,
            timeout_ms: None,
            max_retries: None,
        };
        p.initialize(cfg).await.unwrap();
        assert_eq!(p.base_url, original);
    }

    /// `ModelProvider::initialize`
    /// MUST return `Ok(())` on
    /// success.
    #[tokio::test]
    async fn initialize_returns_ok() {
        let mut p = provider_with_model("gpt-4o");
        let cfg = ProviderConfig {
            api_key: synthia_core::Sensitive::new("k".into()),
            base_url: None,
            timeout_ms: None,
            max_retries: None,
        };
        let result = p.initialize(cfg).await;
        assert!(result.is_ok());
    }
}
