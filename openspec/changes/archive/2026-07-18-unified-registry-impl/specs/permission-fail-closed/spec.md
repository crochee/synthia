## ADDED Requirements

### Requirement: Doom Loop Policy Decision
`PermissionService` SHALL provide `evaluate_doom_loop(op_ctx, detection)` which uses `GuardianService` as the *detector* only, with the policy *decision* (abort/ask/allow) belonging to the permission pipeline. Threshold SHALL be 3 identical tool calls within 5 turns.

#### Scenario: Doom loop triggers policy decision
- **WHEN** `GuardianService::detect()` returns a `DoomLoopVerdict` indicating doom loop
- **THEN** `PermissionService::evaluate_doom_loop()` SHALL return a `PermissionDecision` (Deny, RequireConfirm, or Allow) based on policy

---

### Requirement: Policy Stale Detection
`PermissionDecision` SHALL include a `PolicyStale { current_generation, seen_generation, reload_hint }` variant. When the `PermissionRuleset::generation` has advanced since the caller's materialization, `evaluate()` SHALL return `PolicyStale` instead of a stale decision.

#### Scenario: Policy stale between turns
- **WHEN** a permission rule is added between turn T and turn T+1
- **THEN** `evaluate()` SHALL return `PolicyStale` with the current generation and a reload hint

#### Scenario: Orchestrator rebuilds materialization on stale
- **WHEN** `evaluate()` returns `PolicyStale`
- **THEN** the orchestrator SHALL rebuild the materialization with the new `PermissionRuleset`

---

### Requirement: PermissionRuleset Generation Counter
`PermissionRuleset` SHALL carry an `AtomicU64` generation counter bumped on every `record_session_rule` or external policy update. Session rule count SHALL be capped at 50.

#### Scenario: Generation bumps on rule addition
- **WHEN** `record_session_rule()` succeeds
- **THEN** `generation` SHALL increment by 1

#### Scenario: Session rule cap enforced
- **WHEN** session rule count reaches 50
- **THEN** further `record_session_rule()` calls SHALL return `PermissionError::SessionRuleCap`
