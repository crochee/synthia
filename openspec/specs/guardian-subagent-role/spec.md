# guardian-subagent-role Specification

## Purpose
Defines the Guardian subagent review pattern: spawning an isolated subagent session for LLM-based evaluation of medium-risk tool calls, with three-layer lockdown to prevent recursion and fail-closed fallback on subagent failure.

## Requirements

### Requirement: Guardian subagent SHALL spawn via SubagentSessionFactory

The `GuardianSubagentReviewer` SHALL spawn a Guardian review subagent by calling `SubagentSessionFactory::run_child` with a review prompt built from the parent's `ApprovalRequest` and compressed transcript. The subagent SHALL run in an isolated session with its own `SessionController`, event channel, and turn loop, distinct from the parent session's context. The parent session SHALL await the subagent's `Finish` event synchronously via `tokio::time::timeout` wrapping `run_child`.

#### Scenario: Spawn Guardian subagent for medium-risk tool call
- **WHEN** `GuardianCoordinator::check` determines risk score in [50, 80) for a tool call
- **THEN** `GuardianSubagentReviewer` SHALL call `SubagentSessionFactory::run_child` with `parent_session_id`, a review prompt, and `user_id`
- **AND** the subagent SHALL execute in an isolated session separate from the parent

#### Scenario: Low-risk tool call bypasses subagent spawn
- **WHEN** `GuardianCoordinator::check` determines risk score < 50
- **THEN** `GuardianSubagentReviewer` SHALL NOT be invoked and no subagent SHALL be spawned
- **AND** `SimpleGuardian` SHALL return `GuardianDecision::Allow` immediately

#### Scenario: High-risk tool call bypasses subagent spawn
- **WHEN** `GuardianCoordinator::check` determines risk score >= 80
- **THEN** `GuardianSubagentReviewer` SHALL NOT be invoked and no subagent SHALL be spawned
- **AND** `SimpleGuardian` SHALL return `GuardianDecision::Deny` immediately

---

### Requirement: Guardian subagent SHALL enforce three-layer configuration lockdown

The Guardian subagent SHALL be configured with three independent lockdown layers to prevent recursion and enforce least privilege: (1) permission layer via `derive_subagent_permission` Deny-only inheritance; (2) config layer with `guardian_enabled: false` and `max_iterations: 1`; (3) tool layer with an empty tool registry. Each layer SHALL function independently such that failure of one layer does not compromise the others.

#### Scenario: Permission layer denies all tool execution in subagent
- **WHEN** the Guardian subagent attempts any tool call
- **THEN** `derive_subagent_permission` SHALL deny the call because only Deny rules are inherited from the parent

#### Scenario: Config layer prevents Guardian recursion
- **WHEN** the Guardian subagent processes a tool call that would trigger another Guardian review
- **THEN** `guardian_enabled: false` in the subagent's `AgentRunConfig` SHALL prevent spawning a nested Guardian subagent

#### Scenario: Config layer limits subagent to single iteration
- **WHEN** the Guardian subagent completes one LLM call
- **THEN** `max_iterations: 1` SHALL terminate the subagent's turn loop after a single LLM response

#### Scenario: Tool layer provides empty registry
- **WHEN** the Guardian subagent session is initialized
- **THEN** the subagent's tool registry SHALL contain zero tools, ensuring the subagent can only produce a text assessment

---

### Requirement: Guardian subagent SHALL use isolated prompt-cache key

The Guardian subagent's prompt-cache key SHALL be `guardian:{parent_session_id}`, injected via `SystemContext Source` (P1-4). This key SHALL namespace the Guardian's cache entries separately from the parent session and from other sessions, satisfying the project memory hard constraint that cache hashes include user_id namespace (the `parent_session_id` already contains the user_id namespace).

#### Scenario: Prompt-cache key namespaced to parent session
- **WHEN** a Guardian subagent is spawned for parent session `S1`
- **THEN** the subagent's `prompt_cache_key` SHALL be `guardian:S1`
- **AND** the key SHALL NOT collide with the parent session's own `prompt_cache_key`

#### Scenario: Cross-session cache isolation
- **WHEN** Guardian subagents are spawned for sessions `S1` and `S2` belonging to different users
- **THEN** their prompt-cache keys (`guardian:S1` and `guardian:S2`) SHALL be distinct, preventing cross-user cache pollution

---

### Requirement: Guardian subagent decision SHALL flow back via existing AgentEvent types

