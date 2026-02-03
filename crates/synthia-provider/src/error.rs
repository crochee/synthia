use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Rate limit error: {0}")]
    RateLimitError(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Missing API key")]
    MissingApiKey,

    #[error("Unsupported model: {0}")]
    UnsupportedModel(String),

    #[error("Timeout")]
    Timeout,

    #[error("Request cancelled")]
    Cancelled,
}

impl ProviderError {
    pub fn api(message: impl Into<String>) -> Self {
        Self::ApiError(message.into())
    }

    pub fn authentication(message: impl Into<String>) -> Self {
        Self::AuthenticationError(message.into())
    }

    pub fn rate_limit(message: impl Into<String>) -> Self {
        Self::RateLimitError(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error() {
        let error = ProviderError::api("Test API error");
        assert_eq!(format!("{error}"), "API error: Test API error");
    }

    #[test]
    fn test_authentication_error() {
        let error = ProviderError::authentication("Test auth error");
        assert_eq!(format!("{error}"), "Authentication error: Test auth error");
    }

    #[test]
    fn test_rate_limit_error() {
        let error = ProviderError::rate_limit("Test rate limit error");
        assert_eq!(
            format!("{error}"),
            "Rate limit error: Test rate limit error"
        );
    }

    #[test]
    fn test_invalid_response_error() {
        let error =
            ProviderError::InvalidResponse("Test invalid response".into());
        assert_eq!(
            format!("{error}"),
            "Invalid response: Test invalid response"
        );
    }

    #[test]
    fn test_missing_api_key_error() {
        let error = ProviderError::MissingApiKey;
        assert_eq!(format!("{error}"), "Missing API key");
    }

    #[test]
    fn test_unsupported_model_error() {
        let error = ProviderError::UnsupportedModel("test-model".into());
        assert_eq!(format!("{error}"), "Unsupported model: test-model");
    }

    #[test]
    fn test_timeout_error() {
        let error = ProviderError::Timeout;
        assert_eq!(format!("{error}"), "Timeout");
    }

    #[test]
    fn test_cancelled_error() {
        let error = ProviderError::Cancelled;
        assert_eq!(format!("{error}"), "Request cancelled");
    }
}
