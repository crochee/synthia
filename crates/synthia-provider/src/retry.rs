use std::time::Duration;

use synthia_core::{Clock, Error};

use crate::{
    context_overflow::ContextOverflowDetector,
    error_body::{ProviderErrorBody, parse_provider_error_body},
};

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
///
/// The HTTP-date form needs "now" to turn a timestamp into a delay;
/// that read goes through [`synthia_core::Clock`]
/// (see [`parse_retry_after_at`] for the injectable form), so a test
/// can pin the arithmetic instead of racing the wall clock.
pub fn parse_retry_after(header_value: &str) -> Option<Duration> {
    parse_retry_after_at(
        header_value,
        synthia_core::SharedClock::system().now(),
    )
}

/// [`parse_retry_after`] against an explicit "now".
///
/// Integer-second headers ignore `now` entirely; the HTTP-date form
/// subtracts it.
pub fn parse_retry_after_at(
    header_value: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<Duration> {
    if let Ok(seconds) = header_value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(header_value) {
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
    Fut: Future<Output = Result<T, Error>>,
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
    Fut: Future<Output = Result<T, Error>>,
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

// ---------------------------------------------------------------------------
// Typed retry classification (R4 Phase C.1 — dsh llm-retry semantics)
// ---------------------------------------------------------------------------
/// Failure class for a provider error, each with its own retry
/// budget and backoff shape (dsh `ResolvedRetryPolicy` semantics).
///
/// Classification is the retry decision: [`classify_error`]
/// inspects the [`Error`] variant (and, for provider-opaque
/// payloads, well-known message markers) and returns the class;
/// [`retry_config_for`] maps the class to a [`RetryConfig`].
/// `Permanent` and `Quota` map to a single attempt — no retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RetryClass {
    /// Quota / throttling (HTTP 429, `RateLimited`). Retries
    /// with the largest budget — quota windows recover and the
    /// caller has already committed to waiting.
    RateLimit,
    /// Provider capacity (HTTP 5xx, Anthropic
    /// `overloaded_error`). Bounded retries with short initial
    /// delay — overload bursts usually clear in seconds.
    Overloaded,
    /// Network / timeout / stream hiccups. The pre-R4 default
    /// budget (3 attempts, 1s → 10s).
    Transient,
    /// Degenerate empty terminal completion: the provider
    /// reported a normal stop but produced no content at all
    /// (dsh `EMPTY_RESPONSE`). Retryable with a small budget —
    /// the failure is often a transient sampling artefact, and
    /// one cheap re-ask usually recovers it.
    EmptyResponse,
    /// Exhausted account quota, balance, credits, or billing
    /// (HTTP 402, dsh `QUOTA`). Distinct from `RateLimit`
    /// because the failure is NOT retryable within a session:
    /// one attempt, so callers can report "quota exhausted"
    /// instead of pretending a retry might succeed.
    Quota,
    /// Context-window overflow (`context_length_exceeded`,
    /// "prompt is too long"). One attempt: the same request cannot
    /// fit, so the caller must shrink the prompt instead of
    /// sleeping. Reported from a structured error body only —
    /// text-only overflow wording still classifies by status.
    ContextOverflow,
    /// Authentication / authorization failure (`invalid_api_key`,
    /// `permission_error`). One attempt: a credential problem needs
    /// a new key or a scope change, not a retry. Reported from a
    /// structured error body only — a text-only 401/403 stays
    /// `Permanent`.
    Auth,
    /// Auth, validation, model-not-found, and other caller-side
    /// failures. Retrying cannot succeed; exactly one attempt.
    Permanent,
}

/// Classify a provider failure into its [`RetryClass`].
///
/// Variant-driven first (`RateLimited`, `RequestFailed`, `Timeout`),
/// then marker sniffing for provider-opaque `Provider` payloads: the
/// typed empty-completion marker
/// ([`crate::validation::is_empty_response_error`] — dsh
/// `EMPTY_RESPONSE`), quota/billing wording (dsh `QUOTA`),
/// `"overloaded_error"` — Anthropic, `"rate_limit"` —
/// OpenAI/Anthropic message text. Everything else is
/// [`RetryClass::Permanent`]: the caller-side error taxonomy
/// (auth, validation, not-found) never benefits from a retry.
///
/// `RequestFailed` carries the status *and* the raw HTTP body, so it
/// is classified by [`classify_provider_error_body`]: a body the
/// provider shaped itself decides the class from its own
/// `type`/`code`/`message` (an OpenAI 429 with `insufficient_quota`
/// is [`RetryClass::Quota`], not [`RetryClass::RateLimit`]), while a
/// text-only body keeps the status mapping it had before R33 —
/// 429 `RateLimit`, 402 `Quota`, 5xx `Overloaded`, everything else
/// `Permanent`.
#[must_use]
pub fn classify_error(err: &Error) -> RetryClass {
    if crate::validation::is_empty_response_error(err) {
        return RetryClass::EmptyResponse;
    }
    match err {
        Error::RateLimited { .. } => RetryClass::RateLimit,
        Error::RequestFailed { status, message } => {
            classify_provider_error_body(*status, message)
        }
        Error::Timeout { .. }
        | Error::Io { .. }
        | Error::Stream { .. }
        | Error::StreamHttpFailure { .. }
        | Error::StreamProtocolError { .. }
        | Error::StreamInternal { .. } => RetryClass::Transient,
        Error::Provider { message } => {
            let normalized = normalize_markers(message);
            if is_quota_marker(&normalized) {
                RetryClass::Quota
            } else if normalized.contains("overloaded") {
                RetryClass::Overloaded
            } else if normalized.contains("rate limit") {
                RetryClass::RateLimit
            } else {
                RetryClass::Permanent
            }
        }
        _ => RetryClass::Permanent,
    }
}

/// Classify an HTTP failure from its status and raw body — the pair
/// the adapters already store in [`Error::RequestFailed`].
///
/// Precedence:
///
/// 1. a **quota/billing** body — the one signal that outranks the
///    status, because an exhausted account cannot recover by waiting
///    and OpenAI reports it with the same 429 it uses for
///    request-rate limits;
/// 2. a **capacity status** (5xx) — the status is its own signal, so
///    wording like "authentication service unavailable" must not
///    turn a recoverable overload into a caller-side failure;
/// 3. the rest of the body ladder (context overflow, auth, overload
///    wording, request rate);
/// 4. `status_class`, the pre-R33 mapping — a body with no
///    recognized signal (non-JSON, or a `type`/`code` outside the
///    marker vocabulary) classifies exactly as it did before R33.
#[must_use]
pub fn classify_provider_error_body(status: u16, raw: &str) -> RetryClass {
    let body = parse_provider_error_body(status, raw);
    let Some(markers) = body_markers(&body) else {
        return status_class(status);
    };
    if is_quota_marker(&markers) {
        return RetryClass::Quota;
    }
    if is_capacity_status(status) {
        return RetryClass::Overloaded;
    }
    classify_body_markers(&markers).unwrap_or(status_class(status))
}

/// Status-only mapping for a body that carries no structured signal.
fn status_class(status: u16) -> RetryClass {
    match status {
        429 => RetryClass::RateLimit,
        402 => RetryClass::Quota,
        _ if is_capacity_status(status) => RetryClass::Overloaded,
        _ => RetryClass::Permanent,
    }
}

/// True for the statuses that mean "the provider itself is strained".
fn is_capacity_status(status: u16) -> bool {
    matches!(status, 500 | 502 | 503 | 504)
}

/// Fold a body's structured signals into one normalized marker
/// string, or `None` when the body carried none at all (non-JSON
/// body, or a JSON object without `type`/`code`/`message`).
fn body_markers(body: &ProviderErrorBody) -> Option<String> {
    let parts: Vec<&str> = [
        body.kind.as_deref(),
        body.code.as_deref(),
        body.message.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        return None;
    }
    Some(normalize_markers(&parts.join(" ")))
}

/// Body ladder for a non-capacity status. Precedence: an account that
/// ran out of context is reported with the same `invalid_request_error`
/// as any other bad request, so overflow is tested before auth, and
/// both before the rate wording every one of these messages carries.
/// `None` means "no recognized signal" — the caller falls back to the
/// status mapping.
fn classify_body_markers(normalized: &str) -> Option<RetryClass> {
    if ContextOverflowDetector::new().is_overflow(normalized) {
        return Some(RetryClass::ContextOverflow);
    }
    if is_auth_marker(normalized) {
        return Some(RetryClass::Auth);
    }
    if normalized.contains("overloaded") {
        return Some(RetryClass::Overloaded);
    }
    if normalized.contains("rate limit") {
        return Some(RetryClass::RateLimit);
    }
    None
}

/// Credential / permission wording: a new key or a scope change is
/// required, so no amount of retrying with the same credential helps.
/// Operates on [`normalize_markers`] output, so `invalid_api_key`
/// matches the same phrase as `invalid api key`.
fn is_auth_marker(normalized: &str) -> bool {
    normalized.contains("authentication")
        || normalized.contains("permission")
        || normalized.contains("unauthorized")
        || normalized.contains("forbidden")
        || normalized.contains("access denied")
        || normalized.contains("api key")
        || normalized.contains("invalid token")
        || normalized.contains("credential")
}

/// Lowercase a provider message and fold `_` / `-` separators into
/// spaces so one phrase list matches `insufficient_quota`,
/// `insufficient-quota`, and `insufficient quota` alike.
fn normalize_markers(message: &str) -> String {
    message.to_lowercase().replace(['_', '-'], " ")
}

/// Quota/billing-wording detector (dsh `isQuotaExceededError`
/// parity): exhausted account quota, balance, credits, budget, or
/// usage limits.
///
/// Operates on [`normalize_markers`] output, so all phrases are
/// space-separated.
fn is_quota_marker(normalized: &str) -> bool {
    let exhausted = normalized.contains("exceeded")
        || normalized.contains("exhausted")
        || normalized.contains("reached")
        || normalized.contains("depleted");
    normalized.contains("insufficient quota")
        || normalized.contains("insufficient balance")
        || normalized.contains("insufficient credit")
        || normalized.contains("out of credits")
        || normalized.contains("out of budget")
        || (exhausted
            && (normalized.contains("quota")
                || normalized.contains("usage limit")
                || normalized.contains("billing")))
}

/// The per-class retry budget (dsh defaults: 2 retries, 500ms →
/// 10s; tuned per class).
///
/// - `RateLimit` — 6 attempts, 1s → 30s: quota windows are the
///   longest-recovering failure and the Retry-After hint (when
///   present) already gates the first sleep.
/// - `Overloaded` — 4 attempts, 500ms → 10s: capacity bursts
///   clear fast; dsh's default shape.
/// - `Transient` — the pre-R4 [`RetryConfig::default`] (3
///   attempts, 1s → 10s), unchanged behaviour.
/// - `EmptyResponse` — 2 attempts, 500ms → 10s: dsh's default
///   bounded policy; one cheap re-ask recovers most degenerate
///   completions, and the budget must not turn into a spin.
/// - `Quota` — 1 attempt, no retry: exhausting an account quota
///   needs an operator (or a different credential), not a sleep.
/// - `ContextOverflow` — 1 attempt, no retry: the same request
///   cannot fit, the caller has to shrink the prompt.
/// - `Auth` — 1 attempt, no retry: a credential problem needs a new
///   key, not a sleep.
/// - `Permanent` — 1 attempt, no retry.
#[must_use]
pub fn retry_config_for(class: RetryClass) -> RetryConfig {
    match class {
        RetryClass::RateLimit => RetryConfig {
            max_attempts: 6,
            initial_interval_ms: 1000,
            max_interval_ms: 30000,
            max_elapsed_ms: 120000,
        },
        RetryClass::Overloaded => RetryConfig {
            max_attempts: 4,
            initial_interval_ms: 500,
            max_interval_ms: 10000,
            max_elapsed_ms: 30000,
        },
        RetryClass::Transient => RetryConfig::default(),
        RetryClass::EmptyResponse => RetryConfig {
            max_attempts: 2,
            initial_interval_ms: 500,
            max_interval_ms: 10000,
            max_elapsed_ms: 15000,
        },
        RetryClass::Quota
        | RetryClass::ContextOverflow
        | RetryClass::Auth
        | RetryClass::Permanent => RetryConfig {
            max_attempts: 1,
            initial_interval_ms: 0,
            max_interval_ms: 0,
            max_elapsed_ms: 0,
        },
    }
}

/// Retry with per-class backoff: each failure is classified and
/// the class's budget governs the next sleep.
///
/// Unlike [`retry_with_backoff`] (one fixed config for every
/// failure), the budget adapts: a 429 keeps retrying longer than
/// a timeout, an empty completion gets one cheap re-ask, and the
/// terminal classes (`Quota`, `ContextOverflow`, `Auth`,
/// `Permanent`) return after the first attempt without sleeping.
/// The elapsed-time budget resets per
/// class — a run that starts `Transient` and later trips a rate
/// limit gets the full `RateLimit` budget.
pub async fn retry_with_classification<F, Fut, T>(
    mut operation: F,
) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut attempt: u32 = 0;
    let start = std::time::Instant::now();
    loop {
        attempt += 1;
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                let class = classify_error(&err);
                let config = retry_config_for(class);
                // `Quota`, `ContextOverflow`, `Auth` and `Permanent`
                // are terminal: return the classifier output after the
                // first attempt without consulting the elapsed budget.
                let terminal = matches!(
                    class,
                    RetryClass::Quota
                        | RetryClass::ContextOverflow
                        | RetryClass::Auth
                        | RetryClass::Permanent
                );
                if terminal || attempt >= config.max_attempts {
                    return Err(Error::RetryExhausted {
                        attempts: attempt,
                        last_error: Box::new(err),
                    });
                }
                if config.max_elapsed_ms > 0
                    && start.elapsed().as_millis() as u64
                        >= config.max_elapsed_ms
                {
                    return Err(Error::RetryExhausted {
                        attempts: attempt,
                        last_error: Box::new(err),
                    });
                }
                let backoff_ms = config
                    .initial_interval_ms
                    .saturating_mul(1_u64 << (attempt - 1).min(16))
                    .min(config.max_interval_ms);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_rate_limited_variant() {
        let e = Error::RateLimited { retry_after: None };
        assert_eq!(classify_error(&e), RetryClass::RateLimit);
    }

    /// `StreamAborted` must stay `Permanent`.
    ///
    /// The streaming adapters consult the cancel token inside their
    /// establishment closure and return `StreamAborted` to stop. That
    /// works *only* because the class is terminal: `Permanent` is
    /// `max_attempts: 1` with no sleep, so the retry exits at once.
    /// Move this variant into the `Transient` arm above — a plausible
    /// edit, since that arm already lists every other `Stream*`
    /// variant — and a cancelled run would instead sit in the backoff
    /// (1s + 2s for `Transient`) before noticing.
    ///
    /// The cancel tests in `tests/provider_test.rs` would still pass
    /// (still zero requests, because the guard fires first), so
    /// nothing else catches that edit.
    #[test]
    fn classify_stream_aborted_is_permanent() {
        let class = classify_error(&Error::stream_aborted("cancelled"));
        assert_eq!(class, RetryClass::Permanent);
        assert_eq!(
            retry_config_for(class).max_attempts,
            1,
            "a cancelled stream must not be retried at all"
        );
        assert_eq!(
            retry_config_for(class).max_elapsed_ms,
            0,
            "and must not sleep through a backoff"
        );
    }

    #[test]
    fn classify_request_failed_by_status() {
        let mk = |status: u16| Error::RequestFailed {
            status,
            message: "x".into(),
        };
        assert_eq!(classify_error(&mk(429)), RetryClass::RateLimit);
        assert_eq!(classify_error(&mk(500)), RetryClass::Overloaded);
        assert_eq!(classify_error(&mk(503)), RetryClass::Overloaded);
        assert_eq!(classify_error(&mk(400)), RetryClass::Permanent);
        assert_eq!(classify_error(&mk(401)), RetryClass::Permanent);
    }

    #[test]
    fn classify_transient_variants() {
        assert_eq!(
            classify_error(&Error::Timeout {
                message: "t".into()
            }),
            RetryClass::Transient
        );
        assert_eq!(
            classify_error(&Error::Stream {
                message: "s".into()
            }),
            RetryClass::Transient
        );
    }

    #[test]
    fn classify_provider_markers() {
        assert_eq!(
            classify_error(&Error::Provider {
                message: "anthropic overloaded_error".into()
            }),
            RetryClass::Overloaded
        );
        assert_eq!(
            classify_error(&Error::Provider {
                message: "rate_limit exceeded".into()
            }),
            RetryClass::RateLimit
        );
        assert_eq!(
            classify_error(&Error::Provider {
                message: "bad input".into()
            }),
            RetryClass::Permanent
        );
    }

    #[test]
    fn classify_permanent_variants() {
        assert_eq!(
            classify_error(&Error::Unauthorized {
                message: "u".into()
            }),
            RetryClass::Permanent
        );
        assert_eq!(
            classify_error(&Error::ModelNotFound {
                message: "m".into()
            }),
            RetryClass::Permanent
        );
        assert_eq!(
            classify_error(&Error::Validation {
                message: "v".into()
            }),
            RetryClass::Permanent
        );
    }

    #[test]
    fn classify_quota_markers() {
        // dsh `isQuotaExceededError` wording, in the shapes the
        // OpenAI/Anthropic-compatible providers actually emit.
        for message in [
            "insufficient_quota",
            "You exceeded your current quota, please check your plan",
            "billing hard limit reached",
            "usage limit exhausted",
            "insufficient balance",
            "your credit balance is out of credits",
        ] {
            assert_eq!(
                classify_error(&Error::Provider {
                    message: message.into()
                }),
                RetryClass::Quota,
                "must classify as Quota: {message}"
            );
        }
    }

    #[test]
    fn classify_quota_is_distinct_from_rate_limit() {
        // Request-rate wording stays RateLimit...
        for message in ["rate_limit exceeded", "Rate limit reached"] {
            assert_eq!(
                classify_error(&Error::Provider {
                    message: message.into()
                }),
                RetryClass::RateLimit,
                "must stay RateLimit: {message}"
            );
        }
        // ...and a TEXT-ONLY body stays status-mapped (contract):
        // 429 is the request-rate status, 402 the account-quota one.
        // A *structured* body gets to override the status — see
        // `classify_openai_quota_body_overrides_the_429_status`.
        assert_eq!(
            classify_error(&Error::RequestFailed {
                status: 429,
                message: "insufficient_quota".into()
            }),
            RetryClass::RateLimit
        );
        assert_eq!(
            classify_error(&Error::RequestFailed {
                status: 402,
                message: "Insufficient Balance".into()
            }),
            RetryClass::Quota
        );
    }

    /// Real error bodies from both providers, classified from the
    /// provider's own `type`/`code`/`message` rather than from the
    /// status — and each landing in its own class, which is the whole
    /// point of reading the body: OpenAI reports an exhausted account
    /// (quota), a request-rate limit, a context overflow and a bad
    /// credential all with overlapping statuses.
    #[test]
    fn classify_real_provider_error_bodies_into_distinct_classes() {
        let cases: [(u16, &str, RetryClass); 7] = [
            (
                429,
                r#"{"error":{"message":"You exceeded your current quota, please check your plan and billing details.","type":"insufficient_quota","param":null,"code":"insufficient_quota"}}"#,
                RetryClass::Quota,
            ),
            (
                429,
                r#"{"error":{"message":"Rate limit reached for gpt-4o in organization org-x on requests per min (RPM): Limit 500, Used 500.","type":"rate_limit_exceeded","param":null,"code":"rate_limit_exceeded"}}"#,
                RetryClass::RateLimit,
            ),
            (
                529,
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
                RetryClass::Overloaded,
            ),
            (
                400,
                r#"{"error":{"message":"This model's maximum context length is 128000 tokens. However, your messages resulted in 150000 tokens. Please reduce the length of the messages.","type":"invalid_request_error","param":"messages","code":"context_length_exceeded"}}"#,
                RetryClass::ContextOverflow,
            ),
            (
                400,
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 213000 tokens > 200000 maximum"}}"#,
                RetryClass::ContextOverflow,
            ),
            (
                401,
                r#"{"error":{"message":"Incorrect API key provided: sk-abc***. You can find your API key at https://platform.openai.com/account/api-keys.","type":"invalid_request_error","param":null,"code":"invalid_api_key"}}"#,
                RetryClass::Auth,
            ),
            (
                401,
                r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
                RetryClass::Auth,
            ),
        ];
        let classes: Vec<RetryClass> = cases
            .iter()
            .map(|(status, body, expected)| {
                let got =
                    classify_error(&Error::request_failed(*status, *body));
                assert_eq!(got, *expected, "body: {body}");
                got
            })
            .collect();
        let distinct: std::collections::HashSet<RetryClass> =
            classes.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            5,
            "quota, rate limit, capacity, overflow and auth must be \
             distinguishable, got {distinct:?}"
        );
    }

    /// The OpenAI 429 shape carries `insufficient_quota`: the body
    /// outranks the status, so the failure is not retried like a rate
    /// limit.
    #[test]
    fn classify_openai_quota_body_overrides_the_429_status() {
        let body = r#"{"error":{"message":"You exceeded your current quota","type":"insufficient_quota","code":"insufficient_quota"}}"#;
        assert_eq!(classify_provider_error_body(429, body), RetryClass::Quota);
        assert_eq!(retry_config_for(RetryClass::Quota).max_attempts, 1);
    }

    /// Regression (R33): a body the classifier cannot read as a
    /// structured signal keeps the pre-R33 status mapping — text-only
    /// bodies, HTML from a proxy, and JSON whose `type` is outside
    /// the marker vocabulary alike.
    #[test]
    fn classify_unrecognized_bodies_keep_the_status_class() {
        let cases: [(u16, &str, RetryClass); 10] = [
            (400, "bad request", RetryClass::Permanent),
            (401, "no", RetryClass::Permanent),
            (402, "Insufficient Balance", RetryClass::Quota),
            (403, "<html>forbidden</html>", RetryClass::Permanent),
            // A bare marker word is not a structured body: it stays
            // status-mapped until the provider shapes it as JSON.
            (429, "insufficient_quota", RetryClass::RateLimit),
            (429, "rate limited", RetryClass::RateLimit),
            (500, "internal server error", RetryClass::Overloaded),
            (503, "<html>bad gateway</html>", RetryClass::Overloaded),
            (529, "Overloaded", RetryClass::Permanent),
            (
                400,
                r#"{"error":{"type":"invalid_request_error","message":"unknown field"}}"#,
                RetryClass::Permanent,
            ),
        ];
        for (status, body, expected) in cases {
            assert_eq!(
                classify_provider_error_body(status, body),
                expected,
                "status {status}, body {body}"
            );
            assert_eq!(
                classify_error(&Error::request_failed(status, body)),
                expected
            );
        }
    }

    #[test]
    fn classify_empty_completion_marker() {
        let err = crate::validation::empty_response_error(
            "test-model",
            Some("end_turn"),
        );
        assert_eq!(classify_error(&err), RetryClass::EmptyResponse);
    }

    /// A 5xx is a capacity signal in its own right: body wording that
    /// would otherwise read as auth must not turn a recoverable
    /// overload into a caller-side failure and cost it its retry
    /// budget. An explicit quota body still wins — waiting cannot
    /// clear an exhausted account, whatever status carries it.
    #[test]
    fn classify_capacity_status_outranks_non_quota_body_wording() {
        let auth_wording = concat!(
            r#"{"error":{"type":"authentication_error","#,
            r#""message":"authentication service unavailable"}}"#
        );
        assert_eq!(
            classify_provider_error_body(503, auth_wording),
            RetryClass::Overloaded
        );
        assert_eq!(retry_config_for(RetryClass::Overloaded).max_attempts, 4);

        let quota = concat!(
            r#"{"error":{"type":"insufficient_quota","#,
            r#""message":"account quota exhausted"}}"#
        );
        assert_eq!(classify_provider_error_body(500, quota), RetryClass::Quota);
    }

    #[test]
    fn retry_config_for_new_class_budgets() {
        let empty = retry_config_for(RetryClass::EmptyResponse);
        assert_eq!(empty.max_attempts, 2, "small re-ask budget");
        assert_eq!(empty.initial_interval_ms, 500);
        assert!(empty.max_interval_ms <= 10000);

        let quota = retry_config_for(RetryClass::Quota);
        assert_eq!(quota.max_attempts, 1, "quota is not retryable");
    }

    #[tokio::test]
    async fn retry_with_classification_gives_up_on_quota() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let result: Result<(), Error> = retry_with_classification(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Err(Error::Provider {
                    message: "insufficient_quota".into(),
                })
            }
        })
        .await;
        let Err(Error::RetryExhausted {
            attempts,
            last_error,
        }) = result
        else {
            panic!("quota must surface as RetryExhausted");
        };
        assert_eq!(attempts, 1, "not retried within a session");
        assert_eq!(classify_error(&last_error), RetryClass::Quota);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_with_classification_reasks_empty_completion() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let result: Result<&'static str, Error> =
            retry_with_classification(|| {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n == 0 {
                        Err(crate::validation::empty_response_error(
                            "m",
                            Some("stop"),
                        ))
                    } else {
                        Ok("recovered")
                    }
                }
            })
            .await;
        assert_eq!(result.unwrap(), "recovered");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_with_classification_exhausts_empty_budget() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let result: Result<(), Error> = retry_with_classification(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Err(crate::validation::empty_response_error("m", Some("stop")))
            }
        })
        .await;
        let Err(Error::RetryExhausted {
            attempts,
            last_error,
        }) = result
        else {
            panic!("empty completions must exhaust their budget");
        };
        assert_eq!(attempts, 2, "2-attempt budget");
        assert!(crate::validation::is_empty_response_error(&last_error));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn retry_config_for_class_budgets() {
        assert_eq!(retry_config_for(RetryClass::RateLimit).max_attempts, 6);
        assert_eq!(retry_config_for(RetryClass::Overloaded).max_attempts, 4);
        assert_eq!(retry_config_for(RetryClass::Transient).max_attempts, 3);
        assert_eq!(retry_config_for(RetryClass::Permanent).max_attempts, 1);
        // Neither a prompt that cannot fit nor a rejected credential
        // becomes viable by waiting.
        assert_eq!(
            retry_config_for(RetryClass::ContextOverflow).max_attempts,
            1
        );
        assert_eq!(retry_config_for(RetryClass::Auth).max_attempts, 1);
    }

    #[tokio::test]
    async fn retry_with_classification_gives_up_on_permanent() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let result: Result<(), Error> = retry_with_classification(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Err(Error::Unauthorized {
                    message: "no".into(),
                })
            }
        })
        .await;
        assert!(matches!(
            result,
            Err(Error::RetryExhausted { attempts: 1, .. })
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_with_classification_succeeds_after_transient() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let result: Result<&'static str, Error> =
            retry_with_classification(|| {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n == 0 {
                        Err(Error::Timeout {
                            message: "t".into(),
                        })
                    } else {
                        Ok("ok")
                    }
                }
            })
            .await;
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

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
