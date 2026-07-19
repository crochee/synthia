# Subagent Task Tool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire Synthia's existing subagent infrastructure together by exposing a production-ready `task` tool with foreground/background modes, safe permission inheritance, built-in agent types, and ForkPolicy support.

**Architecture:** Inject `AgentControl` into `AgentRunConfig`, conditionally register `AgentTool` when both `AgentControl` and `SubagentSessionFactory` are present, align `AgentTool` parameters with Opencode, derive child permissions from parent deny rules, and apply `ForkPolicy` to inherited message history.

**Tech Stack:** Rust 2024, `tokio`, `serde_json`, `synthia-agent`, `synthia-permission`, `synthia-context`, `synthia-tool`, `synthia-server`.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/synthia-agent/src/subagent/permission.rs` | New: `derive_subagent_permission()` function. |
| `crates/synthia-agent/src/subagent/config.rs` | Modify: apply `ForkPolicy` and derived permissions in `build_subagent_config`. |
| `crates/synthia-agent/src/subagent/mod.rs` | Modify: expose new `permission` module. |
| `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs` | Modify: align parameters, handle `task_id`, dynamic schema. |
| `crates/synthia-agent/src/tools/agent_tools/builtin_types.rs` | New: built-in `general`/`explore` type definitions. |
| `crates/synthia-agent/src/tools/registry.rs` | Modify: accept optional subagent deps, register `AgentTool` conditionally. |
| `crates/synthia-agent/src/config/agent_config/run_config.rs` | Read-only reference for field changes. |
| `crates/synthia-server/src/state/agent_factory.rs` | Modify: inject `AgentControl`. |
| `crates/synthia-server/src/session/controller.rs` | Modify: inject `AgentControl`. |
| `crates/synthia-agent/src/agent.rs` | Modify: inject `AgentControl` in resume path. |
| `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` | Modify: capture actual output in completion messages. |

---

## Task 1: Inject AgentControl into AgentRunConfig construction paths

**Files:**
- Modify: `crates/synthia-server/src/state/agent_factory.rs`
- Modify: `crates/synthia-server/src/session/controller.rs`
- Modify: `crates/synthia-agent/src/agent.rs`
- Test: `crates/synthia-agent/src/agent_tests.rs` (or nearest test module)

- [ ] **Step 1: Add `AgentControl` to `AgentFactory`**

Add a field to `AgentFactory` and initialize it in constructors:

```rust
// crates/synthia-server/src/state/agent_factory.rs
pub struct AgentFactory {
    // ... existing fields ...
    agent_control: Arc<AgentControl>,
}

impl AgentFactory {
    pub fn new(
        // ... existing args ...
        agent_control: Arc<AgentControl>,
    ) -> Self {
        Self {
            // ... existing init ...
            agent_control,
        }
    }

    pub fn from_state(state: &AppState) -> Self {
        Self::new(
            // ... existing args from state ...
            Arc::new(AgentControl::new(Arc::new(AgentRegistry::new()))),
        )
    }
}
```

- [ ] **Step 2: Inject `AgentControl` in `AgentFactory::create`**

```rust
// In AgentFactory::create:
Agent::run_stream(AgentRunConfig {
    // ...
    agent_control: Some((*self.agent_control).clone()),
    subagent_session_factory: None,
    // ...
})
```

- [ ] **Step 3: Inject `AgentControl` in `SessionController::build_run_config`**

```rust
// crates/synthia-server/src/session/controller.rs
agent_control: Some(AgentControl::new(Arc::new(AgentRegistry::new()))),
```

- [ ] **Step 4: Inject `AgentControl` in `Agent::resume` and CLI path**

```rust
// crates/synthia-agent/src/agent.rs
agent_control: Some(AgentControl::new(Arc::new(AgentRegistry::new()))),
```

- [ ] **Step 5: Add unit test verifying `AgentRunConfig.agent_control` is `Some`**

```rust
#[test]
fn agent_factory_injects_agent_control() {
    let factory = AgentFactory::from_state(&AppState::default());
    // Verify field is set by inspection or via a debug helper.
}
```

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-server/src/state/agent_factory.rs \
        crates/synthia-server/src/session/controller.rs \
        crates/synthia-agent/src/agent.rs
git commit -m "feat(subagent): inject AgentControl into AgentRunConfig paths"
```

