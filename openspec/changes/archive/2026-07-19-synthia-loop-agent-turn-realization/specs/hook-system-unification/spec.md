# Spec: hook-system-unification (Modified)

## MODIFIED Requirements

### Requirement: HookBuilder fire_* methods deprecated

The `HookBuilder` methods `fire_before_llm`, `fire_after_llm`, `fire_before_tool`, `fire_after_tool` SHALL be marked with `#[deprecated(note = "Use UnifiedHookDispatcher::dispatch() instead. Will be removed after 6-month deprecation window.")]`.

#### Scenario: Deprecated fire_before_llm still compiles

WHEN code calls `hooks.fire_before_llm(&mut agent_ctx)`
THEN the code SHALL compile with a deprecation warning
AND the behavior SHALL be identical to the non-deprecated version

#### Scenario: New code uses UnifiedHookDispatcher

WHEN new code needs to dispatch a hook event
THEN it SHALL use `hook_dispatcher.dispatch(HookEvent::PreToolUse { ... })` instead of `hooks.fire_before_tool()`

### Requirement: AgentHookAdapter updated for UnifiedHookDispatcher

`AgentHookAdapter` SHALL implement the `Hook` trait such that `on_event()` dispatches to the appropriate `AgentHook` method based on the `HookEvent` variant.

#### Scenario: AgentHookAdapter dispatches PreToolUse

WHEN `AgentHookAdapter::on_event(HookEvent::PreToolUse { tool_name, args })` is called
THEN it SHALL call the underlying `AgentHook::on_before_tool()` with the appropriate parameters
AND return the converted `HookOutcome`

#### Scenario: AgentHookAdapter dispatches unhandled events

WHEN `AgentHookAdapter::on_event()` is called with an event that has no `AgentHook` equivalent (e.g., `HookEvent::PreCompact`)
THEN it SHALL return `HookOutcome::Allow` (passthrough)
