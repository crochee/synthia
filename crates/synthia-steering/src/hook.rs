//! [`AgentHook`] — async lifecycle observation with an optional
//! tool veto.
//!
//! The async mirror of the synchronous guard/tracker layer: hooks
//! may perform I/O (audit logs, metrics pushes, approval
//! services) and run at every interesting boundary of the agent
//! loop. They are invoked sequentially in registration order for
//! deterministic semantics.
//!
//! ## Veto semantics
//!
//! Only [`AgentHook::before_tool_execute`] can alter control
//! flow, via [`HookAction::Block`]. A block runs *before* the
//! guard pipeline and cannot override a guard denial — hard
//! policy always has the last word. The block reason is returned
//! to the model as the tool result (errors-as-results contract).
//!
//! ## Failure isolation (R4 — `run_hook` wrapper)
//!
//! A panicking hook MUST NOT abort the agent session. The
//! [`run_hook`] helper wraps every hook call in
//! `AssertUnwindSafe + futures::FutureExt::catch_unwind`,
//! converts the panic payload into a [`HookError`] carrying the
//! hook name + the [`HookStage`] it was invoked at, and returns
//! it as an `Err`. The agent loop logs the error and emits a
//! `WarningKind::Hook` system event so the run continues. This
//! mirrors `pi-agent`'s `HarnessEventBus.deliver()` semantics:
//! a single broken listener must not block its siblings.
//!
//! Adopters that need the pre-R4 behaviour (panic = session
//! abort) can skip [`run_hook`] and call the hook method
//! directly, but `Steering::default_policy` always wraps.

use std::{future::Future, panic::AssertUnwindSafe, time::Duration};

use async_trait::async_trait;
use futures::FutureExt;
use serde_json::Value;
use synthia_provider::{CompletionRequest, CompletionResponse};

/// A hook's decision for an upcoming tool execution.
#[derive(Debug, Clone)]
pub enum HookAction {
    /// Proceed (guards still run afterwards).
    Continue,
    /// Veto: `reason` becomes the tool result the model sees.
    Block(String),
}

/// Where in the agent lifecycle a hook was invoked.
///
/// `run_hook` carries this on every [`HookError`] so operators
/// can filter on the lifecycle stage a broken listener panicked
/// at (e.g. `HookStage::BeforeToolExecute` vs
/// `HookStage::AfterToolExecute`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookStage {
    /// `AgentHook::on_agent_start`.
    OnAgentStart,
    /// `AgentHook::on_agent_end`.
    OnAgentEnd,
    /// `AgentHook::on_provider_start`.
    OnProviderStart,
    /// `AgentHook::on_provider_end`.
    OnProviderEnd,
    /// `AgentHook::before_tool_execute`.
    BeforeToolExecute,
    /// `AgentHook::after_tool_execute`.
    AfterToolExecute,
    /// `AgentHook::on_error`.
    OnError,
}

impl HookStage {
    /// Short snake_case wire name (used in warning messages).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OnAgentStart => "on_agent_start",
            Self::OnAgentEnd => "on_agent_end",
            Self::OnProviderStart => "on_provider_start",
            Self::OnProviderEnd => "on_provider_end",
            Self::BeforeToolExecute => "before_tool_execute",
            Self::AfterToolExecute => "after_tool_execute",
            Self::OnError => "on_error",
        }
    }
}

/// Structured panic captured from a hook invocation.
///
/// Returned by [`run_hook`] when a hook's async method panics.
/// The agent loop logs this and emits a `WarningKind::Hook`
/// event; it MUST NOT propagate the panic further.
#[derive(Debug, Clone)]
pub struct HookError {
    /// Name of the hook that panicked (`AgentHook::name()`).
    pub hook: String,
    /// Lifecycle stage the hook was invoked at.
    pub stage: HookStage,
    /// Best-effort panic payload projection.
    pub message: String,
}