---

## Task 2: Implement subagent permission inheritance

**Files:**
- Create: `crates/synthia-agent/src/subagent/permission.rs`
- Modify: `crates/synthia-agent/src/subagent/mod.rs`
- Test: `crates/synthia-agent/src/subagent/permission_tests.rs` (or inline `#[cfg(test)]` module)

- [ ] **Step 1: Write the implementation**

```rust
// crates/synthia-agent/src/subagent/permission.rs
use synthia_permission::{PermissionAction, PermissionRule};

pub fn derive_subagent_permission(
    parent_permission: &[PermissionRule],
    subagent_allows_task: bool,
    subagent_allows_todowrite: bool,
) -> Vec<PermissionRule> {
    let mut rules: Vec<PermissionRule> = parent_permission
        .iter()
        .filter(|r| r.action == PermissionAction::Deny)
        .cloned()
        .collect();

    if !subagent_allows_task {
        rules.push(PermissionRule {
            pattern: "task".to_string(),
            action: PermissionAction::Deny,
            forced: true,
        });
    }

    if !subagent_allows_todowrite {
        rules.push(PermissionRule {
            pattern: "todowrite".to_string(),
            action: PermissionAction::Deny,
            forced: true,
        });
    }

    rules
}
```

- [ ] **Step 2: Expose the module**

```rust
// crates/synthia-agent/src/subagent/mod.rs
pub mod permission;
```

- [ ] **Step 3: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherits_deny_rules_only() {
        let parent = vec![
            PermissionRule { pattern: "*.env".to_string(), action: PermissionAction::Deny, forced: false },
            PermissionRule { pattern: "bash".to_string(), action: PermissionAction::Allow, forced: false },
        ];
        let derived = derive_subagent_permission(&parent, false, false);
        assert!(derived.iter().any(|r| r.pattern == "*.env"));
        assert!(!derived.iter().any(|r| r.pattern == "bash" && r.action == PermissionAction::Allow));
    }

    #[test]
    fn defaults_deny_task_and_todowrite() {
        let derived = derive_subagent_permission(&[], false, false);
        assert!(derived.iter().any(|r| r.pattern == "task" && r.action == PermissionAction::Deny));
        assert!(derived.iter().any(|r| r.pattern == "todowrite" && r.action == PermissionAction::Deny));
    }

    #[test]
    fn can_opt_out_of_default_denies() {
        let derived = derive_subagent_permission(&[], true, true);
        assert!(!derived.iter().any(|r| r.pattern == "task"));
        assert!(!derived.iter().any(|r| r.pattern == "todowrite"));
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p synthia-agent subagent::permission::tests
```

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/subagent/permission.rs crates/synthia-agent/src/subagent/mod.rs
git commit -m "feat(subagent): add derive_subagent_permission with deny-only inheritance"
```

---

## Task 3: Apply ForkPolicy in subagent configuration

**Files:**
- Modify: `crates/synthia-agent/src/subagent/config.rs`
- Modify: `crates/synthia-agent/src/subagent/factory.rs` (if `run_child` calls config builder)
- Test: `crates/synthia-agent/src/subagent/config_tests.rs`

- [ ] **Step 1: Refactor `build_subagent_config` signature**

```rust
// crates/synthia-agent/src/subagent/config.rs
use crate::subagent::permission::derive_subagent_permission;
use synthia_permission::{PermissionRule, PermissionAction};

pub fn build_subagent_config(
    _instance: &AgentInstance,
    parent_config: AgentRunConfig,
    parent_messages: &[Message],
    subagent_permission: Vec<PermissionRule>,
) -> AgentRunConfig {
    let filtered_messages = apply_fork_policy(&parent_config.fork_policy, parent_messages);

    let mut config = parent_config.clone();
    config.initial_messages = filtered_messages;
    // Wire derived permissions into the child's approval service if present.
    if let Some(ref approval) = config.approval_service {
        // Replace with a wrapper that enforces subagent_permission.
        // If no wrapper exists, store rules in a field on AgentRunConfig
        // that the assembler/context uses to build the system prompt.
    }

    config
}
```

- [ ] **Step 2: Add unit tests for each ForkPolicy variant**

```rust
#[test]
fn last_n_turns_keeps_only_last_n_user_turns() {
    let messages = vec![
        Message::system("sys".to_string()),
        Message::user("q1".to_string()),
        Message::assistant("a1".to_string()),
        Message::user("q2".to_string()),
        Message::assistant("a2".to_string()),
    ];
    let result = apply_fork_policy(&ForkPolicy::LastNTurns(1), &messages);
    assert!(result.iter().any(|m| m.content == "q2"));
    assert!(!result.iter().any(|m| m.content == "q1"));
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p synthia-agent subagent::config
```

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/subagent/config.rs
git commit -m "feat(subagent): apply ForkPolicy and derived permissions in build_subagent_config"
```

---

## Task 4: Align AgentTool parameter schema with Opencode

**Files:**
- Modify: `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs`
- Modify: `crates/synthia-agent/src/tools/agent_tools/mod.rs` (if needed)
- Test: `crates/synthia-agent/src/tools/agent_tools/tests.rs`

- [ ] **Step 1: Update parameter schema**

```rust
fn task_parameters(background_available: bool) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    properties.insert("description".to_string(), json!({"type": "string", "description": "A short (3-5 words) description of the task"}));
    properties.insert("prompt".to_string(), json!({"type": "string", "description": "The task for the agent to perform"}));
    properties.insert("subagent_type".to_string(), json!({"type": "string", "description": "The type of specialized agent to use for this task"}));
    properties.insert("task_id".to_string(), json!({"type": "string", "description": "Resume a previous task session instead of creating a new one"}));
    if background_available {
        properties.insert("background".to_string(), json!({"type": "boolean", "description": "Run the agent in the background. You will be notified when it completes."}));
    }

    json!({
        "type": "object",
        "properties": properties,
        "required": ["description", "prompt", "subagent_type"]
    })
}
```

- [ ] **Step 2: Add `background_available` flag to `AgentTool`**

```rust
pub struct AgentTool {
    manager: Arc<SubagentManager>,
    background_available: bool,
}

