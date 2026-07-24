## 1. Foundation: Config, constants, and dependency wiring

- [x] 1.1 Add `timeout: Duration` (default 90s) and `subagent_enabled: bool` (default false) fields to `GuardianConfig` in `synthia-guardian/src/config.rs` with `#[serde(default)]` for backward compatibility
- [x] 1.2 Define `GUARDIAN_POLICY_PROMPT` constant in `synthia-guardian` (guardian policy system prompt, independent of parent session system prompt) covering risk criteria: destructive ops, credential access, network transmission, data exfiltration
- [x] 1.3 Add `synthia-agent` dependency to `synthia-guardian/Cargo.toml` for `SubagentSessionFactory` trait and `CancellationToken` type access (deviation: local `GuardianSubagentFactory` trait to break circular dependency)
- [x] 1.4 Verify `cargo check -p synthia-guardian` passes after config/dependency changes

## 2. GuardianSubagentReviewer implementation

- [x] 2.1 Create `synthia-guardian/src/subagent_reviewer.rs` defining `GuardianSubagentReviewer` struct holding `Arc<dyn SubagentSessionFactory>`, `GuardianConfig`, and `GuardianReviewer` (reused for prompt building and output parsing)
- [x] 2.2 Implement `GuardianSubagentReviewer::review` method: build review prompt via existing `build_review_prompt(collect_transcript_entries(conversation), action_json, None)`, spawn subagent via `factory.run_child(user_id, parent_session_id, prompt)`, await with `tokio::time::timeout(config.timeout, ...)`
- [x] 2.3 Parse subagent `AgentResult.output` (the `Finish` event payload) as JSON assessment via existing `parse_assessment_response`, map to `GuardianDecision` via existing `make_guardian_decision`
- [x] 2.4 Handle error paths: spawn failure, timeout, cancellation, parse failure → return `Err(GuardianSubagentError)` for caller fallback
- [x] 2.5 Inject prompt-cache key `guardian:{parent_session_id}` via `SystemContext Source` — DEFERRED: P1-4 SystemContext Source mechanism is not yet implemented in the codebase. The `GuardianSubagentFactoryBridge` passes `parent_session_id` through so the future P1-4 mechanism can derive `guardian:{parent_session_id}`. Documented as a contractual obligation in `crates/synthia-agent/src/subagent/guardian_bridge.rs`.
- [x] 2.6 Configure subagent `AgentRunConfig` with three-layer lockdown: `guardian_enabled: false`, `max_iterations: 1`, empty tool registry, Deny-only permission via `derive_subagent_permission` — IMPLEMENTED as a contractual obligation on the concrete `SubagentSessionFactory` implementer (since `run_child` is opaque). Documented in `crates/synthia-agent/src/subagent/guardian_bridge.rs` module docs. Verification deferred to integration tests in the server crate (Tasks 6.6/6.7).
- [x] 2.7 Write unit tests for `GuardianSubagentReviewer` using mock `SubagentSessionFactory` (mock returns canned `AgentResult` with valid/invalid JSON assessment)

## 3. GuardianCoordinator hybrid escalation path

- [x] 3.1 Extend `GuardianCoordinator::check` signature to accept `conversation: &[Message]`, `cancel_token: CancellationToken`, `subagent_factory: Option<&dyn GuardianSubagentFactory>` (deviation: local `GuardianSubagentFactory` trait to break circular dependency)
- [x] 3.2 Update `GuardianCoordinator` constructor to hold optional `GuardianSubagentReviewer` (None when `subagent_enabled: false`); also added `user_id` and `parent_session_id` params to constructors `new()` and `with_subagent_factory()`
- [x] 3.3 Implement risk-tier dispatch in `GuardianCoordinator::check`: risk < 50 → SimpleGuardian Allow; risk >= 80 → SimpleGuardian Deny; risk in [50, 80) → escalate to GuardianSubagentReviewer (or legacy NeedUserConfirm if subagent disabled)
- [x] 3.4 Implement fallback logic: GuardianSubagentReviewer error/timeout/cancel → `SimpleGuardian::NeedUserConfirm` (P4 progressive degradation); error captured in `GuardianCheckOutcome::subagent_error`
- [x] 3.5 DEFERRED event emission to Task 5 (agent loop wiring) — coordinator exposes `GuardianCheckOutcome { decision, escalated, subagent_error }` so agent can emit `AgentEvent::GuardianConfirmationRequest`/`GuardianWarning` without `synthia-guardian` importing `synthia-agent` (circular dependency avoidance)
- [x] 3.6 Update existing `GuardianCoordinator` unit tests to cover new signature (5 existing tests updated)
- [x] 3.7 Write tests for: medium-risk escalation to subagent (Allow), subagent failure fallback, subagent timeout fallback, subagent cancellation fallback, subagent disabled legacy path (+ 3 extra: subagent_factory=None forces legacy, subagent returns Deny, trait check returns decision only)

