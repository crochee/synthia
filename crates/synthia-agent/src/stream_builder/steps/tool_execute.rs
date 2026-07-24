use std::{path::PathBuf, sync::Arc};

use synthia_core::Error;
use synthia_guardian::{GuardianCoordinator, GuardianSubagentFactory};
use synthia_permission::Permission;
use synthia_tool::{registry::ToolRegistry, types::ToolExecutionContext};
use synthia_tool_orchestrator::{
    ExecutionContext,
    ToolCallRequest,
    ToolOrchestrator,
    ToolOrchestratorError,
    ToolOrchestratorEvent,
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::AgentRunConfig,
    loop_context::LoopContext,
    subagent::GuardianSubagentFactoryBridge,
    types::ToolResult,
};

pub struct StepToolExecute {
    tool_registry: Arc<ToolRegistry>,
    workspace_root: PathBuf,
    session_id: String,
    tool_orchestrator: Option<Arc<dyn ToolOrchestrator>>,
    cancel_token: CancellationToken,
    /// Optional Guardian coordinator. When `Some`,
    /// [`execute_and_emit`](crate::stream_builder::builder::tool_execution::execute::execute_and_emit)
    /// runs `GuardianCoordinator::check` before delegating to
    /// [`execute`](Self::execute). `None` = Guardian disabled (legacy).
    guardian_coordinator: Option<Arc<GuardianCoordinator>>,
    /// Bridge wrapping `subagent_session_factory` as a
    /// [`GuardianSubagentFactory`]. Passed to
    /// [`GuardianCoordinator::check`] as the per-call escalation gate.
    /// `None` when no `subagent_session_factory` is configured.
    subagent_factory: Option<Arc<dyn GuardianSubagentFactory>>,
}

impl StepToolExecute {
    pub fn new(config: &AgentRunConfig) -> Self {
        let subagent_factory =
            config.subagent_session_factory.as_ref().map(|f| {
                Arc::new(GuardianSubagentFactoryBridge::new(f.clone()))
                    as Arc<dyn GuardianSubagentFactory>
            });
        Self {
            tool_registry: Arc::new(config.tool_registry.clone()),
            workspace_root: config.config.workspace_root.clone(),
            session_id: config.session_id.clone(),
            tool_orchestrator: config.tool_orchestrator.clone(),
            cancel_token: config.cancel_token.clone(),
            guardian_coordinator: config.guardian_coordinator.clone(),
            subagent_factory,
        }
    }

    pub async fn execute(
        &self,
        ctx: &LoopContext,
        tool_calls: Vec<synthia_provider::types::ToolUse>,
    ) -> Result<Vec<ToolResult>, Error> {
        if let Some(orchestrator) = &self.tool_orchestrator {
            self.execute_via_orchestrator(ctx, tool_calls, orchestrator)
                .await
        } else {
            self.execute_via_registry(ctx, tool_calls).await
        }
    }

    /// Returns the Guardian coordinator, if configured.
    pub(crate) fn guardian_coordinator(
        &self,
    ) -> Option<&Arc<GuardianCoordinator>> {
        self.guardian_coordinator.as_ref()
    }

    /// Returns `true` when a `ToolOrchestrator` is configured.
    ///
    /// The Guardian permission gate uses this to decide whether a
    /// `NeedUserConfirm` decision can be forwarded to an approval
    /// service. When `false`, the gate must deny the call rather than
    /// silently downgrading to execution (P6 — Distrust by Default).
    pub fn has_orchestrator(&self) -> bool {
        self.tool_orchestrator.is_some()
    }

    /// Best-effort fail all currently active tool calls and return the
    /// `(tool_name, call_id)` pairs that were actually interrupted.
    ///
    /// Invoked by the main loop when cancellation (or steering
    /// interruption) is detected so that in-flight tool calls are
    /// cancelled and downstream observers (session JSONL,
    /// `ctx.recent_tool_results`) see a terminal `ToolCallCompleted`
    /// event for each interrupted call rather than leaving them
    /// dangling.
    ///
    /// Returns an empty `Vec` when no orchestrator is configured or no
    /// active calls were interrupted. The returned `Vec` is suitable
    /// for emitting `AgentEvent::ToolCallCompleted` per entry.
    pub fn fail_interrupted_tools(&self) -> Vec<(String, String)> {
        let Some(orchestrator) = &self.tool_orchestrator else {
            return Vec::new();
        };
        // Subscribe BEFORE calling `fail_interrupted_tools` so the
        // receiver captures every `Failed` event emitted by the call.
        // `broadcast::Sender::send` only delivers to receivers that
        // existed at send time.
        let mut rx = orchestrator.event_stream();
        let _count = orchestrator.fail_interrupted_tools();
        let mut interrupted: Vec<(String, String)> = Vec::new();
        // Drain the receiver. `try_recv` returns `Err(Empty)` once all
        // buffered events have been consumed.
        while let Ok(event) = rx.try_recv() {
            if let ToolOrchestratorEvent::Failed {
                call_id, tool_name, ..
            } = event
            {
                interrupted.push((tool_name, call_id));
            }
        }
        interrupted
    }

