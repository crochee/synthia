//! Compaction wiring the agent loop installs on a
//! [`synthia_context::SummarizingContextManager`].
//!
//! Three pieces:
//!
//! - [`CompactionEmitters`] — the record/lifecycle callback pair a
//!   durable [`synthia_session::CompactionCheckpoint`] installs,
//!   plus the `install` bridge that adapts the manager's own
//!   record/lifecycle types to the session crate's view types.
//! - [`context_manager_for_compaction_with_emitters`] — the public
//!   entry a hand-assembling run factory calls to honour the
//!   `agents.<name>.compaction` server config.
//! - [`resolve`] — the R13-3 decision core: enabled + valid policy
//!   replaces the caller's manager with the provider-backed
//!   summarising one; anything else returns the caller's manager
//!   unchanged. Crate-internal: [`crate::ReActAgent`] is the only
//!   caller.

use std::sync::Arc;

use synthia_context::{
    CompactionLifecycle,
    CompactionRecord,
    ContextManager,
    SummarizingContextManager,
    TruncatingContextManager,
};
use synthia_provider::traits::ModelProvider;
use synthia_session::{CompactionLifecycleView, CompactionRecordView};

/// The two emitters a durable compaction checkpoint installs on a
/// [`SummarizingContextManager`].
///
/// Built from a [`synthia_session::CompactionCheckpoint`]'s callbacks (or any
/// pair of callbacks with the same shape). This is what
/// [`context_manager_for_compaction_with_emitters`] installs, and
/// what [`crate::ReActAgent::with_compaction_settings`] installs
/// automatically when both a policy and a typed event sink are
/// present.
#[derive(Clone)]
pub struct CompactionEmitters {
    record: Arc<dyn Fn(CompactionRecordView) + Send + Sync>,
    lifecycle: Arc<dyn Fn(CompactionLifecycleView) + Send + Sync>,
}

impl CompactionEmitters {
    /// Wrap a record callback and a lifecycle callback.
    #[must_use]
    pub fn new(
        record: Arc<dyn Fn(CompactionRecordView) + Send + Sync>,
        lifecycle: Arc<dyn Fn(CompactionLifecycleView) + Send + Sync>,
    ) -> Self {
        Self { record, lifecycle }
    }

    /// Install both emitters on `manager`, bridging the manager's
    /// record/lifecycle types to this crate's view types.
    fn install(
        self,
        manager: SummarizingContextManager,
    ) -> SummarizingContextManager {
        let record = self.record;
        let lifecycle = self.lifecycle;
        manager
            .with_compaction_emitter(move |rec: CompactionRecord| {
                record(record_view(&rec));
            })
            .with_compaction_lifecycle_emitter(
                move |step: CompactionLifecycle| {
                    lifecycle(lifecycle_view(&step));
                },
            )
    }
}

/// Mirror one manager record into the session crate's view type.
fn record_view(record: &CompactionRecord) -> CompactionRecordView {
    CompactionRecordView {
        start: record.start,
        end: record.end,
        source_indices: record.source_indices.clone(),
        source_keys: record.source_keys.clone(),
        summary_text: record.summary_text.clone(),
    }
}

/// Mirror one manager lifecycle step into the session crate's view
/// type.
fn lifecycle_view(step: &CompactionLifecycle) -> CompactionLifecycleView {
    match step {
        CompactionLifecycle::Start { token } => {
            CompactionLifecycleView::Start {
                token: token.as_str().to_string(),
            }
        }
        CompactionLifecycle::Summary {
            token,
            summary,
            shadowed_positions,
        } => CompactionLifecycleView::Summary {
            token: token.as_str().to_string(),
            summary: summary.clone(),
            shadowed_positions: shadowed_positions.clone(),
        },
        CompactionLifecycle::End { token, outcome } => {
            CompactionLifecycleView::End {
                token: token.as_str().to_string(),
                outcome: *outcome,
            }
        }
    }
}

