//! Shared fixtures for the `SessionController` test suite.
//!
//! Houses the factories the test modules compose against
//! (`VecFactory`, `BlockingFactory`, `RecordingFactory`,
//! `ToolCapturingProvider`), the `make_manager_and_controller`
//! helper that builds a controller wired to a fresh in-memory
//! session registry, and the small "wait until N runs landed"
//! / "wait until the log holds this string" probes that keep
//! the async tests deterministic.

use std::{
    pin::Pin,
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use futures::{Stream, StreamExt};
use synthia::{
    harness::{
        AgentEvent,
        AgentInput,
        AgentRunConfig,
        SessionEndReason,
        SystemEvent,
    },
    provider::{
        Content,
        ContentPart,
        Message,
        TextContent,
        traits::ModelProvider,
    },
    session::manager::{InputQueue as SessionInputQueue, SessionRegistry},
    tool::registry::ToolRegistry,
};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::session::controller::{RunStreamFactory, SessionController};

/// Build a [`RunDependencies`] with no capabilities wired — every
/// test that needs a fresh baseline calls this. Returns the
/// struct directly (not the trait) so callers can poke
/// configuration knobs (`with_optional_tool_surface`, …) on the
/// returned value.
pub(super) fn test_deps() -> crate::session::controller::RunDependencies {
    crate::session::controller::RunDependencies::new(
        Arc::new(synthia::test_support::FakeProvider::new(vec![])),
        Arc::new(RwLock::new(ToolRegistry::new())),
        std::path::PathBuf::from("/tmp"),
        synthia::harness::DEFAULT_SYSTEM_PROMPT.to_string(),
    )
}

/// Spawn a fresh controller wired to a temp-dir session
/// registry. Returns the controller, the registry (handy for
/// reading the sink / queue directly), and the temp dir (handy
/// for asserting files on disk post-`close()`).
pub(super) async fn make_manager_and_controller(
    idle_timeout: Duration,
    run_factory: Arc<dyn RunStreamFactory>,
) -> (Arc<SessionController>, SessionRegistry, tempfile::TempDir) {
    make_manager_and_controller_with_deps(
        idle_timeout,
        run_factory,
        test_deps(),
    )
    .await
}

/// [`make_manager_and_controller`] with caller-supplied
/// [`RunDependencies`](crate::session::controller::RunDependencies):
/// the same spawn path the server uses, so a test can hand the
/// controller shared state it also holds a handle on (the
/// process-wide usage counters, a pinned clock, a tool surface).
pub(super) async fn make_manager_and_controller_with_deps(
    idle_timeout: Duration,
    run_factory: Arc<dyn RunStreamFactory>,
    deps: crate::session::controller::RunDependencies,
) -> (Arc<SessionController>, SessionRegistry, tempfile::TempDir) {
    let temp = tempfile::TempDir::new().unwrap();
    let manager = SessionRegistry::new(temp.path().to_path_buf());
    manager
        .create_with_user("s1".to_string(), "alice".to_string())
        .await
        .unwrap();

    let session_sink = manager.sink("alice", "s1");
    let controller = SessionController::spawn(
        "alice",
        "s1",
        manager.input_queue(),
        session_sink,
        deps,
        idle_timeout,
        run_factory,
    );

    (controller, manager, temp)
}

/// Wait until the run factory has been invoked at least `n`
/// times.
///
/// `SessionController` sets `SessionState::Running` inside
/// `maybe_start_run` BEFORE the spawned run task is ever
/// polled, and the `Cancel` op handler sets `Cancelled`
/// synchronously — so waiting on `state()` alone proves
/// neither that a run started nor that it finished. The
/// factory invocation count is the only deterministic
/// observable; same idiom as
/// `test_events_are_persisted_and_broadcast`.
pub(super) async fn wait_for_runs(
    calls: &Arc<Mutex<Vec<AgentRunConfig>>>,
    n: usize,
) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while calls.lock().unwrap().len() < n {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("run factory was not invoked in time");
}

/// Wait until the session log holds a row containing
/// `needle`, then return the full log. The rerun tests need
/// the run task's appends (prompt row, answer row) to have
/// landed before folding.
pub(super) async fn wait_for_log_row(
    manager: &SessionRegistry,
    needle: &str,
) -> Vec<serde_json::Value> {
    let sink = manager.sink("alice", "s1");
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let rows = sink.read().await.unwrap_or_default();
            if rows.iter().any(|row| row.to_string().contains(needle)) {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("row never landed in the session log")
}

/// The flat text of a message, for asserting fold output.
pub(super) fn message_text(message: &Message) -> String {
    match &message.content {
        Content::Single(ContentPart::Text(t)) => t.text.clone(),
        _ => String::new(),
    }
}

/// Find the `events.jsonl` written under `root` (the layout
/// `SessionRegistry` uses nests one directory per session).
pub(super) fn find_events_jsonl(
    root: &std::path::Path,
) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_events_jsonl(&path) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str())
            == Some("events.jsonl")
        {
            return Some(path);
        }
    }
    None
}

