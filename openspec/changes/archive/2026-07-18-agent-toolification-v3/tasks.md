## 1. Tool Trait Decomposition (`tool-trait-decomposition`)

- [x] 1.1 Create `ToolDefinition` sub-trait with methods: `name()`, `description()`, `parameters_schema()`, `category()`, `to_metadata()` — expects compile pass with no callers yet
- [x] 1.2 Create `ToolExecution` sub-trait with methods: `execute()`, `validate()`, `dry_run()`, `cost_estimate()`, `cancel()` — expects compile pass with no callers yet
- [x] 1.3 Create `ToolLifecycle` sub-trait with methods: `on_register()`, `on_unregister()`, `health_check()`, `version()`, `schema_version()` — expects compile pass with no callers yet
- [x] 1.4 Add `ToolV1` alias aggregating the three sub-traits in `synthia-tools` crate root
- [x] 1.5 Update 5 existing Tool implementations to implement all three sub-traits (one PR per impl if needed)
- [x] 1.6 Add unit tests asserting each sub-trait exposes ≤ 5 methods (compile-time check via trait method count)

## 2. AgentMessage View Abstraction (`agent-message-view`)

- [x] 2.1 Add `MessageKind` enum with variants `System`, `User`, `Assistant`, `ToolCall`, `ToolResult` in `synthia-agent`
- [x] 2.2 Add `kind() -> MessageKind` accessor on `AgentMessage`
- [x] 2.3 Add `fn llm_visible(&self) -> bool` method on `AgentMessage` with default impl based on `MessageKind`
- [x] 2.4 Override `llm_visible()` for tool messages to return `false` when tool result is empty
- [x] 2.5 Add benchmark test asserting `llm_visible()` runs in < 1ms over 10k messages
- [x] 2.6 Document `O(1)` and side-effect-free contract in doc-comment on `llm_visible()`

## 3. ToolRegistry Dual-Index (`tool-registry-dual-index`)

- [x] 3.1 Define `ToolMetadata` struct with fields: `name`, `description`, `category`, `parameters_schema`, `version` — derive `Clone + Send + Sync`
- [x] 3.2 Define `ToolCategory` enum (categories to be enumerated during PR; placeholder: `FileSystem`, `Network`, `Computation`, `Agent`, `Other`)
- [x] 3.3 Refactor `ToolRegistry` internal storage to `HashMap<String, Arc<dyn ToolDefinition + ToolExecution + ToolLifecycle>>` + `Vec<ToolMetadata>`
- [x] 3.4 Implement `ToolRegistry::insert` to atomically update both indices
- [x] 3.5 Implement `ToolRegistry::remove` to atomically clean both indices
- [x] 3.6 Implement `ToolRegistry::snapshot() -> Vec<ToolMetadata>` returning cloned metadata
- [x] 3.7 Add tests asserting insertion order preservation in `Vec<ToolMetadata>`

## 4. Provider Trait Abstraction (`provider-trait`)

- [x] 4.1 Define `Provider` trait with methods: `complete()`, `stream()`, `model_id()`, `capabilities()` in `synthia-llm`
- [x] 4.2 Define `ProviderCapabilities` struct with fields: `supports_streaming`, `supports_tool_calls`, `supports_vision`, `max_context_tokens`
- [x] 4.3 Define `CompletionRequest`, `CompletionResponse`, `Chunk` types (minimal, can be re-exported from existing LLM types)
- [x] 4.4 Refactor `Agent` to hold `Arc<dyn Provider>` instead of concrete `LlmClient`
- [x] 4.5 Implement capability-aware code paths: skip `stream()` if `!supports_streaming`
- [x] 4.6 Verify existing tests pass with Provider-trait-based Agent

## 5. CompressionTool Abstraction (`compression-tool`)

- [x] 5.1 Define `CompressionTool` trait with `fn compress(&self, history: &[AgentMessage]) -> Result<CompressionResult>`
- [x] 5.2 Define `CompressionResult` type (minimal: `messages: Vec<AgentMessage>` + `summary: String`)
- [x] 5.3 Implement `DefaultCompactionTool` that reproduces existing inline `compress()` behavior byte-equivalently
- [x] 5.4 Add `Agent::builder().compression(Arc<dyn CompressionTool>)` builder method
- [x] 5.5 Default compression to `Arc::new(DefaultCompactionTool::new())` when not specified
- [x] 5.6 Update `react_loop` to call injected `CompressionTool` instead of inline `compress()`
- [x] 5.7 Add test asserting default behavior matches legacy inline `compress()` for 100-message input

## 6. ToolPermission Trait (`tool-permission`)

- [x] 6.1 Define `ToolPermission` trait with `fn check(&self, ctx: &PermissionContext) -> PermissionDecision`
- [x] 6.2 Define `PermissionDecision` enum with `Allow`, `Deny(String)`, `Ask` variants
- [x] 6.3 Define `PermissionContext` struct with `tool_name`, `arguments`, `agent_run_id`, `user_id` fields
- [x] 6.4 Implement `PermissionAlwaysAllow` default (always returns `Allow`)
- [x] 6.5 Add permission check in `ToolExecution::execute()` returning `Err(ToolError::PermissionDenied)` on `Deny`
- [x] 6.6 Add Ask-path resolution mechanism (UI hook or callback to be defined in implementation)

## 7. AgentTool Factory Wiring (`agent-tool-wiring`)

- [x] 7.1 Locate `AgentTool` factory function in `synthia-tools`
- [x] 7.2 Wire factory invocation into `Agent::builder().build()` so `AgentTool` is registered as `"agent"` entry
- [x] 7.3 Add test asserting `Agent::registry().get("agent")` returns `Some` after `build()`
- [x] 7.4 Verify existing `AgentTool` tests still pass unchanged

## 8. AgentRunConfig Field Cleanup (`config-field-cleanup`)

- [x] 8.1 Audit `AgentRunConfig` and produce a list of all `_xxx` fields with their current state (read / dropped / renamed target)
- [x] 8.2 For each deleted field: add CHANGELOG entry documenting removal
- [x] 8.3 For each renamed field: add deprecated alias with `#[deprecated(note = "use <new_name>")]` pointing to new name
- [x] 8.4 Add unit test scanning `AgentRunConfig` for `_` prefix on fields and failing the build if found without justification
- [x] 8.5 Verify `cargo +nightly fmt --all` and `cargo clippy --all-targets --all-features --tests --all` are clean
- [x] 8.6 Verify `cargo test --all` and `cargo miri test` (if applicable) pass

## 9. Validation & Release Prep

- [x] 9.1 Run full CI pipeline: `cargo +nightly fmt --all` → `cargo clippy --all-targets --all-features --tests --all` → `cargo test --all` → `cargo miri test`
- [x] 9.2 Update CHANGELOG.md with consolidated entry for `agent-toolification-v3`
- [x] 9.3 Verify each spec's `#### Scenario` blocks have corresponding test cases (or document deferral)
- [x] 9.4 Open draft PR linking `openspec/changes/agent-toolification-v3/` and request review
- [x] 9.5 Address review feedback and merge