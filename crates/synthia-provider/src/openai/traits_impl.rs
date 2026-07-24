//! `impl ModelProvider for OpenAICompatibleProvider` and the
//! `wait_cancel_openai` `tokio::select!` arm helper.

use async_trait::async_trait;
use futures::StreamExt;
use synthia_core::Error;
use tokio_util::sync::CancellationToken;

use super::{
    provider::OpenAICompatibleProvider,
    types::{OpenAIEmbeddingRequest, OpenAIEmbeddingResponse, OpenAIResponse},
};
use crate::{
    openai_streaming::OpenAIStreamProcessorV2,
    traits::ModelProvider,
    types::{
        CompletionRequest,
        CompletionResponse,
        Content,
        ContentPart,
        ModelConfig,
        ProviderConfig,
        SamplingResult,
        StreamChunk,
        TextContent,
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
            RetryConfig,
            parse_retry_after,
            retry_with_backoff,
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
                    return Err(Error::RateLimited(retry_after));
                }
                if !status.is_success() {
                    let message = response.text().await.unwrap_or_default();
                    return Err(Error::RequestFailed {
                        status: status.as_u16(),
                        message,
                    });
                }
                response.json::<OpenAIResponse>().await.map_err(Error::from)
            }
        })
        .await
        {
            Ok(r) => r,
            Err(e) => {
                #[cfg(feature = "otel")]
                {
                    llm_span.record("exception.type", e.code().to_string());
                    llm_span.record("exception.message", e.to_string());
                    llm_span.record("otel.status_code", "ERROR");
                }
                return Err(e);
            }
        };

        // Record success attributes from the raw OpenAI response
        // (which carries `finish_reason` + `usage` before they are
        // transformed into `CompletionResponse`).
        #[cfg(feature = "otel")]
        {
            if let Some(choice) = resp.choices.first()
                && let Some(reason) = choice.finish_reason.as_deref()
            {
                llm_span.record("gen_ai.response.finish_reason", reason);
            }
            llm_span
                .record("gen_ai.usage.input_tokens", resp.usage.prompt_tokens);
            llm_span.record(
                "gen_ai.usage.output_tokens",
                resp.usage.completion_tokens,
            );
        }

        let resp_json = serde_json::to_string(&resp).unwrap_or_default();
        tracing::info!(target: "synthia_provider::openai::debug",
            response = %resp_json,
            response_len = resp_json.len(),
            "OpenAI incoming response body"
        );

        Ok(self.parse_response(&resp))
    }

    async fn complete_with_stream(
        &self,
        request: CompletionRequest,
        cancel_token: Option<CancellationToken>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        // 1) Build the streaming body. `transform_request` sets
        //    `stream: false`; force it to true so the upstream sends SSE.
        let url = format!("{}/chat/completions", self.base_url);
        let mut body = self.transform_request(&request);
        body.stream = true;
        // Ask for token usage on the final chunk (OpenAI extension).
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

        let mut req = self.client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await.map_err(|e| {
            Error::stream_error(
                synthia_core::StreamErrorKind::HttpFailure,
                e.to_string(),
            )
        })?;
        let status = resp.status();
        if status.as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(crate::retry::parse_retry_after);
            return Err(Error::RateLimited(retry_after));
        }
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(Error::RequestFailed {
                status: status.as_u16(),
                message,
            });
        }

        // 2) Pull SSE bytes. Cancellation triggers a 5s drain-then-abort
        //    grace period, mirroring the Anthropic implementation.
        const CANCEL_GRACE: std::time::Duration =
            std::time::Duration::from_secs(5);
        let mut processor = OpenAIStreamProcessorV2::new();
        let mut byte_stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut final_sampling: Option<SamplingResult> = None;

        loop {
            if let Some(token) = &cancel_token
                && token.is_cancelled()
            {
                return Err(Error::stream_error(
                    synthia_core::StreamErrorKind::Aborted,
                    "stream cancelled by caller",
                ));
            }

            let next = tokio::select! {
                biased;
                next = byte_stream.next() => next,
                _ = wait_cancel_openai(cancel_token.clone()), if cancel_token.is_some() => {
                    tracing::info!(
                        target: "synthia_provider::openai",
                        grace_ms = CANCEL_GRACE.as_millis() as u64,
                        "OpenAI stream cancellation requested; draining body up to grace period"
                    );
                    let drain = tokio::time::timeout(
                        CANCEL_GRACE,
                        async {
                            while let Some(item) = byte_stream.next().await {
                                if item.is_err() { break; }
                            }
                        }
                    ).await;
                    return Err(Error::stream_error(
                        synthia_core::StreamErrorKind::Aborted,
                        format!("stream aborted by caller (drained={})", drain.is_ok()),
                    ));
                }
            };

            let Some(chunk_result) = next else {
                break; // upstream closed
            };
            let bytes = chunk_result.map_err(|e| {
                Error::stream_error(
                    synthia_core::StreamErrorKind::HttpFailure,
                    e.to_string(),
                )
            })?;
            buf.extend_from_slice(&bytes);

            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim_end_matches('\n').trim();
                // OpenAI uses both `data: ` (with space) and the bare
                // sentinel `data:[DONE]`. Tolerate either.
                let data = line.strip_prefix("data:").map(|s| s.trim_start());
                if let Some(data) = data {
                    for chunk in processor.process_line(data) {
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
            let data = tail.strip_prefix("data:").map(|s| s.trim_start());
            if let Some(data) = data {
                for chunk in processor.process_line(data) {
                    if let StreamChunk::IsDone { result } = &chunk {
                        final_sampling = Some((**result).clone());
                    }
                    on_delta(chunk);
                }
            }
        }

        // 3) Reconstruct a CompletionResponse so callers that still want
        //    a "response" struct get the assembled view.
        let sampling = final_sampling.unwrap_or_default();
        let content = if sampling.tool_calls.is_empty() {
            Content::Single(ContentPart::Text(TextContent {
                text: sampling.text.clone(),
                cache_control: None,
            }))
        } else {
            Content::Multi(
                sampling
                    .tool_calls
                    .iter()
                    .cloned()
                    .map(ContentPart::ToolUse)
                    .collect(),
            )
        };
        Ok(CompletionResponse {
            id: ulid::Ulid::new().to_string(),
            model: request.model,
            content,
            usage: sampling.usage.clone(),
            cached: false,
        })
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

        tracing::info!(target: "synthia_provider::openai::debug",
            url = %url,
            body = %body_json,
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
            return Err(Error::RequestFailed {
                status: status.as_u16(),
                message,
            });
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

/// `tokio::select!` arm that resolves when the (optional) cancellation
/// token fires. Returns `Pending` when no token is supplied, so the
/// select! arm is never taken — keeping the loop purely driven by the
/// byte stream.
pub(super) async fn wait_cancel_openai(token: Option<CancellationToken>) {
    if let Some(t) = token {
        t.cancelled().await;
    } else {
        std::future::pending::<()>().await;
    }
}
