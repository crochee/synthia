# Implementation Tasks

## 1. P0: Tool Cancellation Propagation

### 1.1 Tool Trait Changes
- [ ] 1.1.1 Add `CancellationToken` parameter to `synthia_tool::Tool::call_with_sandbox()` trait signature
- [ ] 1.1.2 Add `CancellationToken` parameter to `synthia_tool::Tool::call_with_progress()` trait signature
- [ ] 1.1.3 Add `ToolError::Cancelled` variant to `ToolError` enum
- [ ] 1.1.4 Update all built-in tool implementations to new signature

### 1.2 ToolAdapter Fixes
- [ ] 1.2.1 Fix `ToolAdapter::execute()` to pass `cancellation_token` (remove underscore prefix)
- [ ] 1.2.2 Fix `ToolAdapter::execute_with_events()` to propagate cancellation token
- [ ] 1.2.3 Verify `ToolOrchestrator::execute()` passes token through to `ToolAdapter`

### 1.3 Built-in Tool Yield Points
- [ ] 1.3.1 Add yield points to `ReadTool::call_with_sandbox()` for large files (chunk at 64KB)
- [ ] 1.3.2 Add yield points to `WriteTool::call_with_sandbox()` for large writes (chunk at 64KB)
- [ ] 1.3.3 Add yield points to `GlobTool::call_with_sandbox()` between directory levels
- [ ] 1.3.4 Add yield points to `GrepTool::call_with_sandbox()` between files
- [ ] 1.3.5 Add yield points to `BashTool::call_with_sandbox()` for long-running commands

### 1.4 Registry Path
- [ ] 1.4.1 Update `execute_via_registry()` to pass `cancel_token` through registry context
- [ ] 1.4.2 Verify `ToolRegistry::run_with_context()` propagates token to tool execution

## 2. P1: Async Permission Deferred

### 2.1 PermissionFuture Type
- [ ] 2.1.1 Create `PermissionFuture` struct wrapping `tokio::sync::oneshot::Receiver`
- [ ] 2.1.2 Implement `Future` trait for `PermissionFuture`
- [ ] 2.1.3 Add `PermissionFutureError` enum with `Cancelled`, `Denied`, `Dropped` variants
- [ ] 2.1.4 Add `await_with_cancellation()` method
- [ ] 2.1.5 Add `immediate_granted()` and `immediate_denied()` constructors

### 2.2 PermissionService Async Interface
- [ ] 2.2.1 Add `ask(&self, request: PermissionRequest) -> PermissionFuture` to `PermissionService` trait
- [ ] 2.2.2 Keep `check()` for sync approval (backward compat)
- [ ] 2.2.3 Add `reply()` method to resolve pending permission futures

### 2.3 HeadlessApprovalService Async
- [ ] 2.3.1 Implement `ask()` returning immediately resolved `PermissionFuture` with denied
- [ ] 2.3.2 Verify backward compatibility with existing sync `check()` callers

### 2.4 TUI Approval Async
- [ ] 2.4.1 Update TUI approval to return `PermissionFuture` from `ask()`
- [ ] 2.4.2 Wire up future resolution to user button handlers (Grant/Deny/Always)
- [ ] 2.4.3 Add "always" persistence

### 2.5 Orchestrator Integration
- [ ] 2.5.1 Update `DefaultToolOrchestrator` to use async `permission_service.ask()`
- [ ] 2.5.2 Await `PermissionFuture` before executing tools
- [ ] 2.5.3 Handle `PermissionFutureError::Cancelled` → `ToolOrchestratorError::Cancelled`

## 3. P1: Scoped Tool Registry

### 3.1 ScopedToolRegistry Core
- [ ] 3.1.1 Create `scoped_registry.rs` module in `synthia-tool`
- [ ] 3.1.2 Define `Token = Arc<()>` for unique scope identity
- [ ] 3.1.3 Implement `ScopedToolRegistry` struct with local + global registry
- [ ] 3.1.4 Add `register_scoped(tools, token)` method

### 3.2 ScopeGuard RAII
- [ ] 3.2.1 Create `ScopeGuard` struct holding `Arc<Mutex<ScopeState>>`
- [ ] 3.2.2 Implement `Drop` for `ScopeGuard` to auto-deregister
- [ ] 3.2.3 Add `create_scope()` factory returning `(Arc<ScopedToolRegistry>, ScopeGuard)`

