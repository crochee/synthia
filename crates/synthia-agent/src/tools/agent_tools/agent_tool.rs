//! The `task` tool (spawns a subagent with its own context).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_permission::{MergedPolicy, PermissionChecker};
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::{
    builtin_types::{
        BUILTIN_SUBAGENT_TYPES,
        builtin_denied_tool_rules,
        get_builtin_config,
    },
    team::SubagentManager,
};
use crate::{
    agent_instance::{AgentResult, AgentStatus, AgentTokenUsage},
    subagent::permission::derive_subagent_permission,
};

/// Build the JSON Schema for the `task` tool parameters.
///
/// `background` is only exposed when the runtime supports background
/// subagent execution.
fn task_parameters(background_available: bool) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    properties.insert(
        "description".to_string(),
        json!({"type": "string", "description": "A short (3-5 words) description of the task"}),
    );
    properties.insert(
        "prompt".to_string(),
        json!({"type": "string", "description": "The task for the agent to perform"}),
    );
    properties.insert(
        "subagent_type".to_string(),
        json!({"type": "string", "description": "The type of specialized agent to use for this task"}),
    );
    properties.insert(
        "task_id".to_string(),
        json!({"type": "string", "description": "Optional identifier used when running in background mode; if omitted, a UUID is generated."}),
    );
    if background_available {
        properties.insert(
            "background".to_string(),
            json!({"type": "boolean", "description": "Run the agent in the background. You will be notified when it completes."}),
        );
    }

    json!({
        "type": "object",
        "properties": properties,
        "required": ["description", "prompt", "subagent_type"]
    })
}

/// Build a human-readable description for the `task` tool that advertises
/// the built-in subagent types plus any custom agents currently registered
/// on the manager's coordinator.
fn build_description(manager: &SubagentManager) -> String {
    let mut types: Vec<String> = BUILTIN_SUBAGENT_TYPES
        .iter()
        .map(|s| format!("{}: {}", s, describe_builtin(s)))
        .collect();

    let custom: Vec<String> = manager
        .get_coordinator()
        .list_agents()
        .into_iter()
        .map(|agent| {
            format!(
                "{}: custom agent (capabilities: {:?})",
                agent.id, agent.capabilities
            )
        })
        .collect();
    types.extend(custom);

    let type_list = if types.is_empty() {
        "(none registered)".to_string()
    } else {
        types.join("; ")
    };

    format!(
        "Spawns a subagent with its own context window to handle a task. \
         Available subagent types: {type_list}"
    )
}

fn describe_builtin(name: &str) -> String {
    match get_builtin_config(name) {
        Some(cfg) => cfg.description.to_string(),
        None => "built-in subagent".to_string(),
    }
}

pub struct AgentTool {
    manager: Arc<SubagentManager>,
    background_available: bool,
    description: String,
}

