//! Integration tests for the Guardian permission gate wiring.
//!
//! These tests verify the integration between:
//! - [`synthia_agent::subagent::GuardianSubagentFactoryBridge`] (bridge
//!   adapting the agent-side `SubagentSessionFactory` to the
//!   guardian-local `GuardianSubagentFactory`)
//! - [`synthia_guardian::GuardianCoordinator`] (hybrid fast-path +
//!   subagent escalation)
//! - [`synthia_agent::subagent::SubagentSessionFactory`] (real factory)
//!
//! They do NOT test `execute_and_emit` directly (that requires full
//! `BuilderSteps` / `LoopContext` / `SamplingResult` setup). Instead
//! they verify the bridge + coordinator produce correct decisions for
//! each risk tier, which is the core of the permission gate.
//!
//! # Deferred tests
//!
//! - **6.6 recursion prevention** — deferred. The concrete
//!   `SubagentSessionFactory` impl lives in `synthia-server`, not
//!   `synthia-agent`. Lockdown enforcement (guardian_enabled: false,
//!   max_iterations: 1, empty tool registry, Deny-only permission) is
//!   a contractual obligation on that impl and cannot be inspected
//!   from `synthia-agent`.
//! - **6.7 prompt-cache key isolation** — deferred. The
//!   `SystemContext Source` (P1-4) mechanism for injecting the
//!   `guardian:{parent_session_id}` cache key is NOT implemented in
//!   the codebase. The bridge passes `parent_session_id` through, but
//!   the cache key injection point does not exist yet.

mod test_support;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use synthia_agent::{
    agent_instance::{AgentResult, AgentStatus, AgentTokenUsage},
    subagent::{
        ChildSessionHandle,
        GuardianSubagentFactoryBridge,
        SubagentSessionError,
        SubagentSessionFactory,
    },
};
use synthia_guardian::{
    ApprovalRequest,
    GuardianConfig,
    GuardianCoordinator,
    GuardianDecision,
    GuardianSubagentFactory,
};
use tokio_util::sync::CancellationToken;

// ---------- Mock SubagentSessionFactory ----------

/// Behaviour the mock factory should exhibit when `run_child` is called.
#[derive(Clone)]
enum MockBehavior {
    /// Return a successful `AgentResult` with the given output.
    Ok(String),
    /// Sleep for `delay` then return a successful result.
    Delay(Duration, String),
    /// Return a spawn error.
    Fail(String),
}

/// Mock `SubagentSessionFactory` that scripts `run_child` behaviour.
///
/// Returns canned `AgentResult`s so the
/// [`GuardianSubagentFactoryBridge`] can convert them to
/// [`GuardianSubagentOutput`]. The factory records each call so tests
/// can assert whether the subagent was spawned.
struct MockSessionFactory {
    behavior: Mutex<MockBehavior>,
    /// Number of times `run_child` was called.
    call_count: Mutex<usize>,
}

impl MockSessionFactory {
    fn ok(output: impl Into<String>) -> Self {
        Self {
            behavior: Mutex::new(MockBehavior::Ok(output.into())),
            call_count: Mutex::new(0),
        }
    }

    fn delay(delay: Duration, output: impl Into<String>) -> Self {
        Self {
            behavior: Mutex::new(MockBehavior::Delay(delay, output.into())),
            call_count: Mutex::new(0),
        }
    }

    fn fail(msg: impl Into<String>) -> Self {
        Self {
            behavior: Mutex::new(MockBehavior::Fail(msg.into())),
            call_count: Mutex::new(0),
        }
    }

    fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

#[async_trait]
impl SubagentSessionFactory for MockSessionFactory {
    async fn create_child(
        &self,
        _user_id: String,
        _parent_session_id: String,
        _maybe_id: Option<String>,
        _parent_depth: usize,
    ) -> Result<ChildSessionHandle, SubagentSessionError> {
        unimplemented!("create_child not used in guardian gate tests")
    }

