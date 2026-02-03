use std::{pin::Pin, sync::Arc};

use backoff::future::retry;
use futures::{Stream, StreamExt, TryStreamExt};
use reqwest::{Client, ClientBuilder, StatusCode, Url};
use serde_json::Value;
use tokio::time;
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    io::StreamReader,
    sync::CancellationToken,
};
use tracing::instrument;

use crate::{ProviderError, Result};

pub(super) type LinesStream =
    Pin<Box<dyn Stream<Item = Result<String>> + Send + 'static>>;

const CONTEXT_LENGTH_PHRASES: &[&str] = &[
    "too long",
    "context length",
    "context_length_exceeded",
    "reduce the length",
    "token count",
    "exceeds",
    "exceed context limit",
    "input length",
    "max_tokens",
    "decrease input length",
    "context limit",
    "maximum prompt length",
];

#[derive(Debug, Clone)]
pub(crate) struct BaseProvider {
    pub(crate) client: Arc<Client>,
    pub(crate) api_key: Option<String>,
    pub(crate) base_url: String,
}

impl BaseProvider {
    pub(crate) fn new(base_url: &str, api_key: Option<String>) -> Result<Self> {
        let client = ClientBuilder::new()
            .pool_max_idle_per_host(100)
            .build()
            .map_err(|e| ProviderError::api(e.to_string()))?;

        Ok(Self {
            client: Arc::new(client),
            api_key,
            base_url: base_url.to_string(),
        })
    }

