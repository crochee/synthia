use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use synthia_permission::{ApprovalOutcome, Permission};
use synthia_sandbox::SandboxAttempt;
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

use crate::{
    ActiveCall,
    ActiveCallGuard,
    ConflictInfo,
    DefaultToolOrchestrator,
    ExecutionContext,
    ToolCallRequest,
    ToolCallResult,
    ToolExecutionError,
    ToolOrchestrator,
    ToolOrchestratorError,
    ToolOrchestratorEvent,
    ToolResolver,
    apply_provenance_floor,
    capability_for_tool_name,
    check_conflict,
    extract_file_path,
    is_read_tool,
    is_write_tool,
    record_read,
};

impl DefaultToolOrchestrator {
    pub(crate) fn emit(&self, event: ToolOrchestratorEvent) {
        let _ = self.event_sender.send(event);
    }

    pub(crate) async fn acquire_tool_lock(
        &self,
        tool_name: &str,
    ) -> Option<OwnedMutexGuard<()>> {
        let lock = self
            .per_tool_locks
            .entry(tool_name.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        Some(lock.lock_owned().await)
    }

    pub(crate) async fn check_edit_conflict(
        &self,
        path: &PathBuf,
    ) -> Option<ConflictInfo> {
        check_conflict(path, &self.snapshot_store).await
    }

    pub(crate) async fn record_file_read(
        &self,
        path: &PathBuf,
        content: &[u8],
    ) {
        record_read(path, content, &self.snapshot_store).await;
    }
}

#[async_trait]
impl ToolOrchestrator for DefaultToolOrchestrator {
    async fn execute(
        &self,
        mut request: ToolCallRequest,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolOrchestratorError> {
        if cancellation_token.is_cancelled() {
            self.emit(ToolOrchestratorEvent::Cancelled {
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
            });
            return Err(ToolOrchestratorError::cancelled(&request.call_id));
        }

        let call_id = request.call_id.clone();
        let tool_name = request.tool_name.clone();

        // Use a child token so that `cancel(call_id)` can cancel just this
        // call while still honouring the caller-wide cancellation token.
        let call_token = cancellation_token.child_token();
        self.active_calls.insert(
            call_id.clone(),
            ActiveCall {
                tool_name: tool_name.clone(),
                token: call_token.clone(),
            },
        );
        let _active_guard = ActiveCallGuard {
            map: self.active_calls.clone(),
            call_id: call_id.clone(),
        };

        let tool = match self.tool_resolver.resolve(&tool_name) {
            Some(tool) => tool,
            None => {
                let error = format!("tool '{}' not found", tool_name);
                self.emit(ToolOrchestratorEvent::Failed {
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                    error: error.clone(),
                });
                return Err(ToolOrchestratorError::NotFound {
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                });
            }
        };

        // Populate tool_id from materialization if the resolver is
        // available and the caller did not already supply one.
        if request.tool_id.is_none()
            && let Some(ref resolver) = self.tool_id_resolver
        {
            request.tool_id = resolver.resolve_id(&tool_name);
        }

        self.emit(ToolOrchestratorEvent::Started {
            call_id: call_id.clone(),
            tool_name: tool_name.clone(),
        });

        // Compute the effective permission by applying:
        // 1. Provenance floor — sets the minimum permission level based on
        //    tool origin (Builtin -> AutoApprove, Plugin -> RequireConfirm,
        //    Ephemeral -> RequireExplicit).
        // 2. Capability upgrade — if the broker denies the tool's declared
        //    capability, upgrade the effective permission to Deny.
        let mut effective_permission = if let Some(ref resolver) =
            self.tool_provenance_resolver
        {
            if let Some(provenance) = resolver.resolve_provenance(&tool_name) {
                apply_provenance_floor(&provenance, request.permission.clone())
            } else {
                request.permission.clone()
            }
        } else {
            request.permission.clone()
        };

        // Capability upgrade: if the broker denies the tool's capability,
        // upgrade effective permission to Deny. This runs after the
        // provenance floor so that capability denial is expressed as a
        // permission-level Deny that flows through the approval system
        // normally.
        if !matches!(
            effective_permission,
            Permission::Block | Permission::Deny { .. }
        ) && let Some(ref broker) = self.capability_broker
            && let Some(cap) = capability_for_tool_name(&tool_name)
            && !broker.allowed(cap)
        {
            effective_permission = Permission::Deny {
                reason: format!("capability '{}' denied by broker", cap),
            };
        }

        // Approval check driven by the effective permission level.
        let approval_outcome = match &effective_permission {
            Permission::AutoApprove => ApprovalOutcome::Approve,
            Permission::RequireConfirm | Permission::RequireExplicit => {
                let permission_request =
                    synthia_permission::PermissionRequest::new(
                        tool_name.clone(),
                        request.arguments.clone(),
                        true,
                    );
                let permission_future =
                    self.approval_service.ask(permission_request);
                match permission_future
                    .await_with_cancellation(&call_token)
                    .await
                {
                    Ok(outcome) => outcome.outcome.into(),
                    Err(_) => ApprovalOutcome::Deny,
                }
            }
            Permission::Block | Permission::Deny { .. } => {
                ApprovalOutcome::Deny
            }
        };

        if approval_outcome == ApprovalOutcome::Deny {
            self.emit(ToolOrchestratorEvent::Failed {
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                error: "denied".to_string(),
            });
            return Err(ToolOrchestratorError::denied(&call_id));
        }

        // Sandbox selection. Platform availability is checked here; the caller
        // applies the resulting attempt inside the tool if it spawns commands.
        let mut sandbox_attempt = self
            .sandbox_manager
            .select(
                self.sandbox_policy.clone(),
                &tool_name,
                std::env::consts::OS,
            )
            .await
            .map_err(|e| ToolOrchestratorError::Sandbox {
                call_id: call_id.clone(),
                message: e.to_string(),
            })?;

        if matches!(sandbox_attempt, SandboxAttempt::Unavailable) {
            match self.sandbox_policy.on_unavailable() {
                synthia_sandbox::OnUnavailable::Deny => {
                    self.emit(ToolOrchestratorEvent::Failed {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                        error: "sandbox unavailable".to_string(),
                    });
                    return Err(ToolOrchestratorError::Sandbox {
                        call_id: call_id.clone(),
                        message: "sandbox unavailable".to_string(),
                    });
                }
                synthia_sandbox::OnUnavailable::Prompt => {
                    // Prompt policy explicitly allows falling back to
                    // unsandboxed execution. Emit a warning event so the
                    // decision is observable (P6/P9) and continue with no
                    // sandboxing.
                    self.emit(ToolOrchestratorEvent::Failed {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                        error: "sandbox unavailable; continuing unsandboxed per policy"
                            .to_string(),
                    });
                    sandbox_attempt = SandboxAttempt::None;
                }
            }
        }

        // Serialize non-concurrency-safe tools.
        let _tool_lock = if tool.is_concurrency_safe() {
            None
        } else {
            self.acquire_tool_lock(&tool_name).await
        };

        let event_sender = self.event_sender.clone();
        let event_call_id = call_id.clone();
        let event_tool_name = tool_name.clone();
        let on_event_arc: Option<
            Arc<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
        > = Some(Arc::new(move |event| {
            let _ = event_sender.send(ToolOrchestratorEvent::FileChange {
                call_id: event_call_id.clone(),
                tool_name: event_tool_name.clone(),
                event,
            });
        }));

        let mut attempt = 0u32;
        loop {
            if call_token.is_cancelled() {
                self.emit(ToolOrchestratorEvent::Cancelled {
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                });
                return Err(ToolOrchestratorError::cancelled(&call_id));
            }

            if is_write_tool(&tool_name)
                && let Some(path) = extract_file_path(
                    &tool_name,
                    &request.arguments,
                    &context.workspace_root,
                )
                && let Some(conflict) = self.check_edit_conflict(&path).await
            {
                let original_hash = conflict.agent_snapshot.content_hash;
                let current_hash = conflict.current_hash;
                self.emit(ToolOrchestratorEvent::EditConflict {
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                    path: path.clone(),
                    conflict,
                });
                return Err(ToolOrchestratorError::EditConflict {
                    call_id: call_id.clone(),
                    path,
                    original_content_hash: original_hash,
                    current_content_hash: current_hash,
                });
            }

            let on_event: Option<
                Box<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
            > = on_event_arc.as_ref().map(|arc| {
                let arc = arc.clone();
                Box::new(move |event| arc(event))
                    as Box<dyn Fn(_) + Send + Sync>
            });

            attempt += 1;
            match tool
                .execute_with_events(
                    &request,
                    &context,
                    &sandbox_attempt,
                    call_token.clone(),
                    on_event,
                )
                .await
            {
                Ok(result) => {
                    if is_read_tool(&tool_name)
                        && let Some(path) = extract_file_path(
                            &tool_name,
                            &request.arguments,
                            &context.workspace_root,
                        )
                        && let Some(text) =
                            result.outcome.get("text").and_then(|v| v.as_str())
                    {
                        self.record_file_read(&path, text.as_bytes()).await;
                    }
                    self.emit(ToolOrchestratorEvent::Completed {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                        result: result.clone(),
                        tool_id: request.tool_id,
                    });
                    return Ok(result);
                }
                Err(ToolExecutionError::Cancelled) => {
                    self.emit(ToolOrchestratorEvent::Cancelled {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                    });
                    return Err(ToolOrchestratorError::cancelled(&call_id));
                }
                Err(err) => {
                    let is_transient = err.is_transient();
                    let error_message = err.to_string();
                    if attempt >= self.retry_policy.max_attempts
                        || !is_transient
                    {
                        self.emit(ToolOrchestratorEvent::Failed {
                            call_id: call_id.clone(),
                            tool_name: tool_name.clone(),
                            error: error_message.clone(),
                        });
                        return Err(ToolOrchestratorError::generic(
                            &call_id,
                            error_message,
                        ));
                    }

                    let delay = Duration::from_millis(
                        self.retry_policy.base_delay_ms
                            * 2_u64.pow(attempt.saturating_sub(1)),
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {}
                        _ = call_token.cancelled() => {
                            self.emit(ToolOrchestratorEvent::Cancelled {
                                call_id: call_id.clone(),
                                tool_name: tool_name.clone(),
                            });
                            return Err(ToolOrchestratorError::cancelled(&call_id));
                        }
                    }
                }
            }
        }
    }

    async fn execute_batch(
        &self,
        requests: Vec<ToolCallRequest>,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ToolCallResult>, ToolOrchestratorError> {
        // Phase 1 / Task 1.3.1-1.3.4 — if any tool in the batch is
        // `Sequential` (i.e. requires strict serial execution), downgrade
        // the whole batch to a serial loop. Otherwise keep parallel
        // fan-out limited by `concurrency_policy.max_concurrent`.
        let needs_serial =
            needs_serial_routing(&requests, self.tool_resolver.as_ref());

        if needs_serial {
            let mut results = Vec::with_capacity(requests.len());
            for request in requests {
                if cancellation_token.is_cancelled() {
                    return Err(ToolOrchestratorError::Cancelled {
                        call_id: String::new(),
                    });
                }
                let ctx = context.clone();
                let cancel = cancellation_token.clone();
                let result = self.execute(request, ctx, cancel).await?;
                results.push(result);
            }
            Ok(results)
        } else {
            let futures = requests.into_iter().map(|request| {
                let ctx = context.clone();
                let cancel = cancellation_token.clone();
                async move { self.execute(request, ctx, cancel).await }
            });

            let results: Vec<Result<ToolCallResult, ToolOrchestratorError>> =
                stream::iter(futures)
                    .buffer_unordered(self.concurrency_policy.max_concurrent)
                    .collect()
                    .await;

            results.into_iter().collect()
        }
    }

    fn event_stream(
        &self,
    ) -> tokio::sync::broadcast::Receiver<ToolOrchestratorEvent> {
        self.event_sender.subscribe()
    }

    async fn cancel(&self, call_id: &str) -> Result<(), ToolOrchestratorError> {
        let tool_name = self
            .active_calls
            .get(call_id)
            .map(|e| e.value().tool_name.clone())
            .unwrap_or_default();
        if let Some(entry) = self.active_calls.get(call_id) {
            entry.value().token.cancel();
        }
        self.emit(ToolOrchestratorEvent::Cancelled {
            call_id: call_id.to_string(),
            tool_name,
        });
        Ok(())
    }

    fn fail_interrupted_tools(&self) -> usize {
        let mut count = 0usize;
        // Snapshot (call_id, tool_name, token) first so we do not hold
        // the DashMap iterator across mutation. DashMap iterators
        // borrow the underlying shard shards and cannot outlive
        // removals.
        let entries: Vec<(String, String, CancellationToken)> = self
            .active_calls
            .iter()
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.value().tool_name.clone(),
                    entry.value().token.clone(),
                )
            })
            .collect();