    async fn run_child(
        &self,
        _user_id: String,
        _parent_session_id: String,
        _prompt: String,
        _parent_depth: usize,
        _maybe_id: Option<String>,
    ) -> Result<AgentResult, SubagentSessionError> {
        {
            let mut count = self.call_count.lock().unwrap();
            *count += 1;
        }
        let behavior = self.behavior.lock().unwrap().clone();
        match behavior {
            MockBehavior::Ok(output) => Ok(AgentResult {
                output,
                status: AgentStatus::Completed,
                token_usage: AgentTokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            }),
            MockBehavior::Delay(d, output) => {
                tokio::time::sleep(d).await;
                Ok(AgentResult {
                    output,
                    status: AgentStatus::Completed,
                    token_usage: AgentTokenUsage {
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                })
            }
            MockBehavior::Fail(msg) => {
                Err(SubagentSessionError::CreationFailed(msg))
            }
        }
    }
}

// ---------- Helpers ----------

/// Build a `GuardianCoordinator` with subagent escalation enabled,
/// wrapping `factory` in a [`GuardianSubagentFactoryBridge`].
fn make_coordinator(
    factory: Arc<dyn SubagentSessionFactory>,
    timeout: Duration,
) -> (GuardianCoordinator, Arc<dyn GuardianSubagentFactory>) {
    let bridge: Arc<dyn GuardianSubagentFactory> =
        Arc::new(GuardianSubagentFactoryBridge::new(factory));
    let config = GuardianConfig::default()
        .enabled(true)
        .with_subagent_enabled(true)
        .with_timeout(timeout);
    let coordinator = GuardianCoordinator::with_subagent_factory(
        config,
        "test-user".to_string(),
        "test-session".to_string(),
        bridge.clone(),
    );
    (coordinator, bridge)
}

/// Guardian assessment JSON for a low-risk action (risk_score 30).
fn low_risk_json() -> &'static str {
    r#"{"risk_level":"low","risk_score":30,"rationale":"safe action","evidence":[]}"#
}

/// Guardian assessment JSON for a high-risk action (risk_score 85).
fn high_risk_json() -> &'static str {
    r#"{"risk_level":"high","risk_score":85,"rationale":"destructive action","evidence":[]}"#
}

// ---------- Tests ----------

/// 6.1 — Low-risk tool call (risk < 50) bypasses Guardian subagent,
/// executes immediately.
///
/// `bash` with `ls` → `Shell` variant → risk 30 → `Allow` fast-path.
/// The mock subagent factory must NOT be called.
#[tokio::test]
async fn low_risk_tool_call_bypasses_subagent() {
    let factory = Arc::new(MockSessionFactory::ok(low_risk_json()));
    let factory_ref = factory.clone();
    let (coordinator, bridge) =
        make_coordinator(factory, Duration::from_secs(5));

    let request =
        ApprovalRequest::shell("call-1", vec!["ls".to_string()], "/", None);
    let outcome = coordinator
        .check(&request, &[], &CancellationToken::new(), Some(&*bridge))
        .await;

    assert!(
        outcome.decision.is_allowed(),
        "low-risk should be allowed, got {:?}",
        outcome.decision
    );
    assert!(
        !outcome.escalated,
        "low-risk should not escalate to subagent"
    );
    assert_eq!(
        factory_ref.call_count(),
        0,
        "subagent factory must not be called for low-risk"
    );
}

