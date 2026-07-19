## Why

Synthia already implements most of the subagent infrastructure — `AgentControl`, `AgentTool`, `SubagentSessionFactory`, and a family of agent lifecycle tools — but these layers are not wired together. As a result, the LLM cannot currently delegate work to a subagent, cannot run subagents in the background, and cannot safely inherit parent permissions. This is the largest functional gap relative to production agents such as Opencode, whose `task` tool is a core primitive for decomposing complex work. Closing this gap enables multi-step planning, parallel research, and safer scoped execution.

## What Changes

**Agent control plane injection**
- From: `AgentRunConfig.agent_control` is always `None`.
- To: `AgentControl` is injected in every `AgentRunConfig` construction path that has access to a shared runtime.
- Reason: Background task tracking and agent registry require a live control plane.
- Impact: Server-managed sessions gain subagent delegation; standalone paths remain unchanged.

**`task` tool registration**
- From: `AgentTool` is implemented but never registered; the LLM cannot invoke it.
- To: `build_default_tool_registry` conditionally registers `AgentTool` when both `AgentControl` and `SubagentSessionFactory` are present.
- Reason: The tool is only usable when both the control plane and the session factory are available.
- Impact: Server-managed sessions expose the `task` tool.

**Opencode-aligned parameter schema**
- From: `AgentTool` uses `agent_id` and `run_in_background`.
- To: `AgentTool` uses `description`, `prompt`, `subagent_type`, `background`, and `task_id`.
- Reason: Aligns with the de-facto standard established by Opencode and makes the tool description self-explanatory.
- Impact: Breaking change for any existing direct callers of `AgentTool`.

**Permission inheritance**
- From: Subagents receive the parent's full runtime configuration with no permission filtering.
- To: `derive_subagent_permission()` inherits only parent `Deny` rules and default-denies `task` and `todowrite` unless the subagent type explicitly allows them.
- Reason: Preserves parent security boundaries while preventing unbounded recursion.
- Impact: Subagents are strictly less privileged than the parent.

**ForkPolicy application**
- From: `build_subagent_config` returns the parent's configuration unchanged.
- To: `build_subagent_config` applies `ForkPolicy` to filter inherited message history.
- Reason: Reduces token pressure and keeps subagents focused on their own context.
- Impact: Subagent context size becomes predictable and configurable.

**Background completion notifications**
- From: The main loop polls `AgentControl::check_completed` but injects a generic `<task_result>` message without actual output.
- To: Completed background tasks inject a structured `<task>` result containing the subagent's actual output.
- Reason: The parent LLM needs the subagent's result to continue work.
- Impact: Background delegation becomes end-to-end useful.

## Capabilities

### New Capabilities
- `subagent-task-tool`: Exposes a `task` tool that can spawn foreground or background subagents with a description, prompt, subagent type, and optional task resumption.
- `subagent-permission-inheritance`: Defines how parent session permissions are inherited by child subagents, including default-deny rules for recursion.
- `subagent-background-mode`: Enables background subagent execution with lifecycle tracking and completion notification through `AgentControl`.
- `subagent-built-in-types`: Provides built-in subagent types (`general`, `explore`) with predefined tool sets and permissions.

### Modified Capabilities
- `subagent-session-model`: Requirements for child session construction will be updated to apply `ForkPolicy` and permission inheritance.
- `subagent-event-bridge`: Child session events may include additional metadata related to foreground/background completion.
- `tool-execution`: The tool registry construction contract changes to accept optional `AgentControl` and `SubagentSessionFactory`.

## Impact

- **Affected crates**: `synthia-agent`, `synthia-server`, `synthia-permission`, `synthia-context`, `synthia-tool`.
- **API changes**: `build_default_tool_registry` signature changes; `AgentTool` parameter schema changes.
- **Behavior changes**: Server-managed sessions will expose and be able to execute the `task` tool; subagent permissions become strictly bounded.
- **No database or configuration migrations required**.