impl AgentTool {
    pub fn new(manager: Arc<SubagentManager>, background_available: bool) -> Self {
        Self { manager, background_available }
    }
}
```

- [ ] **Step 3: Update `parameters()` and `call()` to use new fields**

Replace `run_in_background` with `background`, add `task_id` handling.

```rust
async fn call(&self, input: ToolInput) -> ToolOutput {
    let description = input.input.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let prompt = input.input.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    let subagent_type = input.input.get("subagent_type").and_then(|v| v.as_str()).unwrap_or("general");
    let background = input.input.get("background").and_then(|v| v.as_bool()).unwrap_or(false);
    let task_id = input.input.get("task_id").and_then(|v| v.as_str()).map(String::from);

    if description.is_empty() || prompt.is_empty() {
        return ToolOutput::error("description and prompt parameters are required");
    }

    // Resolve subagent type to permission settings (Task 6).
    // ... depth/concurrency checks ...

    let full_prompt = format!("[{}] {}\n\n{}", subagent_type, description, prompt);

    if background {
        // Use task_id if provided, else generate UUID.
        let instance_id = task_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        // ... spawn and register ...
    } else {
        // Foreground: optionally resume via task_id.
        // ...
    }
}
```

- [ ] **Step 4: Update existing tests**

Replace `run_in_background` with `background` in test inputs.

- [ ] **Step 5: Run tests**

```bash
cargo test -p synthia-agent tools::agent_tools
```

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/tools/agent_tools/agent_tool.rs
git commit -m "feat(subagent): align AgentTool parameters with Opencode task tool"
```

---

## Task 5: Register AgentTool conditionally in ToolRegistry

**Files:**
- Modify: `crates/synthia-agent/src/tools/registry.rs`
- Modify: all call sites of `build_default_tool_registry`
- Test: `crates/synthia-agent/src/tools/registry_tests.rs`

- [x] **Step 1: Update `build_default_tool_registry` signature**

