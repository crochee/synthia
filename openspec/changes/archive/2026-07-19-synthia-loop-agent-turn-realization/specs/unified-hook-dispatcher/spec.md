# Spec: unified-hook-dispatcher

## ADDED Requirements

### Requirement: UnifiedHookDispatcher struct

The system SHALL provide a `UnifiedHookDispatcher` that dispatches `HookEvent` to both the `HookRegistry` (via `AgentHookAdapter`) and `ExtensionRegistry` in a defined order.

The dispatcher SHALL accept a `HookEvent` and return a `HookOutcome` that represents the combined result of hook and extension evaluation.

#### Scenario: Dispatch PreToolUse event to hooks and extensions

WHEN `UnifiedHookDispatcher::dispatch(HookEvent::PreToolUse { tool_name, args })` is called
THEN the dispatcher SHALL first invoke `HookRegistry::dispatch()` via `AgentHookAdapter`
AND if the hook outcome is `Deny`, the dispatcher SHALL return `Deny` immediately without invoking extensions
AND if the hook outcome is `Allow` or `ForwardToMainAgent`, the dispatcher SHALL then invoke `ExtensionRegistry::dispatch_event()` with the equivalent extension event
AND the final outcome SHALL be the merged result where `Deny > ForwardToMainAgent > Allow`

#### Scenario: Dispatch returns Allow when both hooks and extensions allow

WHEN a `HookEvent` is dispatched and both hook registry and extension registry return `Allow`
THEN the dispatcher SHALL return `HookOutcome::Allow`

#### Scenario: Extension Deny overrides hook Allow

WHEN a `HookEvent` is dispatched, hook registry returns `Allow`, and extension registry returns `Deny { reason }`
THEN the dispatcher SHALL return `HookOutcome::Deny { reason }`

### Requirement: From ExtensionOutcome to HookOutcome conversion

The system SHALL provide `impl From<ExtensionOutcome> for HookOutcome` in the `synthia-extension-hook` crate.

#### Scenario: Convert ExtensionOutcome Allow

WHEN `HookOutcome::from(ExtensionOutcome::Allow)` is called
THEN the result SHALL be `HookOutcome::Allow`

#### Scenario: Convert ExtensionOutcome Deny

WHEN `HookOutcome::from(ExtensionOutcome::Deny { reason })` is called
THEN the result SHALL be `HookOutcome::Deny { reason }`

#### Scenario: Convert ExtensionOutcome ForwardToMainAgent

WHEN `HookOutcome::from(ExtensionOutcome::ForwardToMainAgent { hint })` is called
THEN the result SHALL be `HookOutcome::ForwardToMainAgent { hint }`

### Requirement: Hook-first dispatch ordering

The dispatcher SHALL always invoke hooks before extensions. Hook `Deny` short-circuits extension dispatch.

#### Scenario: Hook Deny prevents extension dispatch

WHEN `dispatch()` is called and `HookRegistry` returns `Deny`
THEN `ExtensionRegistry::dispatch_event()` SHALL NOT be called
AND the dispatcher SHALL return `Deny`

#### Scenario: Hook Allow allows extension dispatch

WHEN `dispatch()` is called and `HookRegistry` returns `Allow`
THEN `ExtensionRegistry::dispatch_event()` SHALL be called
AND the combined outcome SHALL be returned
