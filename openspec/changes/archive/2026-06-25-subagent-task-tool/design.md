## Context

Synthia has three independently implemented layers of subagent infrastructure, but none of them are wired together in the main agent runtime:

1. **Control layer** (`crates/synthia-agent/src/control/`): `AgentControl`, `AgentRegistry`, `Mailbox`, `SpawnReservation`, and `CompletionWatcher` exist and are functional, yet `AgentRunConfig.agent_control` is set to `None` in every construction path (`SessionController::build_run_config`, `AgentFactory::create`, `Agent::resume`).
2. **Tool layer** (`crates/synthia-agent/src/tools/agent_tools/`): `AgentTool`, `SendMessage`, `Handoff`, `TeamCreate/Delete`, `AgentStatus`, and `RegisterAgent` are implemented, but `build_default_tool_registry()` only registers the basic read/write/grep/bash tools. `AgentTool` is never exposed to the LLM.
3. **Session layer** (`crates/synthia-agent/src/subagent/`): `SubagentSessionFactory` and the server-side `AppStateSubagentFactory` can create and run child sessions, but the factory is only injected in `SessionController::build_run_config`; `AgentFactory::create` and resume paths leave it `None`.

The result is that Synthia cannot currently delegate work to a subagent, cannot run subagents in the background, and cannot safely inherit parent permissions. This is the largest functional gap relative to production agents such as Opencode, whose `task` tool is a core primitive for decomposing complex work.

## Goals / Non-Goals

**Goals:**
- Expose a `task` tool to the LLM that can spawn a subagent to handle multi-step work.
- Support both foreground and background execution of subagents.
- Implement safe permission inheritance from parent to child (deny-only inheritance + default-deny recursion).
- Provide a small set of built-in subagent types (`general`, `explore`) while allowing `RegisterAgent` extensions.
- Wire the existing `AgentControl`, `AgentTool`, and `SubagentSessionFactory` layers together end-to-end.

**Non-Goals:**
- Merge `SubagentManager` and `AgentControl` into a single abstraction.
- Add new UI/TUI panels for background task management.
- Implement Docker/VM-level sandboxing for subagents.
- Support background subagents in CLI/resume/standalone paths that lack `AgentControl`.

## Decisions

### D1: Built-in agent types vs. fully dynamic registry
- **Choice**: Hybrid model. Provide built-in types `general` (broad tool access) and `explore` (read-only) in the tool description, while keeping `RegisterAgent` as the extension mechanism.
- **Rationale**: A minimal built-in set gives immediate value and predictable security profiles. `RegisterAgent` preserves extensibility without forcing users to design types from scratch.
- **Alternatives considered**:
  - Fully dynamic only: slower to adopt, no guaranteed safe defaults.
  - Opencode-style full catalog (`build`/`plan`/`explore`/`general`): heavier than needed; `build` overlaps with the primary agent.

### D2: Background mode availability
- **Choice**: Background subagents are only available when `AgentControl` is injected (currently the server-managed session path).
- **Rationale**: Background lifecycle tracking requires `AgentControl.register_background_task` / `check_completed`. Exposing `background` in paths without `AgentControl` would advertise a capability the runtime cannot fulfill.
- **Alternatives considered**:
  - Always available: would fail at runtime in CLI/resume paths.
  - Feature flag: adds configuration complexity; the real gate is infrastructure presence, not a toggle.

### D3: Permission inheritance model
- **Choice**: Inherit only parent `Deny` rules into the child, and default-deny the `task` and `todowrite` tools unless the subagent type explicitly allows them.
- **Rationale**: Deny rules define hard security boundaries; allowing them to propagate downward preserves those boundaries. Allow rules are intentionally not inherited so the child must earn its own capabilities through its type definition. Default-deny on `task` prevents unbounded recursion.
- **Alternatives considered**:
  - Full inheritance (allow + deny): would over-privilege subagents.
  - No inheritance: would lose parent-configured deny rules such as `.env` protection.

### D4: Tool registration strategy
- **Choice**: `build_default_tool_registry` will accept optional `AgentControl` and `SubagentSessionFactory`. If both are present, it registers `AgentTool`; otherwise it does not.
- **Rationale**: `AgentTool` is useless without both the control plane (background tracking) and the session factory (actual child creation). Conditional registration avoids runtime errors in paths that lack either dependency.
- **Alternatives considered**:
  - Always register and fail at call time: worse UX; LLM sees a tool it cannot use.
  - Register a stub that returns "not supported": adds noise to the context.

### D5: ForkPolicy application
- **Choice**: Apply `ForkPolicy` inside `build_subagent_config` to filter the parent's message history before constructing the child's initial state.
- **Rationale**: The function currently returns `parent_config` unchanged, so child agents inherit the full parent history. Applying the configured policy reduces token pressure and keeps children focused.
- **Alternatives considered**:
  - Apply at call site: scatters policy logic across the codebase.
  - Ignore ForkPolicy: wastes context window and leaks unrelated parent turns.

## Risks / Trade-offs

[Risk] `AgentControl` and `SubagentManager` remain parallel abstractions.
→ Mitigation: Keep their responsibilities clearly separated for this change; `AgentControl` tracks background tasks and registry, `SubagentManager` wraps it for depth/concurrency limits and tool execution. A future change can evaluate unification.

[Risk] Permission inheritance is coarse (tool-level, not file-level).
→ Mitigation: Reuse Synthia's existing `PermissionRule.pattern` matching; if file-level rules are added later, the same deny rules will propagate without structural change.

[Trade-off] Built-in types `general`/`explore` hard-code tool sets and permissions.
→ Acceptance: This is intentional to provide safe defaults. Custom types via `RegisterAgent` remain possible for specialized needs.

[Trade-off] Background tasks only work in server sessions.
→ Acceptance: The CLI path does not currently maintain a persistent control plane; enabling it would require injecting `AgentControl` and keeping it alive across REPL turns, which is out of scope.

## Migration Plan

This change is purely additive and does not introduce new database schemas, API endpoints, or configuration file formats.

1. Merge the code changes.
2. Run the full test suite (`cargo test --workspace`) and fix any failures.
3. Validate that the `task` tool appears in server-managed sessions and is absent in standalone `AgentFactory::create` paths.
4. Rollback: revert the commit. No persistent state migration is required.

## Open Questions

1. Should `AgentFactory::create` also receive `AgentControl` so that programmatically created agents can spawn subagents? Currently it is out of scope, but it is a natural follow-up.
2. Should the `explore` subagent type deny `bash` entirely, or allow read-only bash commands? The final permission set needs review against project security policy.
3. Is there an existing mechanism to advertise only the `background` parameter when `AgentControl` is present, or should the JSON schema always include it and the tool reject it at runtime?