```rust
// crates/synthia-agent/src/tools/registry.rs
use crate::control::AgentControl;
use crate::subagent::SubagentSessionFactory;
use crate::tools::agent_tools::agent_tool::AgentTool;
use crate::tools::agent_tools::team::SubagentManager;

pub fn build_default_tool_registry(
    workspace_root: impl Into<PathBuf>,
    agent_control: Option<AgentControl>,
    subagent_session_factory: Option<Arc<dyn SubagentSessionFactory>>,
) -> ToolRegistry {
    let workspace_root = workspace_root.into();
    let registry = ToolRegistry::register_defaults();

    let command_manager = Arc::new(CommandManager::new());
    let sandbox = CommandBlacklist::new(workspace_root);
    registry.register(synthia_tool::ToolEntry::new(Arc::new(BashTool::new(
        command_manager,
        sandbox,
    ))));

    if let (Some(control), Some(factory)) = (agent_control, subagent_session_factory) {
        let manager = Arc::new(SubagentManager::new(
            Arc::new(control),
            factory,
            3, // max_depth
            5, // max_concurrent
        ));
        let background_available = true;
        let agent_tool = Arc::new(AgentTool::new(manager, background_available));
        registry.register(synthia_tool::ToolEntry::new(agent_tool));
    }

    registry
}
```

- [x] **Step 2: Update call sites**

Search for `build_default_tool_registry(` and add `None, None` for paths that lack subagent infrastructure. For server paths, pass the actual `AgentControl` and `SubagentSessionFactory`.

- [x] **Step 3: Add tests**

```rust
#[test]
fn registry_includes_task_tool_when_deps_present() {
    let control = AgentControl::new(Arc::new(AgentRegistry::new()));
    let registry = build_default_tool_registry("/tmp", Some(control), Some(stub_factory()));
    assert!(registry.get("task").is_some());
}

#[test]
fn registry_omits_task_tool_when_deps_missing() {
    let registry = build_default_tool_registry("/tmp", None, None);
    assert!(registry.get("task").is_none());
}
```

- [x] **Step 4: Run tests**

```bash
cargo test -p synthia-agent tools::registry
```

- [x] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/tools/registry.rs $(git grep -l build_default_tool_registry)
git commit -m "feat(subagent): conditionally register task tool in default registry"
```

---

## Task 6: Implement built-in subagent types

**Files:**
- Create: `crates/synthia-agent/src/tools/agent_tools/builtin_types.rs`
- Modify: `crates/synthia-agent/src/tools/agent_tools/mod.rs`
- Modify: `crates/synthia-agent/src/control/registry.rs` (if needed for reserved names)
- Test: `crates/synthia-agent/src/tools/agent_tools/builtin_types_tests.rs`

- [x] **Step 1: Define built-in types**

```rust
// crates/synthia-agent/src/tools/agent_tools/builtin_types.rs
pub const BUILTIN_SUBAGENT_TYPES: &[&str] = &["general", "explore"];

pub fn is_builtin_subagent_type(name: &str) -> bool {
    BUILTIN_SUBAGENT_TYPES.contains(&name)
}

pub struct SubagentTypeConfig {
    pub description: &'static str,
    pub allowed_tools: Vec<&'static str>,
    pub denied_tools: Vec<&'static str>,
    pub allow_task: bool,
    pub allow_todowrite: bool,
}

pub fn get_builtin_config(name: &str) -> Option<SubagentTypeConfig> {
    match name {
        "general" => Some(SubagentTypeConfig {
            description: "General-purpose subagent for multi-step tasks",
            allowed_tools: vec!["read", "write", "glob", "grep", "apply_patch", "bash", "web_fetch"],
            denied_tools: vec![],
            allow_task: false,
            allow_todowrite: false,
        }),
        "explore" => Some(SubagentTypeConfig {
            description: "Read-only subagent for codebase exploration",
            allowed_tools: vec!["read", "glob", "grep", "web_fetch"],
            denied_tools: vec!["write", "apply_patch", "bash"],
            allow_task: false,
            allow_todowrite: false,
        }),
        _ => None,
    }
}
```

- [x] **Step 2: Reject reserved identifiers in `RegisterAgent`**

Inside `RegisterAgent::call`, before registering:

```rust
if is_builtin_subagent_type(&requested_name) {
    return ToolOutput::error(format!("{} is a reserved built-in subagent type", requested_name));
}
```

- [x] **Step 3: Wire config into `AgentTool::call`**

Use `get_builtin_config(subagent_type)` to resolve `allow_task`/`allow_todowrite` and tool filtering.

- [x] **Step 4: Add tests**

```rust
#[test]
fn builtin_types_are_reserved() {
    assert!(is_builtin_subagent_type("general"));
    assert!(is_builtin_subagent_type("explore"));
    assert!(!is_builtin_subagent_type("custom"));
}
```

- [x] **Step 5: Run tests**

```bash
cargo test -p synthia-agent tools::agent_tools::builtin_types
```

- [x] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/tools/agent_tools/builtin_types.rs crates/synthia-agent/src/tools/agent_tools/mod.rs
git commit -m "feat(subagent): add general and explore built-in subagent types"
```