    pub(crate) fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.to_string();
        self
    }

    pub(crate) fn with_api_key(mut self, api_key: &str) -> Self {
        self.api_key = Some(api_key.to_string());
        self
    }

    #[instrument(skip(request_builder, cancel_token))]
    pub(crate) async fn send_with_cancel(
        request_builder: reqwest::RequestBuilder,
        cancel_token: CancellationToken,
    ) -> Result<reqwest::Response> {
        tokio::select! {
            response = request_builder.send() => {
                response.map_err(ProviderError::HttpError)
            }
            _ = cancel_token.cancelled() => {
                Err(ProviderError::Cancelled)
            }
        }
    }

    pub(crate) async fn with_retry_cancellable<F, T>(
        &self,
        mut f: F,
        cancel_token: CancellationToken,
    ) -> Result<T>
    where
        F: FnMut() -> std::pin::Pin<
            Box<dyn futures::Future<Output = Result<T>> + Send>,
        >,
    {
        use backoff::ExponentialBackoff;

        if cancel_token.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }

        let backoff = ExponentialBackoff {
            initial_interval: time::Duration::from_millis(200),
            max_interval: time::Duration::from_secs(2),
            max_elapsed_time: Some(time::Duration::from_secs(10)),
            ..ExponentialBackoff::default()
        };

        retry(backoff, || {
            let future = f();
            Box::pin(async move {
                future.await.map_err(|e| {
                    // Classify errors as transient or permanent
                    // Transient errors should be retried, permanent errors should not
                    if Self::is_transient_error(&e) {
                        backoff::Error::transient(e)
                    } else {
                        backoff::Error::permanent(e)
                    }
                })
            })
        })
        .await
    }

    fn is_transient_error(error: &ProviderError) -> bool {
        match error {
            // Rate limit errors should be retried
            ProviderError::RateLimitError(_) => true,
            // Timeout errors should be retried
            ProviderError::Timeout => true,
            // HTTP errors from reqwest (network issues) should be retried
            ProviderError::HttpError(_) => true,
            // Server errors (5xx) should be retried
            ProviderError::ApiError(msg) if msg.contains("Server error") => {
                true
            }
            // Authentication errors should NOT be retried
            ProviderError::AuthenticationError(_) => false,
            // API errors (4xx) should NOT be retried unless specifically transient
            ProviderError::ApiError(_) => false,
            // Other errors should not be retried
            _ => false,
        }
    }

    pub(crate) fn check_context_length_exceeded(text: &str) -> bool {
        let text_lower = text.to_lowercase();
        CONTEXT_LENGTH_PHRASES
            .iter()
            .any(|phrase| text_lower.contains(phrase))
    }

    pub(crate) fn map_http_error_to_provider_error(
        status: StatusCode,
        payload: Option<Value>,
        check_context: bool,
    ) -> ProviderError {
        let message = payload
            .as_ref()
            .and_then(|p| {
                p.get("error")
                    .and_then(|e| e.get("message"))
                    .or_else(|| p.get("message"))
                    .and_then(|m| m.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| {
                payload
                    .as_ref()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default()
            });

        match status {
            StatusCode::OK => {
                unreachable!("Should not call this function with OK status")
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                ProviderError::authentication(format!(
                    "Authentication failed. Status: {status}. Response: {message}"
                ))
            }
            StatusCode::NOT_FOUND => ProviderError::api(format!(
                "Resource not found (404): {message}"
            )),
            StatusCode::PAYLOAD_TOO_LARGE => ProviderError::api(message),
            StatusCode::BAD_REQUEST => {
                if check_context
                    && Self::check_context_length_exceeded(&message)
                {
                    ProviderError::api(message)
                } else {
                    ProviderError::api(format!("Bad request (400): {message}"))
                }
            }
            StatusCode::TOO_MANY_REQUESTS => ProviderError::rate_limit(
                format!("Rate limit exceeded: {message}"),
            ),
            _ if status.is_server_error() => ProviderError::api(format!(
                "Server error ({status}): {message}"
            )),
            _ => ProviderError::api(format!(
                "Request failed with status {status}: {message}"
            )),
        }
    }

    pub(crate) async fn handle_response_status(
        &self,
        response: reqwest::Response,
    ) -> Result<reqwest::Response> {
        self.handle_response_status_with_context_check(response, false)
            .await
    }

    pub(crate) async fn handle_response_status_with_context_check(
        &self,
        response: reqwest::Response,
        check_context: bool,
    ) -> Result<reqwest::Response> {
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            tracing::error!("Request failed with status {status}: {body}");
            let payload = serde_json::from_str::<Value>(&body).ok();
            return Err(Self::map_http_error_to_provider_error(
                status,
                payload,
                check_context,
            ));
        }
        Ok(response)
    }

    pub(crate) fn build_url(&self, base_url: &str, path: &str) -> Result<Url> {
        let mut url = Url::parse(base_url)
            .map_err(|e| ProviderError::api(e.to_string()))?;
        let path = path.trim_start_matches('/');

        let current_path = url.path();
        let normalized_current_path = current_path.trim_end_matches('/');
        let normalized_path = path.trim_end_matches('/');

        if !normalized_current_path.ends_with(normalized_path) {
            let new_path = if normalized_current_path.is_empty()
                || normalized_current_path == "/"
            {
                path.to_string()
            } else {
                format!("{normalized_current_path}/{normalized_path}")
            };
            url.set_path(&new_path);
        }

        Ok(url)
    }

    pub(crate) fn create_cancellable_lines_stream(
        response: reqwest::Response,
        cancel_token: CancellationToken,
    ) -> LinesStream {
        let stream = response.bytes_stream().map_err(std::io::Error::other);
        let stream_reader = StreamReader::new(stream);
        let framed = FramedRead::new(stream_reader, LinesCodec::new())
            .map_err(|e| ProviderError::api(e.to_string()));

        let cancellable = futures::stream::unfold(
            (framed, cancel_token),
            |(mut stream, token)| async move {
                if token.is_cancelled() {
                    return None;
                }
                match stream.next().await {
                    Some(Ok(line)) => Some((Ok(line), (stream, token))),
                    Some(Err(e)) => Some((Err(e), (stream, token))),
                    None => None,
                }
            },
        );
        Box::pin(cancellable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_http_error_to_provider_error() {
        let error = BaseProvider::map_http_error_to_provider_error(
            StatusCode::UNAUTHORIZED,
            Some(serde_json::json!({
                "error": {
                    "message": "Invalid API key"
                }
            })),
            false,
        );
        assert!(matches!(error, ProviderError::AuthenticationError(_)));

        let error = BaseProvider::map_http_error_to_provider_error(
            StatusCode::TOO_MANY_REQUESTS,
            Some(serde_json::json!({
                "error": {
                    "message": "Rate limit exceeded"
                }
            })),
            false,
        );
        assert!(matches!(error, ProviderError::RateLimitError(_)));

        let error = BaseProvider::map_http_error_to_provider_error(
            StatusCode::NOT_FOUND,
            Some(serde_json::json!({
                "error": {
                    "message": "Resource not found"
                }
            })),
            false,
        );
        assert!(matches!(error, ProviderError::ApiError(_)));
    }

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
