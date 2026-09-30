# Guardian Subagent Role (P1-2) Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** Upgrade Guardian from inline-LLM to codex-style subagent with isolated context, three-layer lockdown, and hybrid escalation wiring.

**Architecture:** `GuardianSubagentReviewer` wraps `SubagentSessionFactory` + existing `GuardianReviewer` (prompt building/parsing). `GuardianCoordinator` dispatches risk tiers: <50 Allow, >=80 Deny, [50,80) escalate to subagent with fallback to `SimpleGuardian::NeedUserConfirm`. Guardian subagent runs in isolated session with `guardian_enabled: false`, `max_iterations: 1`, empty tool registry, Deny-only permission.

**Tech Stack:** Rust, `synthia-guardian` crate, `synthia-agent` subagent framework (`SubagentSessionFactory`), `tokio::time::timeout`, `CancellationToken`, existing `build_review_prompt`/`parse_assessment_response`.

---

## Task 1: Foundation — Config, constants, dependency wiring

- [ ] **Step 1:** Read `crates/synthia-guardian/src/config.rs` and `crates/synthia-guardian/Cargo.toml` to understand current `GuardianConfig` fields and dependencies
- [ ] **Step 2:** Add `timeout: Duration` (default `Duration::from_secs(90)`) and `subagent_enabled: bool` (default `false`) to `GuardianConfig` with `#[serde(default)]`
- [ ] **Step 3:** Add `synthia-agent = { path = "../synthia-agent" }` to `synthia-guardian/Cargo.toml` `[dependencies]` section for `SubagentSessionFactory` trait
- [ ] **Step 4:** Create `GUARDIAN_POLICY_PROMPT` constant in `synthia-guardian/src/policy.rs` covering risk criteria (destructive ops, credential access, network transmission, data exfiltration); re-export from `lib.rs`
- [ ] **Step 5:** Run `cargo check -p synthia-guardian` to verify config/dependency changes compile
- [ ] **Commit point:** "feat(guardian): add config fields and policy constant for subagent role"

## Task 2: GuardianSubagentReviewer struct and review method

- [ ] **Step 1:** Read `crates/synthia-agent/src/subagent/factory.rs` for `SubagentSessionFactory` trait signature and `ChildSessionHandle`/`AgentResult` types
- [ ] **Step 2:** Read `crates/synthia-guardian/src/review/reviewer.rs` for `build_review_prompt`, `collect_transcript_entries`, `parse_assessment_response`, `make_guardian_decision` signatures
- [ ] **Step 3:** Create `crates/synthia-guardian/src/subagent_reviewer.rs` defining `GuardianSubagentReviewer { factory: Arc<dyn SubagentSessionFactory>, config: GuardianConfig, reviewer: GuardianReviewer }`
- [ ] **Step 4:** Implement `GuardianSubagentReviewer::review(&self, request: &ApprovalRequest, conversation: &[Message], parent_session_id: &str, user_id: &str, cancel_token: CancellationToken) -> Result<GuardianDecision, GuardianSubagentError>`
- [ ] **Step 5:** Inside `review`: build prompt via `build_review_prompt(collect_transcript_entries(conversation), request.to_json()?, None)`, spawn via `self.factory.run_child(user_id.to_string(), parent_session_id.to_string(), prompt)`, wrap in `tokio::time::timeout(self.config.timeout, ...)`
- [ ] **Step 6:** Parse `AgentResult.output` via `parse_assessment_response` → `make_guardian_decision` → return `GuardianDecision`
- [ ] **Step 7:** Define `GuardianSubagentError` enum: `SpawnFailed`, `Timeout`, `Cancelled`, `ParseFailed`, `SessionEnded(String)`
- [ ] **Step 8:** Run `cargo check -p synthia-guardian` to verify struct compiles
- [ ] **Commit point:** "feat(guardian): implement GuardianSubagentReviewer with isolated subagent spawn"

## Task 3: Subagent config lockdown (three-layer)

