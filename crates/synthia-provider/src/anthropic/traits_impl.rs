//! `impl ModelProvider for AnthropicProvider`.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_core::{CancelToken, Error, RegistryItem};

use super::{provider::AnthropicProvider, types::AnthropicResponse};
use crate::{
    streaming::{
        AnthropicStreamEvent,
        DEFAULT_STREAM_IDLE_TIMEOUT,
        StreamProcessor,
        pump_sse,
    },
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
impl ModelProvider for AnthropicProvider {
    async fn initialize(
        &mut self,
        config: ProviderConfig,
    ) -> Result<(), Error> {
        self.api_key = Some(config.api_key.into_inner());
        Ok(())
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn model_config(&self) -> ModelConfig {
        self.model_config.clone()
    }

    fn supports_inline_cache_hints(&self) -> bool {
        true
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
        // All fields that are populated later MUST be declared as
        // `Empty` at the callsite — `Span::record(field, value)` is a
        // silent no-op if the field was not declared in the `span!`
        // macro (lesson from Task 7).
        #[cfg(feature = "otel")]
        let llm_span = tracing::span!(
            target: "synthia.llm",
            tracing::Level::INFO,
            "llm.call",
            gen_ai.system = %crate::traits::ModelProvider::name(self),
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
                    // A 429 can also mean an exhausted account quota;
                    // read the body so the classifier can tell the two
                    // apart (quota is not retried).
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
                    .json::<AnthropicResponse>()
                    .await
                    .map_err(Error::from)?;
                let resp_json =
                    serde_json::to_string(&resp).unwrap_or_default();
                tracing::info!(target: "synthia_provider::anthropic::debug",
                    response = %resp_json,
                    response_len = resp_json.len(),
                    "Anthropic incoming response body"
                );
                // Parse + the empty-terminal guard run INSIDE the
                // retry loop: a degenerate empty completion is a
                // retryable failure class (dsh `EMPTY_RESPONSE`
                // parity), not a successful empty turn.
                self.parse_response(&resp, &req.model).ensure_non_empty()
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
        // values are lifted verbatim from the raw Anthropic body by
        // `parse_response`.
        #[cfg(feature = "otel")]
        {
            if let Some(stop) = response.stop_reason.as_deref() {
                llm_span.record("gen_ai.response.finish_reason", stop);
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
        //    consumer has no way to un-see them. Nothing in the loop
        //    re-sends a streamed pass, so this was the one place a
        //    transient failure ended the whole run — on the path every
        //    ReAct iteration actually takes.
        // Cloned per attempt: the closure is `FnMut` and the async block
        // moves what it captures, so the token is shared rather than
        // borrowed. An `Arc` clone is cheap and this runs once a request.
        let cancel_probe = cancel_token.clone();
        let resp = retry_with_classification(|| {
            let req = request.clone();
            let cancel_probe = cancel_probe.clone();
            async move {
                // Checked *inside* the closure so it is consulted before
                // every attempt, not just the first: a bounded retry
                // budget can otherwise keep re-sending for tens of
                // seconds (`RateLimit` is 6 attempts over a 1s→30s
                // backoff) while the caller has already cancelled.
                // `StreamAborted` classifies as `Permanent`, so it ends
                // the loop immediately instead of sleeping.
                if cancel_probe.as_ref().is_some_and(|t| t.is_cancelled()) {
                    return Err(Error::stream_aborted(
                        "cancelled before the streaming request was sent",
                    ));
                }
                let mut body = self.transform_request(&req);
                body.stream = true;
                let url = format!("{}/v1/messages", self.base_url);
                let mut outbound = self
                    .client
                    .post(url)
                    .json(&body)
                    .header("anthropic-version", "2023-06-01")
                    .header("content-type", "application/json")
                    .header("anthropic-beta", "prompt-caching-2024-07-31");
                if let Some(key) = &self.api_key {
                    outbound = outbound.header("x-api-key", key);
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
        let mut processor = StreamProcessor::new();
        let mut final_sampling: Option<SamplingResult> = None;
        {
            let mut on_line = |line: &str| {
                if let Some(data) = line.strip_prefix("data: ")
                    && let Ok(event) =
                        serde_json::from_str::<AnthropicStreamEvent>(data)
                {
                    for chunk in processor.process_event(&event) {
                        if let StreamChunk::IsDone { result } = &chunk {
                            final_sampling = Some((**result).clone());
                        }
                        on_delta(chunk);
                    }
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
        //    still want a "response" struct get the assembled view. If
        //    the upstream never emitted IsDone (network cut-off
        //    mid-stream), we fall back to the assembled partial
        //    sampling result. A terminal stop that produced no
        //    content at all is rejected as an empty completion (dsh
        //    `EMPTY_RESPONSE` parity).
        final_sampling
            .unwrap_or_default()
            .into_completion_response(request.model)
    }
}

impl RegistryItem for AnthropicProvider {
    fn name(&self) -> &str {
        <Self as ModelProvider>::name(self)
    }

    fn description(&self) -> &str {
        "Anthropic Claude model provider"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ModelConfig, ProviderConfig};

    fn provider() -> AnthropicProvider {
        AnthropicProvider::new(ModelConfig {
            name: "claude-3-5-sonnet".into(),
            provider: "anthropic".into(),
            context_window: 200_000,
            max_output_tokens: 8_192,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        })
    }

    // -- RegistryItem trait ----------------------------------------

    /// `RegistryItem::name(self)`
    /// MUST delegate to
    /// `<Self as ModelProvider>::name(self)`
    /// (return `"anthropic"`).
    #[test]
    fn registry_item_name_is_anthropic() {
        let p = provider();
        assert_eq!(crate::traits::ModelProvider::name(&p), "anthropic");
        assert_eq!(<AnthropicProvider as RegistryItem>::name(&p), "anthropic");
    }

    /// `RegistryItem::description(self)`
    /// MUST return the static
    /// `"Anthropic Claude model provider"`
    /// string.
    #[test]
    fn registry_item_description_is_static() {
        let p = provider();
        assert_eq!(
            <AnthropicProvider as RegistryItem>::description(&p),
            "Anthropic Claude model provider"
        );
    }

    // -- ModelProvider trait (non-async methods) -------------------

    /// `ModelProvider::name(self)`
    /// MUST return `"anthropic"`
    /// (used for routing keys).
    #[test]
    fn model_provider_name_is_anthropic() {
        let p = provider();
        assert_eq!(crate::traits::ModelProvider::name(&p), "anthropic");
    }

    /// `ModelProvider::model_config(self)`
    /// MUST return a clone of the
    /// internally-stored
    /// `ModelConfig`.
    #[test]
    fn model_config_is_cloned_verbatim() {
        let p = provider();
        let m1 = p.model_config();
        let m2 = p.model_config();
        assert_eq!(m1.name, m2.name);
        assert_eq!(m1.context_window, 200_000);
        assert_eq!(m1.max_output_tokens, 8_192);
    }

    /// `ModelProvider::supports_inline_cache_hints(self)`
    /// MUST return `true` for
    /// Anthropic (Anthropic is one
    /// of the two providers that
    /// supports inline cache hints).
    #[test]
    fn anthropic_supports_inline_cache_hints() {
        let p = provider();
        assert!(p.supports_inline_cache_hints());
    }

    /// `ModelProvider::initialize(mut self, config)`
    /// MUST store the API key from
    /// `ProviderConfig::api_key`.
    #[tokio::test]
    async fn initialize_stores_api_key() {
        let mut p = provider();
        let cfg = ProviderConfig {
            api_key: synthia_core::Sensitive::new("sk-test-key".into()),
            base_url: None,
            timeout_ms: None,
            max_retries: None,
        };
        p.initialize(cfg).await.unwrap();
        assert_eq!(p.api_key.as_deref(), Some("sk-test-key"));
    }

    /// `ModelProvider::initialize`
    /// MUST return `Ok(())` on
    /// success (not propagate any
    /// error).
    #[tokio::test]
    async fn initialize_returns_ok() {
        let mut p = provider();
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
