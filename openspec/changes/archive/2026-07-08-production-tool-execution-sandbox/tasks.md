## 1. Foundation: Create crates and core traits

- [x] 1.1 Create `synthia-tool-orchestrator` crate with `ToolOrchestrator` trait, `ToolCallRequest`, `ToolCallResult`, and `ExecutionContext` types.
- [x] 1.2 Create `synthia-sandbox` crate with `SandboxManager`, `SandboxPolicy`, `SandboxType`, `SandboxAttempt`, and `SandboxError` types.
- [x] 1.3 Define `ApprovalService` trait and `ApprovalOutcome` enum in `synthia-permission` or a new shared location.
- [x] 1.4 Define `ApprovalStore` in-memory implementation with deterministic scope key generation.
- [x] 1.5 Add `synthia-tool-orchestrator` and `synthia-sandbox` to the workspace `Cargo.toml`.
- [x] 1.6 Ensure `cargo check` passes for the two new crates.

## 2. ToolOrchestrator core implementation

- [x] 2.1 Implement `ToolOrchestrator::execute` single-tool flow: discovery → approval check → sandbox selection → execution → result projection.
- [x] 2.2 Implement batch execution `execute_all` with configurable concurrency policy.
- [x] 2.3 Wire cancellation token through `ToolOrchestrator` to terminate child processes.
- [x] 2.4 Implement retry policy with exponential backoff for transient errors.
- [x] 2.5 Emit structured `ToolOrchestratorEvent`s for start, complete, fail, cancel.
- [x] 2.6 Write unit tests for `ToolOrchestrator` using mock tools and mock `ApprovalService`/`SandboxManager`.

## 3. Asynchronous approval lifecycle

- [x] 3.1 Implement `ApprovalService` default headless deny fallback.
- [x] 3.2 Implement CLI `ApprovalService` that prompts in the terminal and awaits user input.
- [x] 3.3 Implement `ApprovalStore` caching for `once`, `always-for-session`, and `reject` decisions.
- [x] 3.4 Integrate `ApprovalService` into `ToolOrchestrator` for `RequireConfirm` and `RequireExplicit` permissions.
- [x] 3.5 Ensure timeout, cancellation, and service-unavailable paths return `Deny`.
- [x] 3.6 Write tests for approval timeout, cache hit, and headless fallback.

## 4. Cross-platform sandbox: Linux backend

- [x] 4.1 Implement `SandboxManager::select` returning `Bubblewrap` on Linux and `Unavailable` on unsupported platforms.
- [x] 4.2 Implement `BubblewrapSandboxAttempt::wrap` to generate `bwrap` command with workspace bind mount and system read-only dirs.
- [x] 4.3 Implement workspace boundary enforcement: deny reads/writes outside workspace except allowed system paths.
- [x] 4.4 Implement sandbox unavailability policy (`deny` default, optional `prompt`).
- [x] 4.5 Integrate `SandboxManager` into `ToolOrchestrator` for `bash` and file editing tools.
- [x] 4.6 Add feature flags `landlock` and `seccomp` with stub backends returning `Unavailable`.
- [x] 4.7 Write integration tests using a test workspace and verify `/etc/passwd` read fails inside sandbox.

## 5. Core file editing tools

- [x] 5.1 Implement `read_file` tool with full read, line range, and encoding detection.
- [x] 5.2 Implement `write_file` tool with temporary file + atomic rename.
- [x] 5.3 Implement `apply_patch` tool with unified-diff parser, hunk validation, and atomic application.
- [x] 5.4 Implement `search_files` tool with glob and literal/regex content search.
- [x] 5.5 Emit `FileChangeEvent` progress events from `apply_patch` per hunk.
- [x] 5.6 Enforce workspace boundary and `external_directory` permission in all file tools.
- [x] 5.7 Project tool results into compact, deterministic context representation.
- [x] 5.8 Replace TODO stubs in `synthia-agent/src/tools/builtins/` with real implementations registered via `ToolOrchestrator`.
- [x] 5.9 Write unit and integration tests for each file tool.

## 6. Integration and cleanup

- [x] 6.1 Route existing `BashTool` invocations through `ToolOrchestrator`.
- [x] 6.2 Route existing MCP tool invocations through `ToolOrchestrator`.
- [x] 6.3 Deprecate `EnhancedToolDispatcher` and `ToolExecutor` duplicate logic; keep minimal shim during transition.
- [x] 6.4 Implement server-side `ApprovalService` over WebSocket/HTTP.
- [x] 6.5 Update `AgentRunConfig` to accept `ApprovalService` and `SandboxManager` dependencies.
- [x] 6.6 Update CLI and server startup to construct and inject `ToolOrchestrator`.
- [x] 6.7 Run `cargo +nightly fmt --all` and `cargo clippy --all-targets --all-features --tests --all` and fix all warnings.
- [x] 6.8 Run full test suite and ensure no regressions.
- [x] 6.9 Update documentation and examples to reflect new tool execution flow.