- [ ] **Step 1:** Read `crates/synthia-agent/src/subagent/config.rs` for `build_subagent_config` and `crates/synthia-agent/src/subagent/permission.rs` for `derive_subagent_permission`
- [ ] **Step 2:** In `GuardianSubagentReviewer::review`, before `run_child`, build a `GuardianSubagentConfig` that sets: `guardian_enabled: false`, `max_iterations: 1`, empty tool registry, `derive_subagent_permission` Deny-only
- [ ] **Step 3:** Inject prompt-cache key `guardian:{parent_session_id}` via `SystemContext Source` (read `crates/synthia-agent/src/stream_builder/` for P1-4 Source mechanism)
- [ ] **Step 4:** Verify the lockdown config is passed to `SubagentSessionFactory` (may need to extend `run_child` signature or pass via a config struct — check if existing `build_subagent_config` supports these fields)
- [ ] **Step 5:** Run `cargo check -p synthia-guardian` to verify lockdown config compiles
- [ ] **Commit point:** "feat(guardian): enforce three-layer lockdown on Guardian subagent config"

## Task 4: GuardianSubagentReviewer unit tests

- [ ] **Step 1:** Create `crates/synthia-guardian/tests/subagent_reviewer.rs` integration test file
- [ ] **Step 2:** Write test `review_returns_allow_for_low_risk_assessment` — mock `SubagentSessionFactory` returns `AgentResult { output: r#"{"risk_level":30,"outcome":"approved"}"#, status: Completed, ... }`, assert `GuardianDecision::Allow`
- [ ] **Step 3:** Write test `review_returns_deny_for_high_risk_assessment` — mock returns risk_level 85 denied, assert `GuardianDecision::Deny`
- [ ] **Step 4:** Write test `review_returns_err_on_timeout` — mock blocks longer than `config.timeout`, assert `Err(GuardianSubagentError::Timeout)`
- [ ] **Step 5:** Write test `review_returns_err_on_parse_failure` — mock returns non-JSON output, assert `Err(GuardianSubagentError::ParseFailed)`
- [ ] **Step 6:** Run `cargo test -p synthia-guardian --test subagent_reviewer` to verify all tests pass
- [ ] **Commit point:** "test(guardian): add GuardianSubagentReviewer unit tests"

## Task 5: GuardianCoordinator hybrid escalation

- [ ] **Step 1:** Read `crates/synthia-guardian/src/guardian_coordinator.rs` for current `GuardianCoordinator::check` implementation
- [ ] **Step 2:** Extend `GuardianCoordinator` struct to hold `Option<GuardianSubagentReviewer>` (None when `subagent_enabled: false`)
- [ ] **Step 3:** Update `GuardianCoordinator::check` signature per spec: `async fn check(&self, request, conversation, cancel_token, subagent_factory: Option<&dyn SubagentSessionFactory>) -> GuardianDecision`
- [ ] **Step 4:** Implement risk-tier dispatch: call `SimpleGuardian::assess_risk` first; if risk < 50 → Allow; if risk >= 80 → Deny; if [50, 80) → escalate
- [ ] **Step 5:** For [50, 80) escalation: if `subagent_enabled` and `subagent_factory.is_some()` → call `GuardianSubagentReviewer::review`; on `Err` → fallback `SimpleGuardian::NeedUserConfirm`
- [ ] **Step 6:** Emit `AgentEvent::GuardianConfirmationRequest` at review start and `AgentEvent::GuardianWarning` on Deny/NeedUserConfirm (via parent event sender if available)
- [ ] **Step 7:** Run `cargo check -p synthia-guardian` to verify coordinator compiles
- [ ] **Commit point:** "feat(guardian): implement hybrid escalation to subagent in GuardianCoordinator"

## Task 6: Guardian trait update

- [ ] **Step 1:** Read `crates/synthia-guardian/src/review/mod.rs` for current `Guardian` trait definition
- [ ] **Step 2:** Update `Guardian` trait signature to match spec: `async fn check(&self, request: &ApprovalRequest, conversation: &[Message], cancel_token: CancellationToken, subagent_factory: Option<&dyn SubagentSessionFactory>) -> GuardianDecision`
- [ ] **Step 3:** Update `SimpleGuardian` impl — ignore `conversation`/`cancel_token`/`subagent_factory` (fast-path only, risk < 50 Allow, risk >= 80 Deny, [50,80) NeedUserConfirm)
- [ ] **Step 4:** Update `GuardianCoordinator` impl — full hybrid dispatch per Task 5
- [ ] **Step 5:** Update any existing `Guardian` trait implementations/callers to match new signature (grep for `impl Guardian for`)
- [ ] **Step 6:** Run `cargo check -p synthia-guardian` and `cargo test -p synthia-guardian` to verify trait update compiles and existing tests pass
- [ ] **Commit point:** "refactor(guardian): update Guardian trait signature for subagent support"