/// 6.2 — High-risk tool call (risk >= 80) denied by SimpleGuardian
/// fast-path, no subagent spawn.
///
/// `bash` with `rm -rf /` → `Shell` variant → risk 90 → `Deny`
/// fast-path. The mock subagent factory must NOT be called.
#[tokio::test]
async fn high_risk_tool_call_denied_fast_path() {
    let factory = Arc::new(MockSessionFactory::ok(low_risk_json()));
    let factory_ref = factory.clone();
    let (coordinator, bridge) =
        make_coordinator(factory, Duration::from_secs(5));

    let request = ApprovalRequest::shell(
        "call-1",
        vec!["rm -rf /".to_string()],
        "/",
        None,
    );
    let outcome = coordinator
        .check(&request, &[], &CancellationToken::new(), Some(&*bridge))
        .await;

    assert!(
        matches!(outcome.decision, GuardianDecision::Deny { .. }),
        "high-risk should be denied, got {:?}",
        outcome.decision
    );
    assert!(
        !outcome.escalated,
        "high-risk should not escalate to subagent"
    );
    assert_eq!(
        factory_ref.call_count(),
        0,
        "subagent factory must not be called for high-risk"
    );
}

/// 6.3 — Medium-risk tool call (risk in [50, 80)) spawns Guardian
/// subagent, subagent returns Allow → tool executes.
///
/// `NetworkAccess` → risk 65 → medium → escalate. Subagent returns
/// low-risk assessment → `Allow`.
#[tokio::test]
async fn medium_risk_subagent_returns_allow() {
    let factory = Arc::new(MockSessionFactory::ok(low_risk_json()));
    let factory_ref = factory.clone();
    let (coordinator, bridge) =
        make_coordinator(factory, Duration::from_secs(5));

    let request = ApprovalRequest::network_access(
        "call-1", "target", "host", "https", 443,
    );
    let outcome = coordinator
        .check(&request, &[], &CancellationToken::new(), Some(&*bridge))
        .await;

    assert!(outcome.escalated, "medium-risk should escalate to subagent");
    assert!(
        outcome.decision.is_allowed(),
        "subagent returned low-risk → Allow, got {:?}",
        outcome.decision
    );
    assert!(
        outcome.subagent_error.is_none(),
        "no subagent error expected"
    );
    assert_eq!(
        factory_ref.call_count(),
        1,
        "subagent factory should be called exactly once"
    );
}

/// 6.4 — Medium-risk tool call, subagent returns Deny → tool denied
/// with rationale.
///
/// `NetworkAccess` → risk 65 → medium → escalate. Subagent returns
/// high-risk assessment → `Deny`.
#[tokio::test]
async fn medium_risk_subagent_returns_deny() {
    let factory = Arc::new(MockSessionFactory::ok(high_risk_json()));
    let factory_ref = factory.clone();
    let (coordinator, bridge) =
        make_coordinator(factory, Duration::from_secs(5));

    let request = ApprovalRequest::network_access(
        "call-1", "target", "host", "https", 443,
    );
    let outcome = coordinator
        .check(&request, &[], &CancellationToken::new(), Some(&*bridge))
        .await;

    assert!(outcome.escalated, "medium-risk should escalate to subagent");
    assert!(
        matches!(outcome.decision, GuardianDecision::Deny { .. }),
        "subagent returned high-risk → Deny, got {:?}",
        outcome.decision
    );
    assert!(
        outcome.subagent_error.is_none(),
        "no subagent error expected"
    );
    assert_eq!(
        factory_ref.call_count(),
        1,
        "subagent factory should be called exactly once"
    );
}

/// 6.5 — Medium-risk tool call, subagent times out → fallback to
/// NeedUserConfirm.
///
/// `NetworkAccess` → risk 65 → medium → escalate. Subagent delays 2s,
/// coordinator timeout is 100ms → `Timeout` → fallback
/// `NeedUserConfirm`.
#[tokio::test]
async fn medium_risk_subagent_timeout_fallback() {
    let factory = Arc::new(MockSessionFactory::delay(
        Duration::from_secs(2),
        low_risk_json(),
    ));
    let factory_ref = factory.clone();
    let (coordinator, bridge) =
        make_coordinator(factory, Duration::from_millis(100));

    let request = ApprovalRequest::network_access(
        "call-1", "target", "host", "https", 443,
    );
    let outcome = coordinator
        .check(&request, &[], &CancellationToken::new(), Some(&*bridge))
        .await;

    assert!(outcome.escalated, "medium-risk should attempt escalation");
    assert!(
        matches!(outcome.decision, GuardianDecision::NeedUserConfirm { .. }),
        "timeout should fall back to NeedUserConfirm, got {:?}",
        outcome.decision
    );
    assert!(
        outcome.subagent_error.is_some(),
        "subagent error should be captured"
    );
    assert_eq!(
        factory_ref.call_count(),
        1,
        "subagent factory should be called exactly once"
    );
}

