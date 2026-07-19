# Capability: hook-system-unification

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: codex `codex-rs/core/src/hooks/{runner,outcome}.rs` + Synthia 现有双系统 (`synthia-agent::Hook` + `synthia-plugin::HookRunner`)

## Purpose

合并 Synthia 现有双 hook 系统（`AgentHook` in `synthia-agent` + `HookRunner` in `synthia-plugin`）为单一 `synthia-hook::Hook` trait，引入 `HookOutcome` 3 态（Allow/Deny/ForwardToMainAgent），10 events（其中 `PreMessageDrop` 为 Synthia 独有），集成 LoopDetector 三件套。

## ADDED Requirements

### Requirement: HookOutcome 3-state

Every `Hook::on_event(...)` call MUST return a `HookOutcome` from the enum `{ Allow, Deny { reason }, ForwardToMainAgent { hint } }`.

#### Scenario: Allow outcome default

- **WHEN** a hook returns `HookOutcome::Allow`
- **THEN** the system MUST proceed with the original event flow
- **AND** MUST NOT log a warning

#### Scenario: Deny outcome

- **WHEN** a hook returns `HookOutcome::Deny { reason: String }`
- **THEN** the system MUST abort the current step with `reason` propagated
- **AND** MUST emit a `PreMessageDrop` event (Synthia 独有, 提前告诉 main agent 中断)

#### Scenario: ForwardToMainAgent outcome

- **WHEN** a hook returns `HookOutcome::ForwardToMainAgent { hint }`
- **THEN** the system MUST route the event to the main agent queue
- **AND** MUST NOT block the subagent that triggered the hook
- **AND** MUST log `hook_forwarded_to_main_agent` with hook id + hint hash

### Requirement: 10 typed hook events

The unified `Hook` trait MUST accept these 10 events: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PreResponse`, `PostResponse`, `PreCompact`, `PostCompact`, `PreMessageDrop`.

#### Scenario: 10 events all typed

- **WHEN** the trait's `on_event` is called with any of the 10 events
- **THEN** each payload MUST be a strongly-typed struct (NOT `serde_json::Value`)
- **AND** the consumer MUST NOT need to downcast

#### Scenario: PreMessageDrop Synthia 独有

- **WHEN** an event would be dropped (timeout, cancellation, or tool failure)
- **THEN** the system MUST synthesize and dispatch `PreMessageDrop` BEFORE the actual drop
- **AND** MUST include `reason_code: DropReason` in the payload

### Requirement: LoopDetector integration

The hook system MUST integrate the existing Synthia LoopDetector three-piece suite (`detect_repeat` / `similarity_threshold` / `recovery_action`).

#### Scenario: repeated tool call detected

- **WHEN** `PostToolUse` fires for a third time with > 90% similarity to the previous two
- **THEN** the loop detector MUST classify as `Repeating`
- **AND** MUST emit `HookOutcome::Deny { reason: "loop_detected" }` on the next `PreToolUse`

### Requirement: backward compatibility with deprecation window

The existing `synthia-agent::Hook` + `synthia-plugin::HookRunner` MUST continue to compile, with deprecation warnings, until 6 月 from change #1 merge.

#### Scenario: existing AgentHook still compiles

- **WHEN** a consumer writes `impl AgentHook for Foo {}` targeting `synthia-agent` version pre-deprecation
- **THEN** the build MUST succeed with deprecation warnings visible
- **AND** the trait MUST be auto-bridged to the new `synthia-hook::Hook` via adapter pattern