        for (call_id, tool_name, token) in entries {
            // Cancel first to signal the tool. Cancelling a token that
            // has already been cancelled (or whose call has completed)
            // is a no-op.
            token.cancel();
            // Remove from active_calls. If `ActiveCallGuard::drop`
            // already removed the entry (concurrent completion), skip
            // the emit so we do not double-report.
            if self.active_calls.remove(&call_id).is_some() {
                count += 1;
                self.emit(ToolOrchestratorEvent::Failed {
                    call_id: call_id.clone(),
                    tool_name,
                    error: "Tool execution interrupted".to_string(),
                });
            }
        }
        count
    }
}

/// Decide whether a batch of tool calls must run sequentially.
///
/// Returns `true` when **any** resolved tool declares
/// [`synthia_tool::traits::ExecutionMode::Sequential`]. The orchestrator
/// uses this to downgrade a whole batch to a serial loop whenever a
/// mutating tool is involved — running a mutating tool concurrently
/// with other tools (even read-only ones) can produce surprising
/// interleaving (e.g. `write` racing against `read` of the same path).
///
/// Tools that cannot be resolved are treated as sequential (the safe
/// default — better to serialize than to race on an unknown tool).
pub fn needs_serial_routing(
    requests: &[ToolCallRequest],
    resolver: &dyn ToolResolver,
) -> bool {
    requests.iter().any(|req| {
        resolver
            .resolve(&req.tool_name)
            .map(|tool| {
                tool.execution_mode()
                    == synthia_tool::traits::ExecutionMode::Sequential
            })
            .unwrap_or(true)
    })
}