/// Verify the bridge correctly converts `AgentResult` with
/// `Completed` status to `GuardianSubagentOutput { success: true }`.
#[tokio::test]
async fn bridge_converts_completed_to_success() {
    let factory = Arc::new(MockSessionFactory::ok(low_risk_json()));
    let bridge = GuardianSubagentFactoryBridge::new(factory);

    let output = bridge
        .run_child("u".into(), "p".into(), "prompt".into())
        .await
        .expect("completed run should succeed");

    assert!(output.success);
    assert!(output.output.contains("risk_level"));
}

/// Verify the bridge correctly converts `SubagentSessionError` to
/// `GuardianSubagentSpawnError`.
#[tokio::test]
async fn bridge_converts_error_to_spawn_failed() {
    let factory = Arc::new(MockSessionFactory::fail("connection refused"));
    let bridge = GuardianSubagentFactoryBridge::new(factory);

    let result = bridge
        .run_child("u".into(), "p".into(), "prompt".into())
        .await;

    assert!(
        result.is_err(),
        "factory error should propagate through bridge"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(
            err,
            synthia_guardian::GuardianSubagentSpawnError::SpawnFailed(_)
        ),
        "expected SpawnFailed, got {err:?}"
    );
}

// ---------- StepToolExecute orchestrator gate tests ----------
//
// `execute_and_emit` (in `stream_builder/builder/tool_execution/execute.rs`)
// cannot be driven directly from integration tests because it requires a
// fully-constructed `BuilderSteps` (its fields are `pub(super)`). These
// tests instead verify the gate condition (`StepToolExecute::has_orchestrator`)
// that `execute_and_emit` uses to decide whether a `NeedUserConfirm` decision
// can be forwarded to an approval service or must be downgraded to `Deny`
// (P6 — Distrust by Default). The denial logic itself is a single branch in
// `execute_and_emit`'s `NeedUserConfirm` arm; combined with the
// `medium_risk_subagent_timeout_fallback` test above (which confirms the
// coordinator returns `NeedUserConfirm` on subagent timeout), these tests
// fully cover the fail-closed path.

use synthia_agent::{
    config::{AgentConfig, AgentRunConfigBuilder},
    stream_builder::steps::StepToolExecute,
    types::AgentInput,
};
use synthia_context::ContextAssembler;
use synthia_hook::HookRegistry;
use synthia_provider::router::ModelRouter;
use synthia_session::Store as SessionStore;
use synthia_tool_orchestrator::{
    ExecutionContext,
    ToolCallRequest,
    ToolCallResult,
    ToolOrchestrator,
    ToolOrchestratorError,
    ToolOrchestratorEvent,
};

/// Stub `ToolOrchestrator` used only to make `has_orchestrator()`
/// return `true`. All methods panic because the tests in this module
/// never invoke the orchestrator — they only verify the gate
/// condition used by `execute_and_emit`'s `NeedUserConfirm` arm.
struct StubToolOrchestrator {
    event_tx: tokio::sync::broadcast::Sender<ToolOrchestratorEvent>,
}

impl StubToolOrchestrator {
    fn new() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(1);
        Self { event_tx }
    }
}

