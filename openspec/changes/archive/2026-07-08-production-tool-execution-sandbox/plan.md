# Production Tool Execution & Sandbox Implementation Plan

> **For agentic workers:** Use `subagent-driven-development` or `software-engineering-executor` to implement this plan task-by-task.

**Goal:** Establish a production-grade execution底盘 for Synthia by introducing a unified `ToolOrchestrator`, asynchronous approval lifecycle, Linux sandbox backend, and core file-editing tools, while preserving P1-P10 design principles.

**Architecture:** A new `synthia-tool-orchestrator` crate becomes the single entry point for all tool calls, delegating to `ApprovalService` and `SandboxManager` before executing the underlying tool. A new `synthia-sandbox` crate provides cross-platform abstraction with a Linux bubblewrap backend. File editing tools are implemented atomically and emit progress events.

**Tech Stack:** Rust, Cargo workspace, tokio, bubblewrap (`bwrap`), optional landlock/seccomp features.

---

## Task 1: Create crates and core traits

**Goal:** Scaffold the two new crates and define the trait boundaries.

- [x] **Step 1.1:** Create directory `crates/synthia-tool-orchestrator/` with `Cargo.toml`, `src/lib.rs`, and expose `ToolOrchestrator`, `ToolCallRequest`, `ToolCallResult`, `ExecutionContext`, `ToolOrchestratorEvent`.
- [x] **Step 1.2:** Create directory `crates/synthia-sandbox/` with `Cargo.toml`, `src/lib.rs`, and expose `SandboxManager`, `SandboxPolicy`, `SandboxType`, `SandboxAttempt`, `SandboxError`.
- [x] **Step 1.3:** In `synthia-permission/src/approval/` add `ApprovalService` trait and `ApprovalOutcome` enum with variants `Approve`, `Deny`, `Defer`.
- [x] **Step 1.4:** In `synthia-permission/src/approval/` add `ApprovalStore` with `get(tool, args, scope)` and `set(...)` APIs backed by `DashMap` or `Mutex<HashMap>`.
- [x] **Step 1.5:** Add both crates to root `Cargo.toml` workspace members.
- [x] **Step 1.6:** Run `cargo check -p synthia-tool-orchestrator -p synthia-sandbox` and fix errors.

**Commit point:** `feat: scaffold tool-orchestrator and sandbox crates`

---

## Task 2: Implement ToolOrchestrator core flow

**Goal:** Build the unified execution pipeline.

- [ ] **Step 2.1:** Implement `ToolOrchestrator::execute(ctx: ExecutionContext) -> Result<ToolCallResult, ToolOrchestratorError>` with stages: discover → evaluate permission → request approval if needed → select sandbox → run → project result.
- [ ] **Step 2.2:** Implement `ToolOrchestrator::execute_all(requests, concurrency)` using `FuturesUnordered` or semaphore; honor per-tool concurrency hints.
- [ ] **Step 2.3:** Accept a `CancellationToken` in `ExecutionContext` and propagate it to the spawned command via `tokio::process::Child::kill` on cancellation.
- [ ] **Step 2.4:** Add retry logic in `execute` for errors tagged as `ToolErrorKind::Transient` using exponential backoff (`tokio::time::sleep`).
- [ ] **Step 2.5:** Define `ToolOrchestratorEvent` variants (`Started`, `Completed`, `Failed`, `Cancelled`) and emit them through an `EventSender`.
- [ ] **Step 2.6:** Write tests in `synthia-tool-orchestrator/src/tests.rs` using mock implementations.

**Commit point:** `feat: implement ToolOrchestrator execution pipeline`

---

## Task 3: Implement asynchronous approval lifecycle

**Goal:** Make `RequireConfirm` a real waiting path.

- [ ] **Step 3.1:** Implement `HeadlessApprovalService` that always returns `ApprovalOutcome::Deny`.
- [ ] **Step 3.2:** Implement `TerminalApprovalService` that prints the tool call, waits for `y/n/always` on stdin, and maps to outcomes.
- [ ] **Step 3.3:** In `ApprovalStore`, implement scope key as hash of `(tool_name, normalized_args, workspace_root)`; support `Once`, `AlwaysSession`, `Reject`.
- [ ] **Step 3.4:** In `ToolOrchestrator::execute`, call `approval_service.request_approval(...).await` when permission is `RequireConfirm` or `RequireExplicit`; deny on timeout/cancel/error.
- [ ] **Step 3.5:** Add unit tests for timeout (use a service that never resolves with a short timeout) and cache hit.
- [ ] **Step 3.6:** Run `cargo test -p synthia-tool-orchestrator -p synthia-permission`.

**Commit point:** `feat: async approval service with session-scoped cache`

---

## Task 4: Implement Linux bubblewrap sandbox

**Goal:** Provide OS-level isolation for command execution.

