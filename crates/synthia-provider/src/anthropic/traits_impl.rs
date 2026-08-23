//! `impl ModelProvider for AnthropicProvider` and the
//! `wait_cancel` `tokio::select!` arm helper.

use async_trait::async_trait;
use futures::StreamExt;
use synthia_core::{Error, RegistryItem};
use tokio_util::sync::CancellationToken;

use super::{provider::AnthropicProvider, types::AnthropicResponse};
use crate::{
    streaming::{AnthropicStreamEvent, StreamProcessor},
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
            RetryConfig,
            parse_retry_after,
            retry_with_backoff,
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

        let resp = match retry_with_backoff(RetryConfig::default(), || {
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
                    return Err(Error::rate_limited(retry_after));
                }
                if !status.is_success() {
                    let message = response.text().await.unwrap_or_default();
                    return Err(Error::request_failed(
                        status.as_u16(),
                        message,
                    ));
                }
                response
                    .json::<AnthropicResponse>()
                    .await
                    .map_err(Error::from)
            }
        })
        .await
        {
            Ok(r) => r,
            Err(e) => {
                #[cfg(feature = "otel")]
                {
                    llm_span.record("exception.type", e.to_string());
                    llm_span.record("otel.status_code", "ERROR");
                }
                return Err(e);
            }
        };

        // Record success attributes from the raw Anthropic response
        // (which carries `stop_reason` + `usage` before they are
        // transformed into `CompletionResponse`).
        #[cfg(feature = "otel")]
        {
            if let Some(stop) = resp.stop_reason.as_deref() {
                llm_span.record("gen_ai.response.finish_reason", stop);
            }
            llm_span
                .record("gen_ai.usage.input_tokens", resp.usage.input_tokens);
            llm_span
                .record("gen_ai.usage.output_tokens", resp.usage.output_tokens);
        }

        let resp_json = serde_json::to_string(&resp).unwrap_or_default();
        tracing::info!(target: "synthia_provider::anthropic::debug",
            response = %resp_json,
            response_len = resp_json.len(),
            "Anthropic incoming response body"
        );

        Ok(self.parse_response(&resp, &request.model))
    }

    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        Err(Error::internal(
            "Anthropic provider does not support embedding",
        ))
    }

    async fn complete_with_stream(
        &self,
        request: CompletionRequest,
        cancel_token: Option<CancellationToken>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        // 1) Build the streaming body. `transform_request` sets `stream:
        //    false`; we wrap it and force `stream: true` so the upstream
        //    sends SSE.
        let mut body = self.transform_request(&request);
        body.stream = true;
        let url = format!("{}/v1/messages", self.base_url);
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
        let resp = req
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
            return Err(Error::rate_limited(retry_after));
        }
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(Error::request_failed(status.as_u16(), message));
        }

        // 2) Pull SSE bytes. We select! between the byte stream and
        //    cancel_token; cancellation gives the connection 5s to flush
        //    its body before we abort hard.
        const CANCEL_GRACE: std::time::Duration =
            std::time::Duration::from_secs(5);
        let mut processor = StreamProcessor::new();
        let mut byte_stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut final_sampling: Option<SamplingResult> = None;

        loop {
            // Cancellation check before each read.
            if let Some(token) = &cancel_token
                && token.is_cancelled()
            {
                return Err(Error::stream_aborted(
                    "stream cancelled by caller",
                ));
            }

            let next = tokio::select! {
                biased;
                next = byte_stream.next() => next,
                _ = wait_cancel(cancel_token.clone()), if cancel_token.is_some() => {
                    tracing::info!(
                        target: "synthia_provider::anthropic",
                        grace_ms = CANCEL_GRACE.as_millis() as u64,
                        "Anthropic stream cancellation requested; draining body up to grace period"
                    );
                    // Best-effort: drain remaining bytes within the grace period,
                    // then abort. We do not return early to keep the body
                    // terminator race-safe.
                    let drain = tokio::time::timeout(
                        CANCEL_GRACE,
                        async {
                            while let Some(item) = byte_stream.next().await {
                                if item.is_err() { break; }
                            }
                        }
                    ).await;
                    return Err(Error::stream_aborted(format!(
                        "stream aborted by caller (drained={})",
                        drain.is_ok()
                    )));
                }
            };

            let Some(chunk_result) = next else {
                break; // upstream closed
            };
            let bytes = chunk_result
                .map_err(|e| Error::stream_http_failure(e.to_string()))?;
            buf.extend_from_slice(&bytes);

            // Process complete SSE lines from the buffer. SSE separates
            // events with `\n\n`; we split on `\n` and parse each `data: ` line.
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim_end_matches('\n').trim();
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
            }
        }

        // Drain any trailing non-newline-terminated line.
        if !buf.is_empty() {
            let tail = String::from_utf8_lossy(&buf).trim().to_string();
            if let Some(data) = tail.strip_prefix("data: ")
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
        }

        // 3) Reconstruct a CompletionResponse so callers that still want
        //    a "response" struct get the assembled view. If the upstream
        //    never emitted IsDone (network cut-off mid-stream), we fall
        //    back to the assembled partial sampling result.
        let sampling = final_sampling.unwrap_or_default();
        let content = if sampling.tool_calls.is_empty() {
            crate::types::Content::Single(crate::types::ContentPart::Text(
                crate::types::TextContent {
                    text: sampling.text.clone(),
                    cache_control: None,
                },
            ))
        } else {
            crate::types::Content::Multi(
                sampling
                    .tool_calls
                    .iter()
                    .cloned()
                    .map(crate::types::ContentPart::ToolUse)
                    .collect(),
            )
        };
        Ok(CompletionResponse {
            id: ulid::Ulid::generate().to_string(),
            model: request.model,
            content,
            usage: sampling.usage.clone(),
            cached: false,
            stop_reason: sampling.stop_reason.clone(),
        })
    }
}

/// `tokio::select!` arm that resolves when the (optional) cancellation
/// token fires. Returns `Pending` when no token is supplied, so the
/// select! arm is never taken — keeping the loop purely driven by the
/// byte stream.
pub(super) async fn wait_cancel(token: Option<CancellationToken>) {
    if let Some(t) = token {
        t.cancelled().await;
    } else {
        // Park forever. The biased select! above plus the manual
        // is_cancelled() check at the top of the loop means we never
        // actually park here in practice; the future is cancelled when
        // its corresponding select! branch is disabled by the guard.
        std::future::pending::<()>().await;
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