## Task 7: GuardianCoordinator escalation tests

- [ ] **Step 1:** Add tests to `crates/synthia-guardian/tests/` for escalation scenarios
- [ ] **Step 2:** Write test `medium_risk_escalates_to_subagent` — risk 65, subagent returns Allow → assert `GuardianDecision::Allow` (not NeedUserConfirm)
- [ ] **Step 3:** Write test `medium_risk_subagent_failure_falls_back_to_need_user_confirm` — risk 65, subagent returns `Err(Timeout)` → assert `GuardianDecision::NeedUserConfirm`
- [ ] **Step 4:** Write test `medium_risk_subagent_disabled_returns_need_user_confirm` — risk 65, `subagent_enabled: false` → assert `NeedUserConfirm` (legacy path)
- [ ] **Step 5:** Write test `low_risk_bypasses_subagent` — risk 30 → assert `Allow`, verify mock subagent factory never called
- [ ] **Step 6:** Write test `high_risk_bypasses_subagent` — risk 90 → assert `Deny`, verify mock subagent factory never called
- [ ] **Step 7:** Run `cargo test -p synthia-guardian` to verify all coordinator tests pass
- [ ] **Commit point:** "test(guardian): add GuardianCoordinator hybrid escalation tests"

## Task 8: Agent loop wiring (permission gate)

- [ ] **Step 1:** Read `crates/synthia-agent/src/` to identify the tool execution permission gate location (grep for `PermissionChecker::check` or `ToolOrchestrator::execute`)
- [ ] **Step 2:** Add `guardian_coordinator: Option<Arc<GuardianCoordinator>>` to `AgentRunConfig` (None = Guardian disabled)
- [ ] **Step 3:** At the permission gate, after `PermissionChecker::check` passes, call `GuardianCoordinator::check` if configured: derive `ApprovalRequest` from the tool call, pass `conversation` from session context, `cancel_token` from agent run, `subagent_factory` from `AgentRunConfig`
- [ ] **Step 4:** On `GuardianDecision::Deny` → return tool error with denial reason; on `NeedUserConfirm` → trigger approval flow (or return error if no approval service); on `Allow` → proceed
- [ ] **Step 5:** Run `cargo check -p synthia-agent` to verify wiring compiles
- [ ] **Commit point:** "feat(agent): wire GuardianCoordinator into tool execution permission gate"

## Task 9: Integration tests

- [ ] **Step 1:** Create or extend integration test in `crates/synthia-agent/tests/` for Guardian permission gate
- [ ] **Step 2:** Write test `low_risk_tool_executes_without_guardian_subagent` — tool with risk < 50 executes immediately, no subagent spawned
- [ ] **Step 3:** Write test `high_risk_tool_denied_by_simple_guardian` — tool with risk >= 80 denied, no subagent spawned
- [ ] **Step 4:** Write test `medium_risk_tool_triggers_guardian_subagent_allow` — tool with risk in [50, 80), subagent returns Allow, tool executes
- [ ] **Step 5:** Write test `medium_risk_tool_subagent_timeout_falls_back` — subagent times out, fallback to NeedUserConfirm
- [ ] **Step 6:** Write test `guardian_subagent_config_lockdown_verified` — verify spawned subagent has `guardian_enabled: false`, `max_iterations: 1`, empty tool registry
- [ ] **Step 7:** Write test `guardian_subagent_cache_key_isolation` — verify subagent prompt-cache key is `guardian:{parent_session_id}`, distinct from parent
- [ ] **Step 8:** Run `cargo test -p synthia-agent` to verify all integration tests pass
- [ ] **Commit point:** "test(agent): add Guardian subagent integration tests"

## Task 10: Formatting, clippy, final verification

- [ ] **Step 1:** Run `cargo +nightly fmt --all` to format all new code
- [ ] **Step 2:** Run `cargo clippy --all-targets --all-features --tests --all` and fix all warnings (per rust.md rule, no `dead_code`/`unused` annotations)
- [ ] **Step 3:** Run `cargo test -p synthia-guardian` — all tests pass
- [ ] **Step 4:** Run `cargo test -p synthia-agent` — all tests pass
- [ ] **Step 5:** Run `cargo test --all` — no regressions across workspace
- [ ] **Commit point:** "chore(guardian): fmt + clippy + final verification for subagent role"
