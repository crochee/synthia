use synthia_provider::Message;

use super::*;
use crate::compaction::test_providers::{
    CapturingProvider,
    ConstantProvider,
    EmptyProvider,
    FailingProvider,
};

#[tokio::test]
async fn test_compact_level1_empty_messages() {
    let provider = ConstantProvider("anything".into());
    let result = compact_level1(&[], &provider, None, None).await.unwrap();
    assert_eq!(result.content, "");
    assert_eq!(result.compacted_tokens, 0);
}

#[tokio::test]
async fn test_compact_level1_success() {
    let provider = ConstantProvider("Summary text".into());
    let messages = vec![Message::user("hi"), Message::assistant("hello")];
    let result = compact_level1(&messages, &provider, None, None)
        .await
        .unwrap();
    assert_eq!(result.content, "Summary text");
}

#[tokio::test]
async fn test_compact_level1_provider_empty_fallback() {
    let provider = EmptyProvider;
    let messages = vec![Message::user("hi"), Message::assistant("hello")];
    let result = compact_level1(&messages, &provider, None, None)
        .await
        .unwrap();
    // Falls back to the structured-summary path.
    assert!(result.content.contains("Summary of 2 messages"));
}

#[tokio::test]
async fn test_compact_level1_provider_failure_fallback() {
    let provider = FailingProvider;
    let messages = vec![Message::user("hi"), Message::assistant("hello")];
    let result = compact_level1(&messages, &provider, None, None)
        .await
        .unwrap();
    assert!(result.content.contains("Summary of 2 messages"));
}

#[tokio::test]
async fn test_compact_level1_threads_previous_summary_to_provider() {
    use parking_lot::Mutex;

    let provider = CapturingProvider {
        last_previous: Mutex::new(None),
        summary: "ok".into(),
    };
    let messages = vec![Message::user("hi")];
    let anchor = "previous anchor text";
    let _ = compact_level1(&messages, &provider, Some(anchor), None)
        .await
        .unwrap();
    assert_eq!(
        provider.last_previous.lock().as_deref(),
        Some(anchor),
        "compact_level1 must forward previous_summary to the provider"
    );
}

#[tokio::test]
async fn test_compact_level1_passes_none_when_no_anchor() {
    use parking_lot::Mutex;

    let provider = CapturingProvider {
        last_previous: Mutex::new(None),
        summary: "ok".into(),
    };
    let messages = vec![Message::user("hi")];
    let _ = compact_level1(&messages, &provider, None, None)
        .await
        .unwrap();
    assert_eq!(
        provider.last_previous.lock().as_deref(),
        None,
        "compact_level1 must pass None when no previous_summary is supplied"
    );
}

#[tokio::test]
async fn test_compact_level1_uses_precomputed_tokens_when_supplied() {
    let provider = ConstantProvider("ok".into());
    let messages = vec![Message::user("hi")];
    let result = compact_level1(&messages, &provider, None, Some(42_000))
        .await
        .unwrap();
    assert_eq!(
        result.original_tokens, 42_000,
        "compact_level1 must use the precomputed value, not its own estimate"
    );
}

#[tokio::test]
async fn test_compact_level1_falls_back_to_estimate_when_none() {
    let provider = ConstantProvider("ok".into());
    let messages = vec![Message::user("hi"), Message::assistant("hello")];
    let result = compact_level1(&messages, &provider, None, None)
        .await
        .unwrap();
    assert!(
        result.original_tokens > 0,
        "compact_level1 must compute its own estimate when None is supplied"
    );
}

#[tokio::test]
async fn test_compact_level1_empty_messages_honors_precomputed() {
    let provider = ConstantProvider("ok".into());
    let result = compact_level1(&[], &provider, None, Some(7777))
        .await
        .unwrap();
    assert_eq!(result.original_tokens, 7777);
}