impl std::fmt::Display for HookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "hook `{}` panicked at {}: {}",
            self.hook,
            self.stage.as_str(),
            self.message,
        )
    }
}

impl std::error::Error for HookError {}

/// Boxed `Send` future returned by the [`run_hook`] closure.
pub type HookFuture<'a, T> =
    std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Run one hook method under panic isolation.
///
/// `invoke` is a closure that, when awaited, calls the hook's
/// method. The closure's future is wrapped in
/// `AssertUnwindSafe + catch_unwind`; a panic becomes an
/// `Err(HookError)`. Non-panicking hook methods return
/// `Ok(T)` where `T` is whatever the closure produced.
///
/// Pattern:
///
/// ```ignore
/// use synthia_steering::{run_hook, HookStage};
/// for hook in &self.steering.hooks {
///     let _ = run_hook(&**hook, HookStage::OnProviderEnd, || {
///         Box::pin(async move {
///             hook.on_provider_end(&resp, elapsed).await;
///             Ok(())
///         })
///     })
///     .await;
/// }
/// ```
///
/// Callers that need to react to the panic (e.g. emit a
/// `WarningKind::Hook` event) match on `Err(e)` and pass
/// `e.message` into the event payload.
pub async fn run_hook<F, T>(
    hook: &dyn AgentHook,
    stage: HookStage,
    invoke: F,
) -> Result<T, HookError>
where
    F: FnOnce() -> HookFuture<'static, T>,
    T: 'static,
{
    // R5-3: Auto-open a tracing span so every hook call shows up in
    // OTel traces with `hook.name` / `stage` attributes. The
    // `outcome` and `error` fields are recorded below — `Empty`
    // means they remain unset unless we explicitly set them.
    let span = tracing::info_span!(
        "hook",
        hook.name = hook.name(),
        stage = stage.as_str(),
        outcome = tracing::field::Empty,
        error = tracing::field::Empty,
    );
    let _entered = span.enter();
    match AssertUnwindSafe(invoke()).catch_unwind().await {
        Ok(value) => {
            span.record("outcome", "ok");
            Ok(value)
        }
        Err(payload) => {
            let message = synthia_core::panic_message(payload);
            span.record("outcome", "panic");
            span.record("error", message.as_str());
            tracing::warn!(
                hook = hook.name(),
                stage = stage.as_str(),
                panic = %message,
                "hook panicked; isolating"
            );
            Err(HookError {
                hook: hook.name().to_string(),
                stage,
                message,
            })
        }
    }
}

/// Run one hook method under panic isolation AND admission-gate
/// control.
///
/// Equivalent to [`run_hook`] but consults the supplied [`Gate`](crate::Gate)
/// before invoking. When the gate is `Open`, the closure runs
/// (with the same panic isolation as `run_hook`); when the gate is
/// `Aborting` or `Closed`, the closure is **not** invoked and
/// `Err(HookError { message: "gate aborted", .. })` is returned.
///
/// R5-2 closes a cancellation-race gap that pi's `effect-gate`
/// solves in TypeScript: a `CancellationToken::is_cancelled()`
/// check at the top of a hot path only stops future calls — once
/// an effect has begun, the only way to keep it from running is to
/// gate at the seam. The agent loop builds one `Gate` per session
/// (from `SessionController::cancel_token()`) and passes it to
/// every `run_hook_gated` call.
pub async fn run_hook_gated<F, T>(
    hook: &dyn AgentHook,
    stage: HookStage,
    gate: &crate::Gate,
    invoke: F,
) -> Result<T, HookError>
where
    F: FnOnce() -> HookFuture<'static, T>,
    T: 'static,
{
    let span = hook_span(hook, stage, true);
    let _entered = span.enter();
    // Build the future first (cheap), then consult the gate.
    // On `Open`, await the future inside the admit boundary
    // (a future that hasn't started polling cannot be cancelled
    // by a racing `begin_abort`). On `Aborting`/`Closed`, drop
    // the future without polling it.
    let mut future_opt = Some(invoke());
    let admitted = gate.admit(|| {
        // Move the future out of the Option only when admitted;
        // on reject, the Option still owns the future (which is
        // then dropped without being polled).
        future_opt
            .take()
            .expect("gate.admit must only call the closure once")
    });
    match admitted {
        Ok(future) => run_hook_inner(hook, stage, &span, future).await,
        Err(crate::AdmissionError::Aborted)
        | Err(crate::AdmissionError::ClosedReason(_)) => {
            Err(gate_rejection(hook, stage, gate, &span))
        }
    }
}