impl AgentTool {
    pub fn new(
        manager: Arc<SubagentManager>,
        background_available: bool,
    ) -> Self {
        let description = build_description(&manager);
        Self {
            manager,
            background_available,
            description,
        }
    }
}

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str {
        "task"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        task_parameters(self.background_available)
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let description = input
            .input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let prompt = input
            .input
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let subagent_type =
            match input.input.get("subagent_type").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => return ToolOutput::error("subagent_type is required"),
            };
        let background = input
            .input
            .get("background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let task_id = input
            .input
            .get("task_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        if description.is_empty() || prompt.is_empty() {
            return ToolOutput::error(
                "description and prompt parameters are required",
            );
        }

        if background && !self.background_available {
            return ToolOutput::error(
                "Background execution is not available in this context",
            );
        }

        // Depth limit check
        let current_depth = self.manager.current_depth();
        let max_depth = self.manager.max_depth();
        if current_depth >= max_depth {
            return ToolOutput::error(format!(
                "Max sub-agent depth reached ({}). Cannot spawn nested sub-agent.",
                max_depth
            ));
        }

        // Concurrency limit check. The guard owns the slot; dropping it
        // (implicitly, on any early return) releases the slot back to
        // the manager. For the foreground success path, `commit()`
        // prevents drop from releasing so we can manage the slot
        // explicitly across the `.await`.
        let guard = match self.manager.try_acquire_slot() {
            Some(g) => g,
            None => {
                return ToolOutput::error(format!(
                    "Max concurrent sub-agents reached ({}). Try again later or wait for existing sub-agents to complete.",
                    self.manager.max_concurrent()
                ));
            }
        };

        // Get parent config (required for subagent execution)
        let mut parent_config = match self.manager.parent_config() {
            Some(cfg) => cfg,
            None => {
                return ToolOutput::error(
                    "Sub-agent execution requires parent config to be set on SubagentManager",
                );
            }
        };

        // Resolve the requested subagent type and apply its permission
        // set to the child's tool registry. Built-in types contribute
        // explicit denied-tool rules; all types default-deny `task` and
        // `todowrite` unless their configuration opts out.
        let (allow_task, allow_todowrite) = get_builtin_config(subagent_type)
            .map(|cfg| (cfg.allow_task, cfg.allow_todowrite))
            .unwrap_or((false, false));
        let mut subagent_rules = builtin_denied_tool_rules(subagent_type);
        subagent_rules.extend(derive_subagent_permission(
            &[],
            allow_task,
            allow_todowrite,
        ));
        if !subagent_rules.is_empty() {
            let workspace_root = &parent_config.config.workspace_root;
            let checker = PermissionChecker::new(MergedPolicy::new(
                &[],
                &subagent_rules,
                &[],
            ))
            .with_workspace_root(workspace_root);
            parent_config.tool_registry =
                parent_config.tool_registry.with_checker(checker);
        }

        let factory = match parent_config.subagent_session_factory {
            Some(factory) => factory,
            None => {
                return ToolOutput::error(
                    "Sub-agent execution requires a subagent session factory in AgentRunConfig",
                );
            }
        };

        let agent_control = parent_config.agent_control.clone();
        let user_id = parent_config.user_id.clone();
        let parent_session_id = parent_config.session_id.clone();
        let full_prompt =
            format!("[{}] {}\n\n{}", subagent_type, description, prompt);

        // Generate the child session id up front and register it with
        // the manager for recursive subtree cancellation
        // (spec: `subagent-tree-cancellation`). The child's cancel token
        // is derived from the parent's via `child_token()`, so canceling
        // the parent's shared token still propagates to all children
        // (existing behavior), while `cancel_session_tree` can cancel
        // this subtree in isolation.
        let child_session_id = uuid::Uuid::new_v4().to_string();
        let child_cancel_token = parent_config.cancel_token.child_token();
        self.manager.register_child_session(
            parent_session_id.clone(),
            child_session_id.clone(),
            child_cancel_token,
        );

        if background {
            // Background mode: spawn and return immediately. The handle
            // is registered with AgentControl for later tracking and
            // notification injection into the parent loop.
            let control = match agent_control {
                Some(control) => control,
                None => {
                    // Roll back the registration so the manager does not
                    // accumulate entries for sessions that never run.
                    self.manager.remove_session(&child_session_id);
                    return ToolOutput::error(
                        "Background sub-agent execution requires AgentControl",
                    );
                }
            };

            let instance_id =
                task_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let parent_depth = self.manager.current_depth();
            let manager_clone = Arc::clone(&self.manager);
            let child_id_for_cleanup = child_session_id.clone();
            let handle = tokio::spawn(async move {
                let result = factory
                    .run_child(
                        user_id,
                        parent_session_id,
                        full_prompt,
                        parent_depth,
                        Some(child_id_for_cleanup.clone()),
                    )
                    .await
                    .unwrap_or_else(|e| AgentResult {
                        output: e.to_string(),
                        status: AgentStatus::Errored,
                        token_usage: AgentTokenUsage {
                            input_tokens: 0,
                            output_tokens: 0,
                        },
                    });
                // Clean up the registration now that the child has
                // finished. This keeps `child_sessions` from growing
                // unboundedly as new background subagents spawn.
                manager_clone.remove_session(&child_id_for_cleanup);
                result
            });

            control.register_background_task(instance_id.clone(), handle);

            // Release the slot for background tasks (they run
            // independently). Dropping the guard calls release_slot()
            // automatically — no commit() needed here.
            drop(guard);

            ToolOutput::text(format!(
                "Sub-agent spawned in background (id: {}). Task: {}",
                instance_id, description
            ))
        } else {
            // Foreground mode: create a real child session, enqueue the
            // prompt, and wait for the child agent to complete.
            //
            // If `task_id` is provided, it is intended to resume an
            // existing session; the actual resume path is wired in later
            // tasks once `SubagentSessionFactory` supports it.
            //
            // Commit the guard so Drop does NOT release the slot. The
            // guard is consumed here (no SlotGuard crosses the await),
            // and we explicitly release the slot after the child run
            // completes.
            guard.commit();

            let result = factory
                .run_child(
                    user_id,
                    parent_session_id,
                    full_prompt,
                    self.manager.current_depth(),
                    Some(child_session_id.clone()),
                )
                .await;

            // Manual release after await; if await panics/cancels, slot
            // leaks (accepted trade-off per SlotGuard design).
            self.manager.release_slot();
            // Clean up the registration now that the child has finished.
            self.manager.remove_session(&child_session_id);

            match result {
                Ok(result) => match result.status {
                    AgentStatus::Completed => ToolOutput::text(format!(
                        "Sub-agent completed.\n\n{}",
                        result.output
                    )),
                    AgentStatus::Errored => ToolOutput::error(format!(
                        "Sub-agent error: {}",
                        result.output
                    )),
                    AgentStatus::Cancelled => {
                        ToolOutput::error("Sub-agent was cancelled".to_string())
                    }
                    _ => ToolOutput::error("Sub-agent ended unexpectedly"),
                },
                Err(e) => ToolOutput::error(format!(
                    "Sub-agent session failed: {}",
                    e
                )),
            }
        }
    }
}
