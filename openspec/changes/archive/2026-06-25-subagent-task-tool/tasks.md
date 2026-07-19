## 1. Inject AgentControl into AgentRunConfig construction paths

- [x] 1.1 Add `AgentControl` to `AgentFactory` dependencies and inject it into `AgentFactory::create`.
- [x] 1.2 Inject `AgentControl` in `SessionController::build_run_config`.
- [x] 1.3 Inject `AgentControl` in `Agent::resume` and `Agent::run_stream` CLI paths.
- [x] 1.4 Update tests that construct `AgentRunConfig` to provide `AgentControl` where needed.

## 2. Implement subagent permission inheritance

- [x] 2.1 Create `crates/synthia-agent/src/subagent/permission.rs` with `derive_subagent_permission()`.
- [x] 2.2 Add unit tests for deny-only inheritance and default-deny `task`/`todowrite`.
- [x] 2.3 Expose `derive_subagent_permission` through `synthia-agent/src/subagent/mod.rs`.

## 3. Apply ForkPolicy in subagent configuration

- [x] 3.1 Refactor `build_subagent_config` to accept parent messages and `ForkPolicy`.
- [x] 3.2 Apply `apply_fork_policy` to filter child initial messages.
- [x] 3.3 Wire derived permissions into the child `AgentRunConfig` approval service.
- [x] 3.4 Add unit tests for each `ForkPolicy` variant.

## 4. Align AgentTool parameter schema with Opencode

- [x] 4.1 Replace `agent_id` with `subagent_type` in `AgentTool` input.
- [x] 4.2 Add `description`, `background`, and `task_id` parameters.
- [x] 4.3 Update `AgentTool::call` to handle `task_id` resumption.
- [x] 4.4 Update existing `AgentTool` tests to use the new parameter schema.

## 5. Register AgentTool conditionally in ToolRegistry

- [x] 5.1 Change `build_default_tool_registry` signature to accept optional `AgentControl` and `SubagentSessionFactory`.
- [x] 5.2 Register `AgentTool` only when both dependencies are present.
- [x] 5.3 Build dynamic tool description listing available subagent types.
- [x] 5.4 Update all call sites of `build_default_tool_registry` to pass the new arguments.
- [x] 5.5 Add tests verifying presence/absence of `task` tool based on dependencies.

## 6. Implement built-in subagent types

- [x] 6.1 Define `general` subagent type with broad read/write tool access and default-deny `task`/`todowrite`.
- [x] 6.2 Define `explore` subagent type with read-only tool access and deny `bash`/`write`/`task`/`todowrite`.
- [x] 6.3 Ensure `RegisterAgent` rejects reserved built-in identifiers.
- [x] 6.4 Add tests for built-in type registration and permission sets.

## 7. Enable background subagent execution

- [x] 7.1 Conditionally include `background` in the `task` tool schema only when `AgentControl` is present.
- [x] 7.2 When `background` is true, spawn via `tokio::spawn` and register the handle with `AgentControl::register_background_task`.
- [x] 7.3 Return an immediate background-start response to the LLM.
- [x] 7.4 Add tests for background launch and `AgentControl::check_completed`.

## 8. Improve background completion notifications

- [x] 8.1 Capture actual subagent output in `CompletedTask` from `AgentControl::check_completed`.
- [x] 8.2 Update main-loop polling to inject structured `<task>` XML with output.
- [x] 8.3 Distinguish `completed` vs `error` states.
- [x] 8.4 Add integration test verifying end-to-end background task completion notification.

## 9. Verify and finalize

- [x] 9.1 Run `cargo +nightly fmt --all`.
- [x] 9.2 Run `cargo clippy --all-targets --all-features --tests --all` and fix warnings.
- [x] 9.3 Run `cargo test --workspace` and fix failures.
- [x] 9.4 Run `openspec validate --all`.
