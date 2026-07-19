<!--
Raw capture of brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: subagent-task-tool

## Background

Synthia currently has three independently implemented layers of subagent infrastructure, but they are not wired together:

1. **Control layer** (`crates/synthia-agent/src/control/`): `AgentControl`, `AgentRegistry`, `Mailbox`, `SpawnReservation`, `CompletionWatcher` are fully implemented but never injected into `AgentRunConfig`.
2. **Tool layer** (`crates/synthia-agent/src/tools/agent_tools/`): `AgentTool`, `SendMessage`, `Handoff`, `TeamCreate/Delete`, `AgentStatus`, `RegisterAgent` are defined but never registered in `ToolRegistry`.
3. **Session layer** (`crates/synthia-agent/src/subagent/`): `SubagentSessionFactory` trait and `AppStateSubagentFactory` server implementation exist, but `subagent_session_factory` is only set in `SessionController::build_run_config`; `AgentFactory::create` and `Agent::resume` leave it as `None`.

The largest functional gap compared to production agents like Opencode is the lack of a working `task` tool that can delegate work to a subagent, run it in the background, and inherit parent permissions safely.

## Decision Chain

### Q1: What agent-type model should the `task` tool use?

- **Option A**: Reuse existing `RegisterAgent` metadata dynamically.
- **Option B**: Introduce built-in agent types (`build`/`plan`/`explore`/`general`) like Opencode.
- **Option C (chosen)**: Hybrid — provide a small set of built-in types (`general`, `explore`) and let `RegisterAgent` register additional custom types.

**Rationale**: A minimal built-in set gives immediate value and predictable security profiles, while `RegisterAgent` preserves extensibility.

### Q2: When should background subagents be available?

- **Option A**: Always available.
- **Option B**: Gated by an experimental feature flag.
- **Option C (chosen)**: Only available in the server-managed session path where `AgentControl` is injected.

**Rationale**: Background tasks require the `AgentControl` registry for lifecycle tracking. Exposing `background` in paths that lack `AgentControl` would create a promise the runtime cannot keep.

## Design Trade-offs

| Approach | Scope | Pros | Cons |
|----------|-------|------|------|
| **A. MVP** | Register `AgentTool`, inject `AgentControl`, foreground only | Minimal change, fast validation | No background, no permission inheritance, no built-in types |
| **B. Full implementation (recommended)** | Opencode-aligned task params + background + permission inheritance + built-in types | Aligns with production-grade practice, closes the biggest functional gap | Larger change, needs careful testing |
| **C. Refactor & unify** | Merge `SubagentManager` and `AgentControl` | Eliminates two parallel registries | High risk, out of current scope |

## Approved Design

**Chosen approach: B — Full implementation.**

### High-level flow

```
User/LLM calls task tool
        │
        ▼
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   ToolRegistry  │────▶│    AgentTool     │────▶│ AgentControl    │
│  (registered)   │     │  (new signature) │     │ (injected)      │
└─────────────────┘     └────────┬─────────┘     └────────┬────────┘
                                 │                        │
                                 ▼                        ▼
                        ┌─────────────────┐      ┌─────────────────┐
                        │ derive_subagent │      │ background_tasks│
                        │  _permission()  │      │  (check_completed)│
                        └────────┬────────┘      └────────┬────────┘
                                 │                        │
                                 ▼                        ▼
                        ┌─────────────────┐      ┌─────────────────┐
                        │ SubagentSession │      │  Main loop polls│
                        │    Factory      │      │  completion     │
                        └─────────────────┘      └─────────────────┘
```

### Core changes

1. Inject `AgentControl` into all `AgentRunConfig` construction paths.
2. Register `AgentTool` in `build_default_tool_registry` when `AgentControl` and `SubagentSessionFactory` are available.
3. Align `AgentTool` parameters with Opencode `task` tool: `description`, `prompt`, `subagent_type`, `background`, `task_id`.
4. Implement `derive_subagent_permission()`: inherit parent `Deny` rules, default-deny `task` and `todowrite`.
5. Add built-in subagent types `general` and `explore`.
6. Enable background mode only when `AgentControl` is present.
7. Apply `ForkPolicy` in `build_subagent_config` to filter inherited message history.
8. Improve main-loop completion notification to include actual subagent output in `<task_result>`.

### Security boundaries

- Default-deny recursive `task` calls.
- Default-deny `todowrite` in subagents.
- Inherit all parent `Deny` rules.
- Max recursion depth 3.
- Max concurrent background tasks 5.

### Key interface

```rust
// New: crates/synthia-agent/src/subagent/permission.rs
pub fn derive_subagent_permission(
    parent_permission: &[PermissionRule],
    subagent_allows_task: bool,
    subagent_allows_todowrite: bool,
) -> Vec<PermissionRule>;
```

```json
{
  "description": "string",
  "prompt": "string",
  "subagent_type": "string",
  "background": "boolean",
  "task_id": "string?"
}
```