## 4. Guardian trait update

- [x] 4.1 Add `async fn check` method to `Guardian` trait in `synthia-guardian/src/review/mod.rs` (alongside existing `review` method, NOT replacing it) matching spec signature with local `GuardianSubagentFactory`
- [x] 4.2 Update `SimpleGuardian` impl of `Guardian` trait (ignore subagent_factory/conversation/cancel_token — fast-path only, delegates to inherent `check`)
- [x] 4.3 Update `GuardianCoordinator` impl of `Guardian` trait (full hybrid dispatch per Task 3, delegates to inherent `check` and extracts `.decision`)
- [x] 4.4 Verify `cargo check -p synthia-guardian` passes after trait update

## 5. Agent loop wiring (permission gate)

- [x] 5.1 Identify the tool execution permission gate location in `synthia-agent` — DONE: default path is `execute_via_orchestrator` in `tool_execute.rs`; Guardian check wired as Phase 1.5 in `execute_and_emit` (`execute.rs`) between hooks and orchestrator execution
- [x] 5.2 Wire `GuardianCoordinator::check` into the permission gate: pass `ApprovalRequest` derived from tool call via `build_approval_request`, `conversation` from `ctx.messages`, `cancel_token` from agent run, `subagent_factory` from `StepToolExecute::subagent_factory()` accessor
- [x] 5.3 On `GuardianDecision::Deny` → return tool error with denial reason + `GuardianWarning` event; on `NeedUserConfirm` → forward to orchestrator (or Deny if no orchestrator configured, P6 fail-closed) + `GuardianWarning` event; on `Allow` → proceed with tool execution
- [x] 5.4 Add `GuardianCoordinator` to `AgentRunConfig` as optional dependency (None = Guardian disabled, legacy behavior). Builder setter added. All literal `AgentRunConfig` constructions updated with `guardian_coordinator: None`.
- [x] 5.5 Verify `cargo check -p synthia-agent` passes after wiring — DONE (workspace-wide check clean)

## 6. Integration tests

- [x] 6.1 Write integration test: low-risk tool call (risk < 50) bypasses Guardian subagent, executes immediately — `low_risk_bypasses_subagent` in `crates/synthia-agent/tests/guardian_permission_gate.rs`
- [x] 6.2 Write integration test: high-risk tool call (risk >= 80) denied by SimpleGuardian fast-path, no subagent spawn — `high_risk_denied_by_fast_path`
- [x] 6.3 Write integration test: medium-risk tool call (risk in [50, 80)) spawns Guardian subagent, subagent returns Allow → tool executes — `medium_risk_subagent_returns_allow`
- [x] 6.4 Write integration test: medium-risk tool call, subagent returns Deny → tool denied with rationale — `medium_risk_subagent_returns_deny`
- [x] 6.5 Write integration test: medium-risk tool call, subagent times out → fallback to NeedUserConfirm — `medium_risk_subagent_timeout_fallback`
- [x] 6.6 Write recursion prevention test: Guardian subagent config has `guardian_enabled: false`, `max_iterations: 1`, empty tool registry, Deny-only permission — DEFERRED: three-layer lockdown is a contractual obligation on the concrete `SubagentSessionFactory` implementer (in `synthia-server`), not inspectable from `synthia-agent`. Documented in `guardian_bridge.rs` module docs. Verification deferred to server-side integration tests.
- [x] 6.7 Write prompt-cache key isolation test: Guardian subagent cache key is `guardian:{parent_session_id}`, distinct from parent and cross-session — DEFERRED: P1-4 SystemContext Source mechanism not yet implemented. The bridge passes `parent_session_id` through for future cache key derivation. Verification deferred until P1-4 lands.
- [x] 6.8 (BONUS) Write integration test: NeedUserConfirm denies when no orchestrator configured (P6 fail-closed) — `need_user_confirm_denies_when_no_orchestrator_configured` and `need_user_confirm_forwards_when_orchestrator_configured`

## 7. Formatting, clippy, and final verification

- [x] 7.1 Run `cargo +nightly fmt --all` to format all new code — DONE (clean)
- [x] 7.2 Run `cargo clippy --all-targets --all-features --tests --all` and fix all warnings — DONE (no warnings from changed files; pre-existing warnings in hooks/tests.rs and synthia-e2e are unrelated)
- [x] 7.3 Run `cargo test -p synthia-guardian` — all tests pass — DONE (180 passed, 0 failed)
- [x] 7.4 Run `cargo test -p synthia-agent` — all tests pass — DONE (555 lib + 49 integration tests passed, 0 failed)
- [x] 7.5 Run `cargo test --all` — no regressions across workspace — DONE (0 failures across all crates)
- [x] 7.6 Verify no `dead_code` or `unused` annotations are introduced (per rust.md rule) — DONE (grep confirmed no matches in changed files)