    /// Returns the Guardian subagent factory bridge, if configured.
    /// Passed to [`GuardianCoordinator::check`] as the per-call
    /// escalation gate.
    pub(crate) fn subagent_factory(
        &self,
    ) -> Option<&dyn GuardianSubagentFactory> {
        self.subagent_factory.as_deref()
    }

    async fn execute_via_orchestrator(
        &self,
        ctx: &LoopContext,
        tool_calls: Vec<synthia_provider::types::ToolUse>,
        orchestrator: &Arc<dyn ToolOrchestrator>,
    ) -> Result<Vec<ToolResult>, Error> {
        let execution_context = ExecutionContext {
            session_id: self.session_id.clone(),
            workspace_root: self.workspace_root.clone(),
            caller_agent: "synthia-agent".to_string(),
        };

        let mut results = Vec::with_capacity(tool_calls.len());
        for call in tool_calls {
            let request = ToolCallRequest {
                call_id: call.id.clone(),
                tool_name: call.name.clone(),
                arguments: call.input.clone(),
                permission: default_permission_for_tool(&call.name),
            };

            match orchestrator
                .execute(
                    request,
                    execution_context.clone(),
                    self.cancel_token.child_token(),
                )
                .await
            {
                Ok(result) => results.push(ToolResult {
                    tool_name: result.tool_name,
                    tool_call_id: result.call_id,
                    output: format_result_outcome(&result.outcome),
                    is_error: result.is_error,
                }),
                Err(ToolOrchestratorError::NotFound { .. }) => {
                    let registry_results = self
                        .run_registry_calls(ctx, vec![call.clone()])
                        .await?;
                    if let Some(result) = registry_results.into_iter().next() {
                        results.push(result);
                    } else {
                        results.push(ToolResult {
                            tool_name: call.name.clone(),
                            tool_call_id: call.id.clone(),
                            output: format!("Tool '{}' not found", call.name),
                            is_error: true,
                        });
                    }
                }
                Err(ToolOrchestratorError::EditConflict {
                    call_id: _,
                    path,
                    original_content_hash,
                    current_content_hash,
                }) => {
                    return Err(synthia_core::Error::EditConflict {
                        path,
                        original_hash: original_content_hash,
                        current_hash: current_content_hash,
                    });
                }
                Err(err) => results.push(ToolResult {
                    tool_name: call.name,
                    tool_call_id: call.id,
                    output: err.to_string(),
                    is_error: true,
                }),
            }
        }

        Ok(results)
    }

    async fn execute_via_registry(
        &self,
        ctx: &LoopContext,
        tool_calls: Vec<synthia_provider::types::ToolUse>,
    ) -> Result<Vec<ToolResult>, Error> {
        self.run_registry_calls(ctx, tool_calls).await
    }

    async fn run_registry_calls(
        &self,
        ctx: &LoopContext,
        tool_calls: Vec<synthia_provider::types::ToolUse>,
    ) -> Result<Vec<ToolResult>, Error> {
        let context = ToolExecutionContext::new(
            ctx.session_id.clone(),
            self.workspace_root.clone(),
        )
        .with_messages(ctx.messages.clone());

        let outputs = self
            .tool_registry
            .run_with_context(tool_calls.clone(), context)
            .await?;
        Ok(tool_calls
            .into_iter()
            .zip(outputs)
            .map(|(call, o)| ToolResult {
                tool_name: call.name,
                tool_call_id: call.id,
                output: o
                    .content
                    .iter()
                    .filter_map(|p| p.text())
                    .collect::<Vec<_>>()
                    .join("\n"),
                is_error: o.is_error.unwrap_or(false),
            })
            .collect())
    }
}

/// Default permission heuristic for tools executed through the orchestrator.
///
/// Tools that mutate the filesystem or spawn subprocesses require explicit
/// confirmation by default. Read-only / search tools are auto-approved.
fn default_permission_for_tool(tool_name: &str) -> Permission {
    match tool_name {
        "bash" | "write" | "apply_patch" | "multi_edit" => {
            Permission::RequireConfirm
        }
        _ => Permission::AutoApprove,
    }
}

/// Format a [`serde_json::Value`] tool outcome into a compact string.
fn format_result_outcome(outcome: &serde_json::Value) -> String {
    match outcome {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}