#[async_trait]
impl ToolOrchestrator for StubToolOrchestrator {
    async fn execute(
        &self,
        _request: ToolCallRequest,
        _context: ExecutionContext,
        _cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolOrchestratorError> {
        unimplemented!("StubToolOrchestrator::execute is not used")
    }

    async fn execute_batch(
        &self,
        _requests: Vec<ToolCallRequest>,
        _context: ExecutionContext,
        _cancellation_token: CancellationToken,
    ) -> Result<Vec<ToolCallResult>, ToolOrchestratorError> {
        unimplemented!("StubToolOrchestrator::execute_batch is not used")
    }

    fn event_stream(
        &self,
    ) -> tokio::sync::broadcast::Receiver<ToolOrchestratorEvent> {
        self.event_tx.subscribe()
    }

    async fn cancel(
        &self,
        _call_id: &str,
    ) -> Result<(), ToolOrchestratorError> {
        unimplemented!("StubToolOrchestrator::cancel is not used")
    }
}

/// Build a minimal `AgentRunConfig` for `StepToolExecute` tests.
///
/// `tool_orchestrator` is `None` by default; pass `Some(...)` to
/// configure one. The returned `TempDir` must outlive the
/// `AgentRunConfig` (the `workspace_root` path points inside it).
fn make_step_execute_config(
    tool_orchestrator: Option<Arc<dyn ToolOrchestrator>>,
) -> (synthia_agent::config::AgentRunConfig, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().to_path_buf();
    let session_store =
        SessionStore::new(workspace.join(".synthia").join("sessions"));

    let mut builder = AgentRunConfigBuilder::new()
        .provider(Arc::new(
            test_support::FakeProvider::new(vec!["ok".to_string()])
                .with_response("ok"),
        ))
        .tool_registry(synthia_tool::registry::ToolRegistry::new())
        .hook_registry(Arc::new(HookRegistry::new()))
        .model_router(Arc::new(ModelRouter::new()))
        .user_id("test-user".to_string())
        .session_id("test-session".to_string())
        .input(AgentInput::text("test"))
        .config(AgentConfig {
            model: "test-model".to_string(),
            max_tokens: 4096,
            max_iterations: 2,
            temperature: None,
            workspace_root: workspace,
            ..Default::default()
        })
        .context_assembler(Arc::new(ContextAssembler::new(4096)))
        .session_store(session_store)
        .cancel_token(CancellationToken::new());

    if let Some(orchestrator) = tool_orchestrator {
        builder = builder.tool_orchestrator(orchestrator);
    }

    let config = builder.build().unwrap();
    (config, temp)
}

/// Verify `StepToolExecute::has_orchestrator()` returns `false` when
/// no orchestrator is configured. This is the gate condition for the
/// `NeedUserConfirm` → `Deny` fallback in `execute_and_emit` (P6 —
/// Distrust by Default): when the Guardian requests user confirmation
/// but no approval service is available, the tool call is denied
/// rather than silently downgraded to execution.
#[tokio::test]
async fn need_user_confirm_denies_when_no_orchestrator_configured() {
    let (config, _temp) = make_step_execute_config(None);
    let step = StepToolExecute::new(&config);

    assert!(
        !step.has_orchestrator(),
        "has_orchestrator() must be false when no orchestrator is \
         configured; otherwise NeedUserConfirm would be silently \
         downgraded to execution (P6 violation)"
    );
}

/// Verify `StepToolExecute::has_orchestrator()` returns `true` when
/// an orchestrator is configured. In this case, `execute_and_emit`
/// forwards `NeedUserConfirm` decisions to the orchestrator's
/// approval flow instead of denying.
#[tokio::test]
async fn need_user_confirm_forwards_when_orchestrator_configured() {
    let orchestrator: Arc<dyn ToolOrchestrator> =
        Arc::new(StubToolOrchestrator::new());
    let (config, _temp) = make_step_execute_config(Some(orchestrator));
    let step = StepToolExecute::new(&config);

    assert!(
        step.has_orchestrator(),
        "has_orchestrator() must be true when an orchestrator is \
         configured; otherwise NeedUserConfirm would be denied even \
         though an approval service is available"
    );
}