/// Adapter that wraps a [`synthia_tool::Tool`] so it can be used as an
/// [`ExecutableTool`](crate::ExecutableTool) inside the orchestrator.
pub mod adapter {
    use std::sync::Arc;

    use async_trait::async_trait;
    use synthia_tool_exec_base::FileMutationQueue;
    use tokio_util::sync::CancellationToken;

    use crate::{
        ExecutableTool,
        ExecutionContext,
        ToolCallRequest,
        ToolCallResult,
        ToolExecutionError,
        extract_file_path,
    };

    /// Wraps a `synthia_tool::Tool` to expose the orchestrator's
    /// `ExecutableTool` interface.
    ///
    /// When constructed with [`ToolAdapter::with_file_queue`], file-mutating
    /// tools (`write`, `multi_edit`) acquire a per-filepath mutex before
    /// the underlying tool is invoked, serializing concurrent writes to
    /// the same realpath while allowing parallel writes to different files.
    #[derive(Clone)]
    pub struct ToolAdapter {
        tool: Arc<dyn synthia_tool::Tool>,
        file_queue: Option<FileMutationQueue>,
    }

    impl ToolAdapter {
        pub fn new(tool: Arc<dyn synthia_tool::Tool>) -> Self {
            Self {
                tool,
                file_queue: None,
            }
        }