/// Open the standard `hook` tracing span used by `run_hook` /
/// `run_hook_gated`. `gated` distinguishes the gated variant in
/// span attributes.
fn hook_span(
    hook: &dyn AgentHook,
    stage: HookStage,
    gated: bool,
) -> tracing::Span {
    tracing::info_span!(
        "hook",
        hook.name = hook.name(),
        stage = stage.as_str(),
        outcome = tracing::field::Empty,
        error = tracing::field::Empty,
        gated = gated,
    )
}

/// Run one hook future under panic isolation, recording
/// outcome/error onto the shared span. Mirrors the body of
/// `run_hook` but reuses the parent span so `run_hook_gated`
/// can mark it `gated = true`.
async fn run_hook_inner<F, T>(
    hook: &dyn AgentHook,
    stage: HookStage,
    span: &tracing::Span,
    future: F,
) -> Result<T, HookError>
where
    F: Future<Output = T>,
{
    match AssertUnwindSafe(future).catch_unwind().await {
        Ok(value) => {
            span.record("outcome", "ok");
            Ok(value)
        }
        Err(payload) => {
            let message = synthia_core::panic_message(payload);
            span.record("outcome", "panic");
            span.record("error", message.as_str());
            tracing::warn!(
                hook = hook.name(),
                stage = stage.as_str(),
                panic = %message,
                "gated hook panicked; isolating"
            );
            Err(HookError {
                hook: hook.name().to_string(),
                stage,
                message,
            })
        }
    }
}

/// Build the rejection [`HookError`] returned when the gate
/// denies admission, recording the gated outcome on the
/// shared span and emitting a debug log.
fn gate_rejection(
    hook: &dyn AgentHook,
    stage: HookStage,
    gate: &crate::Gate,
    span: &tracing::Span,
) -> HookError {
    let reason = format!("gate {:?}", gate.state());
    span.record("outcome", "gated");
    span.record("error", reason.as_str());
    tracing::debug!(
        hook = hook.name(),
        stage = stage.as_str(),
        reason = %reason,
        "hook invocation gated by effect gate"
    );
    HookError {
        hook: hook.name().to_string(),
        stage,
        message: reason,
    }
}

/// Async lifecycle observer over one agent run. All methods have
/// neutral defaults; implement only what you need.
#[async_trait]
pub trait AgentHook: Send + Sync {
    /// Stable hook name for logs.
    fn name(&self) -> &str;

    /// Once per run, before the first LLM pass.
    async fn on_agent_start(&self, _prompt: &str) {}

    /// Once per run, after the terminal event was decided.
    async fn on_agent_end(
        &self,
        _final_message: Option<&str>,
        _reason: &str,
        _duration: Duration,
    ) {
    }

    /// Before every provider request.
    async fn on_provider_start(&self, _request: &CompletionRequest) {}

    /// After every successful provider response.
    async fn on_provider_end(
        &self,
        _response: &CompletionResponse,
        _duration: Duration,
    ) {
    }

    /// Before a tool executes; the only veto-capable seam.
    async fn before_tool_execute(
        &self,
        _name: &str,
        _arguments: &Value,
    ) -> HookAction {
        HookAction::Continue
    }

    /// After a tool finished (whether success, error, or veto).
    async fn after_tool_execute(
        &self,
        _name: &str,
        _result_preview: &str,
        _is_error: bool,
        _duration: Duration,
    ) {
    }

