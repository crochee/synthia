use std::time::Duration;

use synthia_core::Error;

#[derive(Debug, Clone, Default)]
pub enum RetryPolicy {
    #[default]
    Default,
    Aggressive,
    Conservative,
    Custom(RetryConfig),
}

impl RetryPolicy {
    pub fn config(&self) -> RetryConfig {
        match self {
            RetryPolicy::Default => RetryConfig::default(),
            RetryPolicy::Aggressive => RetryConfig {
                max_attempts: 5,
                initial_interval_ms: 500,
                max_interval_ms: 15000,
                max_elapsed_ms: 120000,
            },
            RetryPolicy::Conservative => RetryConfig {
                max_attempts: 1,
                initial_interval_ms: 5000,
                max_interval_ms: 30000,
                max_elapsed_ms: 60000,
            },
            RetryPolicy::Custom(config) => config.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_interval_ms: u64,
    pub max_interval_ms: u64,
    pub max_elapsed_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_interval_ms: 1000,
            max_interval_ms: 10000,
            max_elapsed_ms: 60000,
        }
    }
}

pub fn is_retryable_error(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// Extract Retry-After duration from a rate limit response.
/// Supports both integer seconds and HTTP date formats.
pub fn parse_retry_after(header_value: &str) -> Option<Duration> {
    if let Ok(seconds) = header_value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(header_value) {
        let now = chrono::Utc::now();
        let diff = date.with_timezone(&chrono::Utc) - now;
        if diff.num_seconds() > 0 {
            return Some(Duration::from_secs(diff.num_seconds() as u64));
        }
    }
    None
}

pub async fn retry_with_backoff<F, Fut, T>(
    config: RetryConfig,
    mut operation: F,
) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, Error>>,
{
    let mut attempts = 0u32;
    let mut delay_ms = config.initial_interval_ms;
    let start = std::time::Instant::now();

    loop {
        attempts += 1;

        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !e.is_retryable() || attempts >= config.max_attempts {
                    return Err(e);
                }

                let elapsed_ms = start.elapsed().as_millis() as u64;
                if elapsed_ms >= config.max_elapsed_ms {
                    return Err(Error::retry_exhausted(attempts, e));
                }

                if e.is_rate_limited()
                    && let Error::RateLimited {
                        retry_after: Some(retry_after),
                        ..
                    } = &e
                {
                    tokio::time::sleep(*retry_after).await;
                    continue;
                }

                let actual_delay = delay_ms.min(config.max_interval_ms);
                delay_ms = (delay_ms * 2).min(config.max_interval_ms);
                tokio::time::sleep(Duration::from_millis(actual_delay)).await;
            }
        }
    }
}

/// Retry wrapper that respects Retry-After header for rate limits.
/// If a RateLimited error with a Retry-After duration is encountered,
/// wait for that duration instead of exponential backoff.
pub async fn retry_with_retry_after<F, Fut, T>(
    config: RetryConfig,
    retry_after_header: Option<String>,
    mut operation: F,
) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, Error>>,
{
    let mut attempts = 0u32;
    let mut delay_ms = config.initial_interval_ms;
    let start = std::time::Instant::now();

    loop {
        attempts += 1;

        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !e.is_retryable() || attempts >= config.max_attempts {
                    return Err(e);
                }

                let elapsed_ms = start.elapsed().as_millis() as u64;
                if elapsed_ms >= config.max_elapsed_ms {
                    return Err(Error::retry_exhausted(attempts, e));
                }

                if e.is_rate_limited() {
                    if let Error::RateLimited {
                        retry_after: Some(duration),
                        ..
                    } = &e
                    {
                        tokio::time::sleep(*duration).await;
                        continue;
                    }
                    if let Some(ref header) = retry_after_header
                        && let Some(duration) = parse_retry_after(header)
                    {
                        tokio::time::sleep(duration).await;
                        continue;
                    }
                }

                let actual_delay = delay_ms.min(config.max_interval_ms);
                delay_ms = (delay_ms * 2).min(config.max_interval_ms);
                tokio::time::sleep(Duration::from_millis(actual_delay)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error(429));
        assert!(is_retryable_error(500));
        assert!(is_retryable_error(503));
        assert!(!is_retryable_error(400));
        assert!(!is_retryable_error(401));
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_interval_ms, 1000);
        assert_eq!(config.max_interval_ms, 10000);
    }

    #[test]
    fn test_parse_retry_after_seconds() {
        let duration = parse_retry_after("30");
        assert_eq!(duration, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_parse_retry_after_invalid() {
        let duration = parse_retry_after("not-a-number");
        assert!(duration.is_none());
    }
}