        pub fn with_file_queue(
            tool: Arc<dyn synthia_tool::Tool>,
            file_queue: FileMutationQueue,
        ) -> Self {
            Self {
                tool,
                file_queue: Some(file_queue),
            }
        }
    }

    #[async_trait]
    impl ExecutableTool for ToolAdapter {
        fn name(&self) -> &str {
            self.tool.name()
        }

        fn is_concurrency_safe(&self) -> bool {
            self.tool.is_concurrency_safe()
        }

        fn execution_mode(&self) -> synthia_tool::traits::ExecutionMode {
            self.tool.execution_mode()
        }

        async fn execute(
            &self,
            request: &ToolCallRequest,
            context: &ExecutionContext,
            sandbox_attempt: &synthia_sandbox::SandboxAttempt,
            cancellation_token: CancellationToken,
        ) -> Result<ToolCallResult, ToolExecutionError> {
            if request.tool_name != self.tool.name() {
                return Err(ToolExecutionError::Permanent(format!(
                    "tool name mismatch: expected '{}', got '{}'",
                    self.tool.name(),
                    request.tool_name
                )));
            }

            validate_tool_input(&request.arguments, &self.tool.parameters())?;

            // Acquire per-filepath lock for file-mutating tools so that
            // concurrent writes to the same realpath are serialized.
            let _file_guard = if let Some(queue) = &self.file_queue
                && let Some(path) = extract_file_path(
                    &request.tool_name,
                    &request.arguments,
                    &context.workspace_root,
                ) {
                Some(queue.acquire(path).await)
            } else {
                None
            };

            let tool_context = synthia_tool::types::ToolExecutionContext::new(
                context.session_id.clone(),
                context.workspace_root.clone(),
            );

            let input = synthia_tool::ToolInput {
                name: request.tool_name.clone(),
                input: request.arguments.clone(),
                context: tool_context,
            };

            // Route through `call_with_sandbox` so tools that spawn
            // subprocesses (bash) apply the orchestrator-selected
            // `SandboxAttempt`. Pure tools (file I/O, search) keep the
            // default impl which ignores `sandbox_attempt` and delegates
            // to `Tool::call`. Previously this called `self.tool.call`
            // directly, discarding the sandbox — that was the U1
            // single-point-of-failure (bash ran with parent-process
            // privileges regardless of the policy verdict).
            let output = self
                .tool
                .call_with_sandbox(input, sandbox_attempt, &cancellation_token)
                .await;
            let outcome =
                serde_json::to_value(&output.content).map_err(|e| {
                    ToolExecutionError::Permanent(format!(
                        "failed to serialize tool output: {}",
                        e
                    ))
                })?;

            Ok(ToolCallResult {
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
                outcome,
                is_error: output.is_error.unwrap_or(false),
                tool_id: request.tool_id,
            })
        }