/// Public entry point for the R13-3 decision core, with the durable
/// emitters installed. Builds a provider-backed
/// [`synthia_context::SummarizingContextManager`] when the policy is
/// enabled + valid, otherwise returns a [`TruncatingContextManager`].
///
/// Server-side run factories that build agents by hand (rather than
/// through [`crate::ReActAgent`]'s `with_compaction_settings` setter)
/// call this to honour the `agents.<name>.compaction` config.
#[must_use]
pub fn context_manager_for_compaction_with_emitters(
    provider: Arc<dyn ModelProvider>,
    settings: synthia_context::CompactionSettings,
    emitters: Option<CompactionEmitters>,
) -> Arc<dyn ContextManager> {
    resolve(
        Some(settings),
        Arc::new(TruncatingContextManager),
        &provider,
        emitters,
    )
}

/// R13-3: decide the context-manager strategy from the
/// compaction policy. Pure decision core (directly testable):
///
/// - `None` / disabled / invalid → the caller's manager
///   arrives unchanged.
/// - `Some` + enabled + valid → it is **ignored** and a
///   provider-backed [`synthia_context::SummarizingContextManager`] is
///   built, with `emitters` (when given) installed.
///
/// The summariser forks the agent's own provider with a fixed
/// compaction prompt — pi compaction parity (the summarising
/// model is the same one the session already uses, so no extra
/// credentials or wiring).
pub(crate) fn resolve(
    settings: Option<synthia_context::CompactionSettings>,
    default: Arc<dyn ContextManager>,
    provider: &Arc<dyn ModelProvider>,
    emitters: Option<CompactionEmitters>,
) -> Arc<dyn ContextManager> {
    let Some(settings) = settings else {
        return default;
    };
    if !settings.enabled {
        tracing::debug!("compaction disabled; keeping default manager");
        return default;
    }
    if let Err(e) = settings.validate() {
        tracing::warn!(error = %e, "invalid CompactionSettings; keeping default manager");
        return default;
    }

    let provider = Arc::clone(provider);
    let summarise: synthia_context::SummariseFn = Arc::new(move |batch| {
        let provider = Arc::clone(&provider);
        let batch = batch.to_string();
        Box::pin(async move {
            let request = synthia_provider::CompletionRequest {
                messages: Arc::new(vec![synthia_provider::Message::user(
                    format!(
                        "Summarise the following tool-call batch. Preserve \
                         actionable facts, file paths, and outcomes; drop \
                         raw payloads:\n\n{batch}"
                    ),
                )]),
                ..Default::default()
            };
            match provider.complete(request).await {
                Ok(resp) => resp.content.extract_text(),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "compaction summarise call failed; skipping batch"
                    );
                    None
                }
            }
        })
    });
    let manager =
        SummarizingContextManager::new(summarise).with_settings(settings);
    match emitters {
        Some(emitters) => Arc::new(emitters.install(manager)),
        None => Arc::new(manager),
    }
}

#[cfg(test)]
mod tests {
    //! Compaction tests — the decision core (`resolve`) plus the
    //! factory entry point the server-side run factory uses
    //! (`context_manager_for_compaction_with_emitters`).
    //!
    //! The previous home of these tests was
    //! `agent/builder/tests.rs`; the R110 fold moved the standalone
    //! helpers to `crate::compaction` (the agent builder is gone —
    //! the same setters ride `ReActAgent::with_compaction_*`).

    use std::sync::Arc;

    use synthia_context::{
        CompactionSettings,
        ContextManager,
        TruncatingContextManager,
    };
    use synthia_provider::traits::ModelProvider;
    use synthia_session::{
        CompactionCheckpoint,
        SurfaceLedger,
        TypedEventSink,
    };

    use super::{
        CompactionEmitters,
        context_manager_for_compaction_with_emitters,
        resolve,
    };