The parent session SHALL emit `AgentEvent::GuardianConfirmationRequest` when a Guardian subagent review was initiated (reported after the review completes via the `escalated` field of `GuardianCheckOutcome`), and `AgentEvent::GuardianWarning` when the review returns `Deny` or `NeedUserConfirm`. The Guardian subagent's `Finish { output }` event SHALL be parsed by the parent into a `GuardianDecision` via the existing `parse_assessment_response` logic. No new `AgentEvent` variant SHALL be introduced.

#### Scenario: Review start emits GuardianConfirmationRequest
- **WHEN** `GuardianSubagentReviewer` spawns a Guardian subagent
- **THEN** the parent session SHALL emit `AgentEvent::GuardianConfirmationRequest` with the `ApprovalRequest` being reviewed

#### Scenario: Review deny emits GuardianWarning
- **WHEN** the Guardian subagent returns an assessment with risk >= 80 or outcome denied
- **THEN** the parent session SHALL parse the `Finish` output into `GuardianDecision::Deny`
- **AND** SHALL emit `AgentEvent::GuardianWarning` with the denial rationale

#### Scenario: Review allow does not emit GuardianWarning
- **WHEN** the Guardian subagent returns an assessment with risk < 50 and outcome approved
- **THEN** the parent session SHALL parse the `Finish` output into `GuardianDecision::Allow`
- **AND** SHALL NOT emit `AgentEvent::GuardianWarning`

---

### Requirement: Guardian subagent SHALL support cancellation via cancel_token

The Guardian subagent lifecycle SHALL be cancellable via a `CancellationToken` propagated from the parent session. When the parent session is interrupted (steering, abort, or session reset), the `CancellationToken` SHALL be triggered, causing the `run_child` await to return early. The parent SHALL treat cancellation as a subagent failure and apply the fallback policy.

#### Scenario: Parent session abort cancels Guardian subagent
- **WHEN** the parent session receives an abort signal while awaiting Guardian subagent review
- **THEN** the `CancellationToken` SHALL be triggered
- **AND** the `run_child` await SHALL return early
- **AND** `GuardianCoordinator` SHALL apply fallback to `SimpleGuardian::NeedUserConfirm`

#### Scenario: Guardian subagent timeout triggers fallback
- **WHEN** the Guardian subagent does not complete within `GuardianConfig::timeout` (default 90s)
- **THEN** `tokio::time::timeout` SHALL return `Err(Elapsed)`
- **AND** `GuardianCoordinator` SHALL apply fallback to `SimpleGuardian::NeedUserConfirm`

---

### Requirement: GuardianConfig SHALL include subagent timeout and enable flag

The `GuardianConfig` SHALL include a `timeout: Duration` field (default 90s) controlling the maximum duration of a Guardian subagent review, and a `subagent_enabled: bool` field (default `false` for backward compatibility) controlling whether the subagent review path is active. Both fields SHALL use `#[serde(default)]` for backward compatibility with existing configuration files.

#### Scenario: Default config disables subagent review
- **WHEN** `GuardianConfig` is deserialized from a config file without `subagent_enabled`
- **THEN** `subagent_enabled` SHALL default to `false`
- **AND** `GuardianCoordinator` SHALL use the legacy `SimpleGuardian::NeedUserConfirm` path for 50-79 risk without spawning a subagent

#### Scenario: Subagent review enabled with custom timeout
- **WHEN** `GuardianConfig` is configured with `subagent_enabled: true` and `timeout: 60s`
- **THEN** `GuardianCoordinator` SHALL spawn a Guardian subagent for 50-79 risk tool calls
- **AND** the subagent review SHALL be bounded by 60s timeout

---

### Requirement: GuardianSubagentReviewer SHALL build review prompt from parent transcript

The `GuardianSubagentReviewer` SHALL construct the Guardian subagent's user message by calling the existing `build_review_prompt` with `collect_transcript_entries(conversation)`, the serialized `ApprovalRequest` JSON, and `None` for the optional context. The subagent's system prompt SHALL be a guardian policy constant defined in `synthia-guardian`, independent of the parent session's system prompt. The expected subagent output SHALL be a JSON assessment `{ risk_level, user_authorization, outcome, rationale }`.

#### Scenario: Review prompt includes compressed transcript
- **WHEN** `GuardianSubagentReviewer` builds the review prompt for a tool call in a multi-turn conversation
- **THEN** the prompt SHALL include transcript entries from `collect_transcript_entries(conversation)`
- **AND** the prompt SHALL include the serialized `ApprovalRequest` as `action_json`

#### Scenario: Subagent system prompt is guardian policy
- **WHEN** the Guardian subagent session is initialized
- **THEN** the subagent's system prompt SHALL be the guardian policy constant
- **AND** SHALL NOT include the parent session's system prompt content