        async fn execute_with_events(
            &self,
            request: &ToolCallRequest,
            context: &ExecutionContext,
            sandbox_attempt: &synthia_sandbox::SandboxAttempt,
            cancellation_token: CancellationToken,
            on_event: Option<
                Box<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
            >,
        ) -> Result<ToolCallResult, ToolExecutionError> {
            if request.tool_name != self.tool.name() {
                return Err(ToolExecutionError::Permanent(format!(
                    "tool name mismatch: expected '{}', got '{}'",
                    self.tool.name(),
                    request.tool_name
                )));
            }

            validate_tool_input(&request.arguments, &self.tool.parameters())?;

            let _file_guard = if let Some(queue) = &self.file_queue
                && let Some(path) = extract_file_path(
                    &request.tool_name,
                    &request.arguments,
                    &context.workspace_root,
                ) {
                Some(queue.acquire(path).await)
            } else {
                None
            };

            let tool_context = synthia_tool::types::ToolExecutionContext::new(
                context.session_id.clone(),
                context.workspace_root.clone(),
            );

            let input = synthia_tool::ToolInput {
                name: request.tool_name.clone(),
                input: request.arguments.clone(),
                context: tool_context,
            };

            let output = if let Some(on_event) = on_event {
                let callback = std::sync::Arc::new(on_event);
                self.tool
                    .call_with_progress(input, callback, &cancellation_token)
                    .await
            } else {
                self.tool
                    .call_with_sandbox(
                        input,
                        sandbox_attempt,
                        &cancellation_token,
                    )
                    .await
            };

            let outcome =
                serde_json::to_value(&output.content).map_err(|e| {
                    ToolExecutionError::Permanent(format!(
                        "failed to serialize tool output: {}",
                        e
                    ))
                })?;

            Ok(ToolCallResult {
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
                outcome,
                is_error: output.is_error.unwrap_or(false),
                tool_id: request.tool_id,
            })
        }
    }

    /// Validate `arguments` against the tool's `parameters()` JSON schema.
    ///
    /// This is a sanity check, not a full JSON Schema validator: it only
    /// enforces that `arguments` is a JSON object, that every entry in
    /// `required` is present, and that present fields match the declared
    /// `type`. Unknown fields are ignored (serde default). Constraints like
    /// `minimum`, `maximum`, `pattern`, or `minLength` are intentionally
    /// not checked.
    fn validate_tool_input(
        arguments: &serde_json::Value,
        parameters: &serde_json::Value,
    ) -> Result<(), ToolExecutionError> {
        let args_obj = arguments.as_object().ok_or_else(|| {
            ToolExecutionError::Permanent(
                "Invalid input: expected JSON object".to_string(),
            )
        })?;

        if let Some(required) =
            parameters.get("required").and_then(|v| v.as_array())
        {
            for field in required {
                if let Some(field_name) = field.as_str()
                    && !args_obj.contains_key(field_name)
                {
                    return Err(ToolExecutionError::Permanent(format!(
                        "Invalid input: missing required field `{field_name}`"
                    )));
                }
            }
        }

        if let Some(properties) =
            parameters.get("properties").and_then(|v| v.as_object())
        {
            for (field, schema) in properties {
                let Some(value) = args_obj.get(field) else {
                    continue;
                };
                let Some(expected_type) =
                    schema.get("type").and_then(|v| v.as_str())
                else {
                    continue;
                };
                if !type_matches(expected_type, value) {
                    return Err(ToolExecutionError::Permanent(format!(
                        "Invalid input: field `{field}` expected type {expected_type}, got {}",
                        json_type_name(value)
                    )));
                }
            }
        }

        Ok(())
    }

