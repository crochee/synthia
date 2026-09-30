//! [`CompactionHookFn`] — typed policy hook a deployment installs to
//! shape the prompt the [`SummarizingContextManager`](super::SummarizingContextManager)
//! hands to its `SummariseFn` (R62, OpenCode
//! `experimental.session.compacting` parity).
//!
//! The hook is called once per compaction, with the serialised
//! tool-batch the manager would otherwise pass straight to the
//! summariser. The hook may:
//!
//! - return `None` to keep the default prompt (a no-op),
//! - return `Some(prompt)` to replace the prompt the manager
//!   would otherwise build.
//!
//! The hook is panic-isolated at the call site so a buggy hook
//! cannot poison the compaction; the same fail-soft contract as
//! `synthia_steering::AgentHook` applies.

use std::sync::Arc;

/// Async callback that decides the prompt handed to the
/// compaction summariser.
///
/// Receives the serialised tool-batch (`batch`) and must return
/// either:
/// - `None` to keep the default prompt (the manager appends the
///   default `prefix + batch` shape unchanged);
/// - `Some(prompt)` to use `prompt` verbatim as the LLM input.
pub type CompactionHookFn = Arc<
    dyn Fn(&str) -> futures::future::BoxFuture<'static, Option<String>>
        + Send
        + Sync,
>;

/// Adapter: lift a [`CompactionHookFn`] into a closure compatible
/// with [`super::SummariseFn`] — the two share the same
/// signature, so a deployment that prefers the hook seam can use
/// the same closure type for both.
pub fn as_summarise_fn(hook: CompactionHookFn) -> super::SummariseFn {
    Arc::new(move |batch: &str| {
        let batch = batch.to_string();
        let hook = Arc::clone(&hook);
        Box::pin(async move { hook(&batch).await })
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[tokio::test]
    async fn identity_hook_returns_none() {
        let h: CompactionHookFn = Arc::new(|_| Box::pin(async { None }));
        assert!(h("batch").await.is_none());
    }

    #[tokio::test]
    async fn as_summarise_fn_forwards_result() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in = Arc::clone(&calls);
        let h: CompactionHookFn = Arc::new(move |batch| {
            let calls = Arc::clone(&calls_in);
            let batch = batch.to_string();
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Some(format!("custom: {batch}"))
            })
        });
        let f = as_summarise_fn(h);
        let out = f("hello").await.unwrap();
        assert_eq!(out, "custom: hello");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