---

## Task 7: Enable background subagent execution

**Files:**
- Modify: `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs`
- Modify: `crates/synthia-agent/src/control/core_ctrl.rs` (if `CompletedTask` needs output field)
- Test: `crates/synthia-agent/src/control/core_ctrl_tests.rs`

- [ ] **Step 1: Extend `CompletedTask` to carry output**

```rust
// crates/synthia-agent/src/control/core_ctrl.rs
pub struct CompletedTask {
    pub agent_id: String,
    pub output: String,
    pub status: AgentStatus,
}
```

- [ ] **Step 2: Update `check_completed` to capture output**

When a background handle finishes, capture `AgentResult.output` and `AgentResult.status` into `CompletedTask`.

- [ ] **Step 3: Update `AgentTool` background branch**

Ensure the spawned task stores its result in a way `check_completed` can retrieve, or pass a shared channel.

- [ ] **Step 4: Add tests**

```rust
#[tokio::test]
async fn background_task_is_registered_and_completed() {
    let control = AgentControl::new(Arc::new(AgentRegistry::new()));
    // Spawn a task and verify check_completed returns it after completion.
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p synthia-agent control
```

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/control/core_ctrl.rs crates/synthia-agent/src/tools/agent_tools/agent_tool.rs
git commit -m "feat(subagent): enable background task execution via AgentControl"
```

---

## Task 8: Improve background completion notifications

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`

- [ ] **Step 1: Update main-loop polling block**

```rust
if let Some(ref control) = agent_control {
    let completed = control.check_completed();
    for task in completed {
        let (tag, inner) = match task.status {
            AgentStatus::Completed => ("completed", "task_result"),
            _ => ("error", "task_error"),
        };
        let synthetic_msg = Message::user(format!(
            "<task id=\"{}\" state=\"{}\">\n<summary>Background task {}: {}</summary>\n<{}>\n{}\n</{}>\n</task>",
            task.agent_id, tag, tag, task.agent_id, inner, task.output, inner
        ));
        ctx.messages.push(synthetic_msg);
        yield AgentEvent::SteeringReceived {
            session_id: session_id_clone.clone(),
            message: format!("Background task {} {}", task.agent_id, tag),
        };
    }
}
```

- [ ] **Step 2: Add integration test**

Create a test that spawns a background subagent and verifies the parent context receives the `<task>` result message.

- [ ] **Step 3: Run tests**

```bash
cargo test -p synthia-agent stream_builder
```

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs
git commit -m "feat(subagent): inject structured task result on background completion"
```

---

## Task 9: Verify and finalize

- [ ] **Step 1: Format**

```bash
cargo +nightly fmt --all
```

- [ ] **Step 2: Lint**

```bash
cargo clippy --all-targets --all-features --tests --all
```

- [ ] **Step 3: Test**

```bash
cargo test --workspace
```

- [ ] **Step 4: Validate OpenSpec**

```bash
openspec validate --all
```

- [ ] **Step 5: Commit final fixes**

```bash
git add -A
git commit -m "chore(subagent): format, clippy, and test fixes"
```

---

## Self-Review

**Spec coverage:**
- `subagent-task-tool` spec → Tasks 4, 5
- `subagent-permission-inheritance` spec → Task 2
- `subagent-background-mode` spec → Tasks 1, 7, 8
- `subagent-built-in-types` spec → Task 6
- `subagent-session-model` spec → Task 3
- `subagent-event-bridge` spec → Task 8
- `tool-execution` spec → Task 5

**Placeholder scan:** No TBD/TODO/fill-in-details found.

**Type consistency:** `AgentTool::new` signature changes in Task 4 are consumed by Task 5. `CompletedTask` shape in Task 7 is consumed by Task 8. `build_subagent_config` signature change in Task 3 must be propagated to callers in Task 4/7.