- [ ] **Step 4.1:** In `synthia-sandbox/src/manager.rs`, implement `SandboxManager::select(policy)` that returns `Bubblewrap` on Linux when `bwrap` is in PATH, else `Unavailable`.
- [ ] **Step 4.2:** In `synthia-sandbox/src/linux/bwrap.rs`, implement `BubblewrapAttempt::wrap(command)` that prefixes the command with `bwrap --bind workspace /workspace --chdir /workspace --ro-bind /usr /usr --ro-bind /bin /bin --proc /proc --dev /dev ... command`.
- [ ] **Step 4.3:** Add validation that any path argument is inside workspace; reject otherwise before wrapping.
- [ ] **Step 4.4:** Add `SandboxPolicy::OnUnavailable` enum (`Deny`, `Prompt`) and default to `Deny`.
- [ ] **Step 4.5:** Wire `SandboxManager` into `ToolOrchestrator` so `bash` invocations are sandboxed by default.
- [ ] **Step 4.6:** Add feature-gated stubs for `landlock` and `seccomp` backends.
- [ ] **Step 4.7:** Write integration test that runs `cat /etc/passwd` inside sandbox and asserts failure.

**Commit point:** `feat: linux bubblewrap sandbox backend`

---

## Task 5: Implement core file editing tools

**Goal:** Replace TODO stubs with real tools.

- [ ] **Step 5.1:** In `synthia-agent/src/tools/builtins/read_file.rs`, implement full read and line range read; detect UTF-8/UTF-8 BOM.
- [ ] **Step 5.2:** In `synthia-agent/src/tools/builtins/write_file.rs`, write to `.synthia/tmp/<uuid>` then `fs::rename` to target.
- [ ] **Step 5.3:** In `synthia-agent/src/tools/builtins/apply_patch.rs`, parse unified diff hunks, validate context, apply atomically; emit `FileChangeEvent` per hunk.
- [ ] **Step 5.4:** In `synthia-agent/src/tools/builtins/search_files.rs`, use `glob` crate for patterns and `grep` crate or custom scan for content.
- [ ] **Step 5.5:** Add `FileChangeEvent` enum and emit it via the orchestrator event sender.
- [ ] **Step 5.6:** Enforce workspace boundary in each tool by resolving and checking absolute paths against `workspace_root`.
- [ ] **Step 5.7:** Project results into compact strings with deterministic formatting before returning.
- [ ] **Step 5.8:** Register new tools in `synthia-agent/src/tool_registry.rs` and remove TODO stubs.
- [ ] **Step 5.9:** Add unit tests for each tool in `synthia-agent/src/tools/builtins/tests.rs`.

**Commit point:** `feat: core file editing tools`

---

## Task 6: Integration, cleanup, and verification

**Goal:** Wire everything together and remove old duplicate paths.

- [ ] **Step 6.1:** Update `synthia-tool/src/registry/registration/registry.rs` to delegate execution to `ToolOrchestrator` instead of running tools directly.
- [ ] **Step 6.2:** Update MCP handler to route MCP tool calls through `ToolOrchestrator`.
- [ ] **Step 6.3:** Remove or deprecate duplicate logic in `EnhancedToolDispatcher` and `ToolExecutor`; keep thin shims if needed for backward compatibility.
- [ ] **Step 6.4:** Implement `WebSocketApprovalService` in `synthia-server` (or a new `synthia-server-approval` module).
- [ ] **Step 6.5:** Add `approval_service` and `sandbox_manager` fields to `AgentRunConfig` in `synthia-agent/src/config/agent_config/run_config.rs`.
- [ ] **Step 6.6:** Update `synthia-cli/src/main.rs` and `synthia-server/src/server.rs` to construct and inject dependencies.
- [ ] **Step 6.7:** Run `cargo +nightly fmt --all` then `cargo clippy --all-targets --all-features --tests --all`; fix all warnings.
- [ ] **Step 6.8:** Run `cargo test --workspace` and fix regressions.
- [ ] **Step 6.9:** Update `docs/` examples and README if applicable.

**Commit point:** `feat: integrate orchestrator, approval, sandbox, and file tools`

---

## Verification commands

After each major commit:
```bash
cargo check -p synthia-tool-orchestrator -p synthia-sandbox -p synthia-agent
cargo test -p synthia-tool-orchestrator -p synthia-sandbox -p synthia-agent
```

Final verification:
```bash
cargo +nightly fmt --all
cargo clippy --all-targets --all-features --tests --all
cargo test --workspace
```

## Rollback strategy

- Each task commit should keep existing tests passing.
- `ToolOrchestrator` can be feature-flagged (`tool-orchestrator-v2`) during transition; old path remains as fallback until final cleanup commit.
- If sandbox backend is unavailable, default `Deny` prevents accidental unsandboxed execution.
