//! Compaction wiring: which context manager a run actually uses.
//!
//! The compaction setters and [`ReActAgent::with_context_manager`]
//! describe one decision — an enabled + valid policy replaces the
//! caller's manager, anything else keeps it — and these tests pin that
//! the decision is made per run, so the order the setters are called in
//! cannot change the outcome. Before the resolution moved out of the
//! setters, `.with_context_manager(..).with_compaction_settings(..)`
//! silently discarded the caller's manager.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use futures::StreamExt;
use synthia_context::{AgentState, CompactionSettings, ContextManager};
use synthia_provider::{
    ContentPart,
    SamplingResult,
    StreamChunk,
    TextContent,
    traits::ModelProvider,
};
use synthia_tool::ToolRegistry;
use tokio_util::sync::CancellationToken;

use super::{support::*, *};

/// Counts how many times the loop asked it to fit the context window.
struct SpyManager {
    prepares: Arc<AtomicUsize>,
}

#[async_trait]
impl ContextManager for SpyManager {
    async fn prepare(
        &self,
        _messages: &mut Vec<Message>,
        _state: &mut AgentState,
    ) {
        self.prepares.fetch_add(1, Ordering::SeqCst);
    }
}

/// Script a one-iteration run that answers and stops.
fn answer_script() -> ScriptedStreamProvider {
    ScriptedStreamProvider::new(vec![vec![
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "done".to_string(),
            cache_control: None,
        })),
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: "done".to_string(),
                ..SamplingResult::default()
            }),
        },
    ]])
}

async fn run_to_completion(agent: &ReActAgent) -> Vec<SessionEndReason> {
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut reasons = Vec::new();
    while let Some(event) = stream.next().await {
        if let AgentEvent::System(SystemEvent::SessionEnded { reason }) = event
        {
            reasons.push(reason);
        }
    }
    reasons
}

/// A disabled policy leaves the caller's manager in place whichever
/// setter ran first.
#[tokio::test]
async fn disabled_policy_keeps_the_caller_manager_in_either_order() {
    let disabled = CompactionSettings {
        enabled: false,
        ..CompactionSettings::default()
    };

    for settings_first in [true, false] {
        let prepares = Arc::new(AtomicUsize::new(0));
        let spy: Arc<dyn ContextManager> = Arc::new(SpyManager {
            prepares: Arc::clone(&prepares),
        });
        let provider: Arc<dyn ModelProvider> = Arc::new(answer_script());

        let base = ReActAgent::new(provider, Arc::new(ToolRegistry::new()));
        let agent = if settings_first {
            base.with_compaction_settings(disabled)
                .with_context_manager(spy)
        } else {
            base.with_context_manager(spy)
                .with_compaction_settings(disabled)
        };

        let reasons = run_to_completion(&agent).await;
        assert_eq!(
            reasons,
            vec![SessionEndReason::Completed],
            "settings_first={settings_first}: the run must complete"
        );
        assert_eq!(
            prepares.load(Ordering::SeqCst),
            1,
            "settings_first={settings_first}: a disabled policy must not \
             discard the caller's own context manager"
        );
    }
}

/// An enabled + valid policy replaces the caller's manager whichever
/// setter ran first.
#[tokio::test]
async fn enabled_policy_replaces_the_caller_manager_in_either_order() {
    // The scripted provider's window is wide enough that no compaction
    // triggers, so the summariser never calls the provider.
    let enabled = CompactionSettings::default();

    for settings_first in [true, false] {
        let prepares = Arc::new(AtomicUsize::new(0));
        let spy: Arc<dyn ContextManager> = Arc::new(SpyManager {
            prepares: Arc::clone(&prepares),
        });
        let provider: Arc<dyn ModelProvider> = Arc::new(answer_script());

        let base = ReActAgent::new(provider, Arc::new(ToolRegistry::new()));
        let agent = if settings_first {
            base.with_compaction_settings(enabled)
                .with_context_manager(spy)
        } else {
            base.with_context_manager(spy)
                .with_compaction_settings(enabled)
        };

        let reasons = run_to_completion(&agent).await;
        assert_eq!(
            reasons,
            vec![SessionEndReason::Completed],
            "settings_first={settings_first}: the run must complete"
        );
        assert_eq!(
            prepares.load(Ordering::SeqCst),
            0,
            "settings_first={settings_first}: an enabled policy replaces \
             the caller's context manager"
        );
    }
}

/// An externally-owned checkpoint supplies the durable emitters, so a
/// policy + checkpoint pair still replaces the caller's manager — in
/// either setter order.
#[tokio::test]
async fn external_checkpoint_still_replaces_the_caller_manager() {
    let enabled = CompactionSettings::default();

    for settings_first in [true, false] {
        let prepares = Arc::new(AtomicUsize::new(0));
        let spy: Arc<dyn ContextManager> = Arc::new(SpyManager {
            prepares: Arc::clone(&prepares),
        });
        let (sink, _rx) = synthia_session::TypedEventSink::channel(16);
        let checkpoint = synthia_session::CompactionCheckpoint::new(
            sink,
            Arc::new(synthia_session::SurfaceLedger::new()),
        );
        let provider: Arc<dyn ModelProvider> = Arc::new(answer_script());

        let base = ReActAgent::new(provider, Arc::new(ToolRegistry::new()));
        let agent = if settings_first {
            base.with_compaction_settings(enabled)
                .with_compaction_checkpoint(checkpoint)
                .with_context_manager(spy)
        } else {
            base.with_context_manager(spy)
                .with_compaction_checkpoint(checkpoint)
                .with_compaction_settings(enabled)
        };

        let reasons = run_to_completion(&agent).await;
        assert_eq!(
            reasons,
            vec![SessionEndReason::Completed],
            "settings_first={settings_first}: the run must complete"
        );
        assert_eq!(
            prepares.load(Ordering::SeqCst),
            0,
            "settings_first={settings_first}: an enabled policy replaces \
             the caller's context manager even with an external checkpoint"
        );
    }
}