### 3.3 Materialize with Last-Wins
- [ ] 3.3.1 Implement `materialize()` with scoped overriding global
- [ ] 3.3.2 Thread-safety using `RwLock`
- [ ] 3.3.3 Multiple scopes: most recent registration wins

### 3.4 Integration
- [ ] 3.4.1 Wire scoped registry to session lifecycle
- [ ] 3.4.2 Test per-session tool isolation

## 4. P1: Proactive Doom-Loop Detection

### 4.1 DoomLoopDetector Core
- [ ] 4.1.1 Create `doom_loop_detector.rs` in `synthia-guardian`
- [ ] 4.1.2 Define `ToolCallSignature` with `tool_name` and `input_hash`
- [ ] 4.1.3 Implement sliding window with `VecDeque<ToolCallSignature>`
- [ ] 4.1.4 Add `check(tool_name, args) -> (LoopStatus, Option<LoopAction>)`

### 4.2 Hash-based Signatures
- [ ] 4.2.1 Use `xxhash64` for fast hashing of `(tool_name, JSON.stringify(args))`
- [ ] 4.2.2 Fallback full comparison on hash collision
- [ ] 4.2.3 Configurable threshold via `AgentConfig.doom_loop_threshold`

### 4.3 Guardian Integration
- [ ] 4.3.1 Add `DoomLoopDetector` alongside `GuardianCircuitBreaker`
- [ ] 4.3.2 Wire `RequirePermission` action to `permission.ask(doom_loop, ...)`
- [ ] 4.3.3 Fallback to `LoopStatus::Detected` if caller ignores action

### 4.4 Tests
- [ ] 4.4.1 Unit: 3 identical calls triggers detection
- [ ] 4.4.2 Unit: different args resets window
- [ ] 4.4.3 Unit: different tool resets window
- [ ] 4.4.4 Integration: permission ask called on detection

## 5. P1: Smart Compaction Agent

### 5.1 Token Selection (Backward Walk)
- [ ] 5.1.1 Extend `synthia-context` with `select_tokens(entries, keep_tokens)` method
- [ ] 5.1.2 Walk backward from most recent message
- [ ] 5.1.3 Split overflowing message preserving suffix in `recent`
- [ ] 5.1.4 Filter out prior `compaction` messages

### 5.2 LLM Summarization
- [ ] 5.2.1 Add `summarize(model, previous_summary, head) -> String` method
- [ ] 5.2.2 Use same model as main agent, no tools, 4K output cap
- [ ] 5.2.3 Build summary prompt using template (Goal/Progress/Decisions/Next Steps)
- [ ] 5.2.4 Handle failure: fallback to truncation
- [ ] 5.2.5 Empty summary: abandon compaction

### 5.3 Incremental Summary Chaining
- [ ] 5.3.1 Include previous summary in prompt for subsequent compactions
- [ ] 5.3.2 Chain builds over multiple compaction events

### 5.4 Compaction Message
- [ ] 5.4.1 Create `compaction` message type with `text` and `recent`
- [ ] 5.4.2 Insert after successful compaction
- [ ] 5.4.3 One-shot recovery: fail hard if overflow after compaction

### 5.5 Configuration
- [ ] 5.5.1 Add `ContextConfig.compaction_buffer` (default 20,000)
- [ ] 5.5.2 Add `ContextConfig.keep_tokens` (default 8,000)
- [ ] 5.5.3 Add `AgentConfig.doom_loop_threshold` (default 3)

## 6. Integration & Testing

### 6.1 Compilation
- [ ] 6.1.1 Verify `cargo build -p synthia-tool -p synthia-tool-orchestrator -p synthia-permission -p synthia-guardian -p synthia-context`
- [ ] 6.1.2 Run `cargo clippy --all-targets --all-features --tests`

### 6.2 Unit Tests
- [ ] 6.2.1 Test cancellation propagation through tool chain
- [ ] 6.2.2 Test PermissionFuture await and cancellation
- [ ] 6.2.3 Test ScopedToolRegistry scope cleanup
- [ ] 6.2.4 Test DoomLoopDetector sliding window
- [ ] 6.2.5 Test SmartCompaction backward token selection

### 6.3 E2E Tests
- [ ] 6.3.1 E2E: tool cancellation mid-execution
- [ ] 6.3.2 E2E: doom-loop triggers permission prompt
- [ ] 6.3.3 E2E: compaction generates coherent summary