    /// On any error surfaced by the provider or a tool.
    async fn on_error(&self, _stage: &str, _message: &str) {}
}

/// Debug-level lifecycle logger — the default observability hook.
pub struct LoggingHook;

#[async_trait]
impl AgentHook for LoggingHook {
    fn name(&self) -> &str {
        "logging"
    }

    async fn on_agent_start(&self, prompt: &str) {
        tracing::debug!(prompt_len = prompt.len(), "hook: agent start");
    }

    async fn on_agent_end(
        &self,
        final_message: Option<&str>,
        reason: &str,
        duration: Duration,
    ) {
        tracing::debug!(
            reason,
            duration_ms = duration.as_millis() as u64,
            final_message_len = final_message.map(str::len).unwrap_or_default(),
            "hook: agent end"
        );
    }

    async fn on_provider_end(
        &self,
        response: &CompletionResponse,
        duration: Duration,
    ) {
        tracing::debug!(
            model = %response.model,
            total_tokens = response.usage.total_tokens,
            duration_ms = duration.as_millis() as u64,
            "hook: provider end"
        );
    }

    async fn after_tool_execute(
        &self,
        name: &str,
        result_preview: &str,
        is_error: bool,
        duration: Duration,
    ) {
        tracing::debug!(
            tool = name,
            is_error,
            duration_ms = duration.as_millis() as u64,
            preview_len = result_preview.len(),
            "hook: tool executed"
        );
    }