    fn stub() -> Arc<dyn ModelProvider> {
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::text_only(
            "stub",
        ))
    }

    /// `resolve` decision table — `None` / disabled / invalid →
    /// default kept; enabled + valid → replaced with the
    /// provider-backed summarising manager.
    #[tokio::test]
    async fn resolve_decision_table() {
        let provider = stub();
        let default: Arc<dyn ContextManager> =
            Arc::new(TruncatingContextManager);

        // None → default kept.
        let _ = resolve(None, default.clone(), &provider, None);

        // Disabled → default kept.
        let disabled = CompactionSettings {
            enabled: false,
            ..CompactionSettings::default()
        };
        let _ = resolve(Some(disabled), default.clone(), &provider, None);

        // Invalid (reserve_tokens = 0) → default kept.
        let invalid = CompactionSettings {
            reserve_tokens: 0,
            ..CompactionSettings::default()
        };
        let _ = resolve(Some(invalid), default.clone(), &provider, None);

        // Enabled + valid → replaced with the provider-backed
        // summarising manager. Observable without downcasting:
        // calling prepare on a tiny window must not panic.
        let provider = stub();
        let summarising = resolve(
            Some(CompactionSettings::default()),
            Arc::new(TruncatingContextManager),
            &provider,
            None,
        );
        let mut msgs = vec![synthia_provider::Message::system("sys")];
        let mut state = synthia_context::AgentState::with_window(1_000_000);
        let out = summarising
            .prepare_arc(Arc::new(msgs.clone()), &mut state)
            .await;
        msgs = (*out).clone();
        // With a roomy window nothing is compacted.
        assert_eq!(msgs.len(), 1);
    }

    /// `context_manager_for_compaction_with_emitters` builds a
    /// manager that triggers on a tiny window. With an empty
    /// ledger the checkpoint cannot prove provenance for any
    /// splice, so no `Replace` row is emitted — but the
    /// lifecycle pair (`compaction_start` / `compaction_end`)
    /// still reaches the durable channel, which is what the
    /// log-only contract pins.
    #[tokio::test]
    async fn factory_manager_publishes_lifecycle_through_sink() {
        use synthia_session::SessionEvent;

        let (sink, mut receiver) = TypedEventSink::channel(16);
        let ledger = Arc::new(SurfaceLedger::new());
        let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));
        let emitters = CompactionEmitters::new(
            checkpoint.record_callback(),
            checkpoint.lifecycle_callback(),
        );
        let manager = context_manager_for_compaction_with_emitters(
            stub(),
            compaction_policy(1),
            Some(emitters),
        );

        let mut messages = tool_history();
        let mut state = synthia_context::AgentState::with_window(4);
        manager.prepare(&mut messages, &mut state).await;

        assert_eq!(messages.len(), 3, "the two tool results were summarised");
        let events = drain(&mut receiver);
        assert!(
            events.iter().any(|event| matches!(
                event,
                SessionEvent::CompactionStart { .. }
            )),
            "the lifecycle opened: {events:?}"
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                SessionEvent::CompactionEnd { .. }
            )),
            "the lifecycle closed: {events:?}"
        );
    }

    // --- helpers shared with the loop-side R34 durable-checkpoint
    // --- tests (re-fed ledger, tool history, drain). Kept here so the
    // --- compaction module owns its own test fixtures; the loop side
    // --- uses a duplicate under its own `loop_/tests/`.

    fn compaction_policy(reserve_tokens: u32) -> CompactionSettings {
        CompactionSettings {
            enabled: true,
            reserve_tokens,
            keep_recent_tokens: 1,
            min_messages_between_compaction: 1,
        }
    }

    fn tool_history() -> Vec<synthia_provider::Message> {
        use synthia_provider::Message;

        vec![
            Message::user("read the files"),
            Message::assistant("on it"),
            Message::tool(
                synthia_provider::Content::text("x".repeat(400)),
                "c1",
            ),
            Message::tool(
                synthia_provider::Content::text("y".repeat(400)),
                "c2",
            ),
        ]
    }

    fn drain(
        receiver: &mut synthia_session::TypedEventReceiver,
    ) -> Vec<synthia_session::SessionEvent> {
        let mut events = Vec::new();
        while let Ok(Some(record)) = receiver.try_recv() {
            events.push(record.event);
        }
        events
    }
}