    /// Return `true` if `value` matches the JSON Schema `type` keyword.
    ///
    /// Per the design spec, `integer` and `number` are both satisfied by
    /// any JSON number — a float like `1.5` is accepted for an `integer`
    /// field. This keeps the check a lenient sanity check rather than a
    /// strict schema validator.
    fn type_matches(expected: &str, value: &serde_json::Value) -> bool {
        match expected {
            "string" => value.is_string(),
            "integer" | "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => true,
        }
    }

    /// Human-readable name for a JSON value's type, for error messages.
    fn json_type_name(value: &serde_json::Value) -> &'static str {
        match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    }
}

/// Build a [`HashMapResolver`](crate::HashMapResolver) containing the default tools provided by
/// `synthia-tool`, wrapped via [`adapter::ToolAdapter`].
pub fn default_tool_resolver() -> crate::HashMapResolver {
    use std::sync::Arc;

    use synthia_tool::builtin::{
        ApplyPatchTool,
        GlobTool,
        GrepTool,
        MultiEditTool,
        ReadTool,
        WebFetchTool,
        WriteTool,
    };

    let mut tools: HashMap<String, Arc<dyn crate::ExecutableTool>> =
        HashMap::new();
    tools.insert(
        "read".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(ReadTool::new()))),
    );
    tools.insert(
        "write".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(WriteTool))),
    );
    tools.insert(
        "glob".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(GlobTool))),
    );
    tools.insert(
        "grep".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(GrepTool))),
    );
    tools.insert(
        "multi_edit".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(MultiEditTool))),
    );
    tools.insert(
        "apply_patch".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(
            ApplyPatchTool::default(),
        ))),
    );
    tools.insert(
        "web_fetch".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(WebFetchTool::new()))),
    );
    crate::HashMapResolver::new(tools)
}

/// Build a [`HashMapResolver`](crate::HashMapResolver) with the default tools, wiring a shared
/// [`FileMutationQueue`] into file-mutating tools (`write`, `multi_edit`)
/// so that concurrent writes to the same realpath are serialized.
///
/// All clones of the returned queue share the same underlying map, so
/// callers can clone the queue before calling this function and use the
/// other clone for inspection or to pass to additional tools.
pub fn default_tool_resolver_with_file_queue(
    file_queue: synthia_tool_exec_base::FileMutationQueue,
) -> crate::HashMapResolver {
    use std::sync::Arc;

    use synthia_tool::builtin::{
        ApplyPatchTool,
        GlobTool,
        GrepTool,
        MultiEditTool,
        ReadTool,
        WebFetchTool,
        WriteTool,
    };

    let mut tools: HashMap<String, Arc<dyn crate::ExecutableTool>> =
        HashMap::new();
    tools.insert(
        "read".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(ReadTool::new()))),
    );
    tools.insert(
        "write".to_string(),
        Arc::new(adapter::ToolAdapter::with_file_queue(
            Arc::new(WriteTool),
            file_queue.clone(),
        )),
    );
    tools.insert(
        "glob".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(GlobTool))),
    );
    tools.insert(
        "grep".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(GrepTool))),
    );
    tools.insert(
        "multi_edit".to_string(),
        Arc::new(adapter::ToolAdapter::with_file_queue(
            Arc::new(MultiEditTool),
            file_queue.clone(),
        )),
    );
    tools.insert(
        "apply_patch".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(
            ApplyPatchTool::default(),
        ))),
    );
    tools.insert(
        "web_fetch".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(WebFetchTool::new()))),
    );
    crate::HashMapResolver::new(tools)
}