/// A factory that emits a fixed list of events and drains the
/// session input queue, useful for persistence and broadcast tests.
pub(super) struct VecFactory {
    pub(super) events: Vec<AgentEvent>,
    pub(super) calls: Arc<Mutex<Vec<AgentRunConfig>>>,
    pub(super) queue: Option<SessionInputQueue>,
}

impl VecFactory {
    pub(super) fn new(
        events: Vec<AgentEvent>,
        calls: Arc<Mutex<Vec<AgentRunConfig>>>,
        queue: Option<SessionInputQueue>,
    ) -> Self {
        Self {
            events,
            calls,
            queue,
        }
    }
}

impl RunStreamFactory for VecFactory {
    fn run_stream(
        &self,
        config: AgentRunConfig,
        _input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        self.calls.lock().unwrap().push(config.clone());

        let events = self.events.clone();
        let _queue = self.queue.clone();

        let stream = futures::stream::iter(events).take_while(move |_| {
            futures::future::ready(!cancel.is_cancelled())
        });
        Box::pin(stream)
    }
}

/// A factory that blocks until its cancellation token fires,
/// useful for verifying that only one run is active at a time.
pub(super) struct BlockingFactory {
    pub(super) calls: Arc<Mutex<Vec<AgentRunConfig>>>,
    pub(super) progress_count: Arc<AtomicUsize>,
}

impl RunStreamFactory for BlockingFactory {
    fn run_stream(
        &self,
        config: AgentRunConfig,
        _input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        self.calls.lock().unwrap().push(config.clone());

        let count = Arc::clone(&self.progress_count);

        let stream = async_stream::stream! {
            while !cancel.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(5)).await;
                count.fetch_add(1, Ordering::SeqCst);
                yield AgentEvent::Model(ContentPart::Text(TextContent {
                    text: format!("working {}", count.load(Ordering::SeqCst)),
                    cache_control: None,
                }));
            }
            yield AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Cancelled,
            });
        };
        Box::pin(stream)
    }
}

/// Records every `(input)` the controller hands to the agent,
/// plus the events the factory streams back. Used by
/// `test_second_prompt_seeds_history_from_persisted_turns` to
/// pin the multi-turn memory contract.
pub(super) struct RecordingFactory {
    pub(super) calls: Arc<Mutex<Vec<AgentInput>>>,
    pub(super) first_run_events: Vec<AgentEvent>,
    pub(super) second_run_events: Vec<AgentEvent>,
}

impl RecordingFactory {
    pub(super) fn new(
        calls: Arc<Mutex<Vec<AgentInput>>>,
        first_run_events: Vec<AgentEvent>,
        second_run_events: Vec<AgentEvent>,
    ) -> Self {
        Self {
            calls,
            first_run_events,
            second_run_events,
        }
    }
}

impl RunStreamFactory for RecordingFactory {
    fn run_stream(
        &self,
        _config: AgentRunConfig,
        input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        self.calls.lock().unwrap().push(input);
        let run_index = self.calls.lock().unwrap().len() - 1;
        let events = if run_index == 0 {
            self.first_run_events.clone()
        } else {
            self.second_run_events.clone()
        };
        let stream = futures::stream::iter(events).take_while(move |_| {
            futures::future::ready(!cancel.is_cancelled())
        });
        Box::pin(stream)
    }
}

/// Provider that records the tool list of every completion request
/// and then finishes the turn with text.
pub(super) struct ToolCapturingProvider {
    pub(super) captured:
        parking_lot::Mutex<Vec<Arc<Vec<synthia::provider::ToolDefinition>>>>,
}

#[async_trait::async_trait]
impl ModelProvider for ToolCapturingProvider {
    async fn initialize(
        &mut self,
        _config: synthia::provider::ProviderConfig,
    ) -> Result<(), synthia::core::Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "tool-capturing"
    }

    fn model_config(&self) -> synthia::provider::types::ModelConfig {
        synthia::provider::types::ModelConfig {
            name: "fake".to_string(),
            provider: "tool-capturing".to_string(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: synthia::provider::CompletionRequest,
    ) -> Result<synthia::provider::CompletionResponse, synthia::core::Error>
    {
        unreachable!("the factory's agent streams")
    }

    async fn complete_with_stream(
        &self,
        request: synthia::provider::CompletionRequest,
        _cancel_token: Option<Arc<dyn synthia::core::CancelToken>>,
        mut on_delta: Box<dyn FnMut(synthia::provider::StreamChunk) + Send>,
    ) -> Result<synthia::provider::CompletionResponse, synthia::core::Error>
    {
        self.captured.lock().push(request.tools.clone());
        on_delta(synthia::provider::StreamChunk::IsDone {
            result: Box::new(synthia::provider::SamplingResult {
                text: "done".to_string(),
                usage: synthia::provider::TokenUsage::default(),
                ..Default::default()
            }),
        });
        Ok(synthia::provider::CompletionResponse {
            id: "captured-1".to_string(),
            model: "fake".to_string(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: "done".to_string(),
                cache_control: None,
            })),
            usage: synthia::provider::TokenUsage::default(),
            cached: false,
            replay_state: None,
            stop_reason: Some("end_turn".to_string()),
        })
    }
}