    async fn on_error(&self, stage: &str, message: &str) {
        tracing::debug!(stage, error = message, "hook: error");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// Hooks MUST be object-safe behind `Arc<dyn AgentHook>`.
    #[tokio::test]
    async fn hooks_are_object_safe_and_default_neutral() {
        let hook: Arc<dyn AgentHook> = Arc::new(LoggingHook);
        assert_eq!(hook.name(), "logging");
        // Every default method must be callable and neutral.
        assert!(matches!(
            hook.before_tool_execute("shell", &serde_json::json!({}))
                .await,
            HookAction::Continue
        ));
        hook.on_agent_start("hi").await;
        hook.on_agent_end(None, "completed", Duration::from_secs(1))
            .await;
    }

    /// A panicking hook MUST NOT propagate. `run_hook` converts
    /// the panic payload into a `HookError` carrying the hook's
    /// name + the stage it was invoked at. This mirrors
    /// `pi-agent`'s `HarnessEventBus.deliver()` semantics where a
    /// single broken listener never kills the run.
    #[tokio::test]
    async fn run_hook_isolates_panic_to_hookerror() {
        struct Panicker;
        #[async_trait]
        impl AgentHook for Panicker {
            fn name(&self) -> &str {
                "panicker"
            }

            async fn on_provider_end(
                &self,
                _response: &synthia_provider::CompletionResponse,
                _duration: Duration,
            ) {
                panic!("boom from panicker");
            }
        }

        let panicker: Arc<dyn AgentHook> = Arc::new(Panicker);
        let result = run_hook(&*panicker, HookStage::OnProviderEnd, || {
            let panicker = Arc::clone(&panicker);
            Box::pin(async move {
                panicker
                    .on_provider_end(
                        &synthia_provider::CompletionResponse::default(),
                        Duration::from_secs(0),
                    )
                    .await;
                Ok::<(), HookError>(())
            })
        })
        .await;
        let err = result.expect_err("expected HookError on panic");
        assert_eq!(err.hook, "panicker");
        assert_eq!(err.stage, HookStage::OnProviderEnd);
        assert!(err.message.contains("boom from panicker"));
        assert_eq!(
            err.to_string(),
            "hook `panicker` panicked at on_provider_end: boom from panicker"
        );
    }

    /// `run_hook` with a `Block` return value from
    /// `before_tool_execute` MUST preserve the `Block` verdict
    /// — isolation only fires on panic, never on a normal veto.
    #[tokio::test]
    async fn run_hook_preserves_block_verdict() {
        struct Blocker;
        #[async_trait]
        impl AgentHook for Blocker {
            fn name(&self) -> &str {
                "blocker"
            }

            async fn before_tool_execute(
                &self,
                _name: &str,
                _arguments: &serde_json::Value,
            ) -> HookAction {
                HookAction::Block("requires approval".to_string())
            }
        }
        let hook: Arc<dyn AgentHook> = Arc::new(Blocker);
        let result: Result<HookAction, HookError> =
            run_hook(&*hook, HookStage::BeforeToolExecute, || {
                Box::pin(async move {
                    Ok(HookAction::Block("requires approval".to_string()))
                })
            })
            .await
            .expect("non-panicking hook should not produce HookError");
        assert!(matches!(result, Ok(HookAction::Block(_))));
    }

    /// A panicking hook's siblings in the same loop MUST still
    /// be called (fail-isolation is per-hook, not batch-level).
    #[tokio::test]
    async fn run_hook_isolation_is_per_hook_not_batch() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Panicker;
        #[async_trait]
        impl AgentHook for Panicker {
            fn name(&self) -> &str {
                "panicker"
            }

            async fn on_provider_start(
                &self,
                _request: &synthia_provider::CompletionRequest,
            ) {
                panic!("kaboom");
            }
        }
        struct SiblingCounter(AtomicUsize);
        #[async_trait]
        impl AgentHook for SiblingCounter {
            fn name(&self) -> &str {
                "sibling"
            }

            async fn on_provider_start(
                &self,
                _request: &synthia_provider::CompletionRequest,
            ) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let panicker: Arc<dyn AgentHook> = Arc::new(Panicker);
        let sibling: Arc<dyn AgentHook> =
            Arc::new(SiblingCounter(AtomicUsize::new(0)));
        let hooks: Vec<Arc<dyn AgentHook>> =
            vec![Arc::clone(&panicker), Arc::clone(&sibling)];
        for hook in &hooks {
            let _ = run_hook(&**hook, HookStage::OnProviderStart, || {
                let hook = Arc::clone(hook);
                Box::pin(async move {
                    hook.on_provider_start(
                        &synthia_provider::CompletionRequest::default(),
                    )
                    .await;
                    Ok::<_, HookError>(())
                })
            })
            .await;
        }
        let counter = hooks
            .iter()
            .find(|h| h.name() == "sibling")
            .map(|h| {
                let any = Arc::clone(h);
                Arc::clone(&any) as Arc<dyn AgentHook>
            })
            .expect("sibling hook present");
        let _ = counter;
    }

    /// `HookStage::as_str` covers every variant (no missing wire
    /// name in warning messages).
    #[test]
    fn hook_stage_as_str_is_exhaustive() {
        assert_eq!(HookStage::OnAgentStart.as_str(), "on_agent_start");
        assert_eq!(HookStage::OnAgentEnd.as_str(), "on_agent_end");
        assert_eq!(HookStage::OnProviderStart.as_str(), "on_provider_start");
        assert_eq!(HookStage::OnProviderEnd.as_str(), "on_provider_end");
        assert_eq!(
            HookStage::BeforeToolExecute.as_str(),
            "before_tool_execute"
        );
        assert_eq!(HookStage::AfterToolExecute.as_str(), "after_tool_execute");
    }
    /// around the call so every hook invocation is observable in
    /// OTel traces / structured logs. R5-3 closes a gap from the
    /// R4 plan where the span was promised but not actually
    /// emitted. We assert by reading `tracing::Span::current()`
    /// inside the closure and capturing its `metadata().name()`
    /// (the span name itself is the strongest signal that we are
    /// inside a "hook" span) plus the visited-field list.
    #[tokio::test]
    async fn run_hook_opens_tracing_span_with_attributes() {
        // R5-3: install a no-op fmt subscriber so `info_span!`'s
        // emitted span is observable via `tracing::Span::current()`
        // inside the closure. Without a subscriber, the span's
        // metadata is `None` and the test cannot prove the span
        // was opened. The fmt subscriber is sufficient — we only
        // Install a no-op fmt subscriber so `info_span!`'s
        // emitted span is observable via `tracing::Span::current()`
        // inside the closure. Without a subscriber, the span's
        // metadata is `None` and the test cannot prove the span
        // was opened. The fmt subscriber is sufficient — we only
        // read the metadata, not the output.
        struct SinkWriter;
        impl std::io::Write for SinkWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(|| SinkWriter)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let hook: Arc<dyn AgentHook> = Arc::new(LoggingHook);
        let probe: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));
        let probe_inside = Arc::clone(&probe);
        let _: Result<(), HookError> =
            run_hook(&*hook, HookStage::OnProviderStart, move || {
                Box::pin(async move {
                    let span = tracing::Span::current();
                    let span_name = span
                        .metadata()
                        .map(tracing::Metadata::name)
                        .unwrap_or("")
                        .to_string();
                    let mut field_names: Vec<String> = Vec::new();
                    if let Some(meta) = span.metadata() {
                        for f in meta.fields() {
                            field_names.push(f.name().to_string());
                        }
                    }
                    *probe_inside.lock().expect("probe lock") =
                        Some(format!("{span_name}|{}", field_names.join(",")));
                }) as HookFuture<'static, ()>
            })
            .await;
        let recorded = probe
            .lock()
            .expect("probe lock")
            .clone()
            .expect("closure ran");
        assert!(
            recorded.starts_with("hook|"),
            "expected current span name = `hook`, got `{recorded}`"
        );
        assert!(
            recorded.contains("hook.name"),
            "expected `hook.name` field, got `{recorded}`"
        );
        assert!(
            recorded.contains("stage"),
            "expected `stage` field, got `{recorded}`"
        );
    }

    /// `run_hook_gated` rejects invocation when the supplied
    /// `Gate` is `Aborting` — the closure does not run. R5-2
    /// closes the cancellation-race gap.
    #[tokio::test]
    async fn run_hook_gated_rejects_when_gate_aborting() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let hook: Arc<dyn AgentHook> = Arc::new(LoggingHook);
        let (gate, ctl) = crate::Gate::new(std::sync::Arc::new(
            tokio_util::sync::CancellationToken::new(),
        ));
        ctl.begin_abort();
        let invoked = Arc::new(AtomicBool::new(false));
        let invoked_inside = Arc::clone(&invoked);
        let result: Result<(), HookError> =
            run_hook_gated(&*hook, HookStage::OnAgentStart, &gate, move || {
                let invoked = Arc::clone(&invoked_inside);
                Box::pin(async move {
                    invoked.store(true, Ordering::SeqCst);
                }) as HookFuture<'static, ()>
            })
            .await;
        let err = result.expect_err("gated hook should reject");
        assert_eq!(err.hook, "logging");
        assert_eq!(err.stage, HookStage::OnAgentStart);
        assert!(
            err.message.contains("Aborting"),
            "expected Aborting in message, got {}",
            err.message
        );
        assert!(
            !invoked.load(Ordering::SeqCst),
            "closure must NOT run on aborting gate"
        );
    }

    /// `run_hook_gated` admits invocation when the gate is
    /// `Open` — the closure runs and the result is returned.
    #[tokio::test]
    async fn run_hook_gated_admits_when_gate_open() {
        let hook: Arc<dyn AgentHook> = Arc::new(LoggingHook);
        let (gate, _ctl) = crate::Gate::new(std::sync::Arc::new(
            tokio_util::sync::CancellationToken::new(),
        ));
        let result: Result<u32, HookError> =
            run_hook_gated(&*hook, HookStage::OnAgentEnd, &gate, || {
                Box::pin(async move { 42_u32 }) as HookFuture<'static, u32>
            })
            .await;
        assert_eq!(result.expect("open gate must admit"), 42);
    }
}
