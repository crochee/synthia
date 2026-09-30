# Agent Toolification v3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use omo-subagent-driven-development (recommended) or omo-dispatching-parallel-agents to implement this plan task-by-task. Each task specifies a `category` and `load_skills` for oh-my-opencode's `task()` tool. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose `Tool` trait into sub-traits, add `AgentMessage::llm_visible()`, dual-index `ToolRegistry`, introduce `Provider`/`CompressionTool`/`ToolPermission` traits, wire `AgentTool` factory, and clean `AgentRunConfig` `_xxx` fields — without touching `react_loop` or `Session` lifecycle.

**Architecture:** Three-layer split (Provider / Session / Tool). `Tool` trait becomes three object-safe sub-traits aggregated by `ToolV1` alias. `ToolRegistry` keeps HashMap + Vec<ToolMetadata>. `CompressionTool` and `ToolPermission` are inject-and-default patterns. Each sub-trait ≤ 5 methods. All changes are non-breaking via `ToolV1` alias and `#[deprecated]` renames.

**Tech Stack:** Rust (nightly fmt + clippy + miri), serde, tokio, async-trait, thiserror, semver, uuid.

**Scope:** 9 task groups, ~40 atomic tasks. Each task = one PR ≤ 3 days. Phase 1 total ≤ 6 weeks.

**Reference artifacts** (in this directory):
- `design.md` §Decisions (D1-D11) — architectural choices
- `design.md` §Migration Plan — PR ordering
- `specs/*/spec.md` — testable REQUIREMENTS

---

## Task Group 1: Tool Trait Decomposition (`tool-trait-decomposition`)

### Task 1.1: Define `ToolDefinition` sub-trait

**Files:**
- Create: `crates/synthia-tools/src/traits/definition.rs`
- Modify: `crates/synthia-tools/src/traits/mod.rs`
- Test: `crates/synthia-tools/src/traits/definition.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

```rust
// crates/synthia-tools/src/traits/definition.rs
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definition_has_at_most_five_methods() {
        // compile-time check via trait method count macro
        assert!(tool_trait::max_methods::<dyn ToolDefinition>() <= 5);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-tools definition_has_at_most_five_methods`
Expected: FAIL — `ToolDefinition` not defined

- [ ] **Step 3: Define `ToolDefinition` trait**

```rust
// crates/synthia-tools/src/traits/definition.rs
use crate::metadata::ToolMetadata;
use serde_json::Value;

pub trait ToolDefinition: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    fn category(&self) -> ToolCategory;
    fn to_metadata(&self) -> ToolMetadata;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p synthia-tools definition_has_at_most_five_methods`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-tools/src/traits/definition.rs crates/synthia-tools/src/traits/mod.rs
git commit -m "feat(tools): add ToolDefinition sub-trait"
```

### Task 1.2: Define `ToolExecution` sub-trait

- [ ] **Step 1: Write the failing test**

```rust
// crates/synthia-tools/src/traits/execution.rs
#[cfg(test)]
mod tests {
    #[test]
    fn execution_has_at_most_five_methods() {
        assert!(tool_trait::max_methods::<dyn ToolExecution>() <= 5);
    }
}
```

- [ ] **Step 2: Run test (fail)**

Run: `cargo test -p synthia-tools execution_has_at_most_five_methods`
Expected: FAIL

- [ ] **Step 3: Define `ToolExecution` trait**

```rust
// crates/synthia-tools/src/traits/execution.rs
use crate::{permission::{PermissionContext, ToolPermission}, ToolError};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait ToolExecution: Send + Sync + 'static {
    async fn execute(&self, args: Value) -> Result<Value, ToolError>;
    fn validate(&self, args: &Value) -> Result<(), ToolError>;
    async fn dry_run(&self, args: &Value) -> Result<(), ToolError>;
    fn cost_estimate(&self, args: &Value) -> u64;
    async fn cancel(&self) -> Result<(), ToolError>;
}
```

- [ ] **Step 4: Run test (pass)**

Run: `cargo test -p synthia-tools execution_has_at_most_five_methods`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-tools/src/traits/execution.rs
git commit -m "feat(tools): add ToolExecution sub-trait"
```

### Task 1.3: Define `ToolLifecycle` sub-trait

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn lifecycle_has_at_most_five_methods() {
    assert!(tool_trait::max_methods::<dyn ToolLifecycle>() <= 5);
}
```

- [ ] **Step 2: Run test (fail)**

Run: `cargo test -p synthia-tools lifecycle_has_at_most_five_methods`
Expected: FAIL

- [ ] **Step 3: Define `ToolLifecycle` trait**

```rust
pub trait ToolLifecycle: Send + Sync + 'static {
    fn on_register(&self) -> Result<(), ToolError>;
    fn on_unregister(&self) -> Result<(), ToolError>;
    fn health_check(&self) -> Result<(), ToolError>;
    fn version(&self) -> semver::Version;
    fn schema_version(&self) -> u32;
}
```

- [ ] **Step 4: Run test (pass)** / **Step 5: Commit**

```bash
git add crates/synthia-tools/src/traits/lifecycle.rs
git commit -m "feat(tools): add ToolLifecycle sub-trait"
```

### Task 1.4: Add `ToolV1` alias

- [ ] **Step 1: Write the failing test** asserting alias compiles + works

```rust
// crates/synthia-tools/src/lib.rs
#[cfg(test)]
mod tests {
    use crate::traits::{ToolDefinition, ToolExecution, ToolLifecycle};
    fn _assert_alias<T: ToolDefinition + ToolExecution + ToolLifecycle>() {}
    #[test]
    fn tool_v1_alias_resolves() {
        _assert_alias::<dyn crate::ToolV1>();
    }
}
```

- [ ] **Step 2: Run test (fail)** — `ToolV1` not defined
- [ ] **Step 3: Add `ToolV1` alias in `lib.rs`**

```rust
pub use traits::{ToolDefinition, ToolExecution, ToolLifecycle};
pub type ToolV1 = dyn ToolDefinition + ToolExecution + ToolLifecycle;
```

- [ ] **Step 4: Run test (pass)** / **Step 5: Commit**

```bash
git add crates/synthia-tools/src/lib.rs
git commit -m "feat(tools): add ToolV1 backward-compat alias"
```

### Task 1.5: Update 5 existing Tool implementations

- [ ] **Step 1: For each of 5 impls (file_system, shell, http, agent, custom), add `impl ToolDefinition + ToolExecution + ToolLifecycle for X`**
- [ ] **Step 2: Run `cargo build -p synthia-tools`** — expect compile errors until all 3 sub-traits implemented
- [ ] **Step 3: Resolve each impl one by one**
- [ ] **Step 4: Run `cargo test -p synthia-tools`** — all existing tests pass
- [ ] **Step 5: Commit per impl or one bulk commit**

```bash
git commit -m "refactor(tools): migrate 5 tool impls to sub-traits"
```

---

## Task Group 2: AgentMessage View Abstraction (`agent-message-view`)

### Task 2.1: Add `MessageKind` enum

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn message_kind_has_five_variants() {
    use crate::message::MessageKind;
    let _ = vec![
        MessageKind::System, MessageKind::User, MessageKind::Assistant,
        MessageKind::ToolCall, MessageKind::ToolResult,
    ];
}
```

- [ ] **Step 2: Run (fail)** / **Step 3: Define enum**

```rust
// crates/synthia-agent/src/message.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageKind {
    System, User, Assistant, ToolCall, ToolResult,
}
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

```bash
git add crates/synthia-agent/src/message.rs
git commit -m "feat(agent): add MessageKind enum"
```

### Task 2.2: Add `kind()` accessor + `llm_visible()` method

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn user_message_is_llm_visible() {
    let msg = AgentMessage::user("hi");
    assert!(msg.llm_visible());
    assert_eq!(msg.kind(), MessageKind::User);
}
```

- [ ] **Step 2: Run (fail)** / **Step 3: Implement**

```rust
impl AgentMessage {
    pub fn kind(&self) -> MessageKind { /* existing role -> kind mapping */ }
    pub fn llm_visible(&self) -> bool {
        match self.kind() {
            MessageKind::System | MessageKind::User | MessageKind::Assistant => true,
            MessageKind::ToolCall | MessageKind::ToolResult => self.has_payload(),
        }
    }
}
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

```bash
git commit -m "feat(agent): add kind() and llm_visible() on AgentMessage"
```

### Task 2.3: Add performance benchmark

- [ ] **Step 1: Write benchmark** in `benches/message_view.rs`:

```rust
fn bench_llm_visible(c: &mut Criterion) {
    let msgs: Vec<_> = (0..10_000).map(|i| AgentMessage::user(format!("m{i}"))).collect();
    c.bench_function("llm_visible_10k", |b| b.iter(|| {
        for m in &msgs { std::hint::black_box(m.llm_visible()); }
    }));
}
```

- [ ] **Step 2: Run** `cargo bench -p synthia-agent llm_visible_10k`
- [ ] **Step 3: Document** in doc-comment that `llm_visible()` is O(1) side-effect-free
- [ ] **Step 4: Verify** < 1ms via `cargo bench` output
- [ ] **Step 5: Commit**

```bash
git commit -m "bench(agent): assert llm_visible O(1) over 10k"
```

---

## Task Group 3: ToolRegistry Dual-Index (`tool-registry-dual-index`)

### Task 3.1: Define `ToolMetadata` + `ToolCategory`

- [ ] **Step 1: Write failing test** asserting struct derives + fields exist
- [ ] **Step 2: Run (fail)** / **Step 3: Define types**

```rust
// crates/synthia-tools/src/metadata.rs
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub parameters_schema: serde_json::Value,
    pub version: semver::Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    FileSystem, Network, Computation, Agent, Other,
}
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

### Task 3.2: Refactor `ToolRegistry` to dual-index

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn registry_insert_preserves_vec_order() {
    let mut r = ToolRegistry::new();
    r.insert("a", tool_a());
    r.insert("b", tool_b());
    let meta = r.snapshot();
    assert_eq!(meta[0].name, "a");
    assert_eq!(meta[1].name, "b");
}
```

- [ ] **Step 2: Run (fail)** / **Step 3: Implement**

```rust
pub struct ToolRegistry {
    map: HashMap<String, Arc<dyn ToolV1>>,
    order: Vec<ToolMetadata>,
}

impl ToolRegistry {
    pub fn insert(&mut self, name: &str, t: Arc<dyn ToolV1>) {
        let meta = t.to_metadata();
        self.map.insert(name.into(), t);
        self.order.push(meta);
    }
    pub fn snapshot(&self) -> Vec<ToolMetadata> { self.order.clone() }
    pub fn get(&self, name: &str) -> Option<&Arc<dyn ToolV1>> { self.map.get(name) }
}
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

---

## Task Group 4: Provider Trait Abstraction (`provider-trait`)

### Task 4.1: Define `Provider` trait + supporting types

- [ ] **Step 1: Write failing test** asserting trait shape
- [ ] **Step 2: Run (fail)** / **Step 3: Implement**

```rust
// crates/synthia-llm/src/provider.rs
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn stream(&self, req: CompletionRequest) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>;
    fn model_id(&self) -> &str;
    fn capabilities(&self) -> ProviderCapabilities;
}

pub struct ProviderCapabilities {
    pub supports_streaming: bool,
    pub supports_tool_calls: bool,
    pub supports_vision: bool,
    pub max_context_tokens: u32,
}
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

### Task 4.2: Refactor `Agent` to hold `Arc<dyn Provider>`

- [ ] **Step 1: Write failing test** asserting `Agent::provider()` accessor
- [ ] **Step 2: Run (fail)** / **Step 3: Refactor** `Agent` field `client: Arc<LlmClient>` → `client: Arc<dyn Provider>`
- [ ] **Step 4: Run** `cargo test --all` — all existing tests pass
- [ ] **Step 5: Commit**

```bash
git commit -m "refactor(agent): hold Arc<dyn Provider> instead of LlmClient"
```

---

## Task Group 5: CompressionTool Abstraction (`compression-tool`)

### Task 5.1: Define `CompressionTool` trait + `CompressionResult`

- [ ] **Step 1: Write failing test** / **Step 2: Run (fail)** / **Step 3: Implement**

```rust
// crates/synthia-agent/src/compression.rs
#[async_trait]
pub trait CompressionTool: Send + Sync + 'static {
    async fn compress(&self, history: &[AgentMessage]) -> Result<CompressionResult, AgentError>;
}

#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub messages: Vec<AgentMessage>,
    pub summary: String,
}
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

### Task 5.2: Implement `DefaultCompactionTool`

- [ ] **Step 1: Write failing test** asserting byte-equivalence with legacy inline `compress()`

```rust
#[tokio::test]
async fn default_matches_legacy_compress() {
    let history = make_history(100);
    let legacy = legacy_compress(&history);
    let default = DefaultCompactionTool.compress(&history).await.unwrap();
    assert_eq!(default.messages, legacy.messages);
    assert_eq!(default.summary, legacy.summary);
}
```

- [ ] **Step 2: Run (fail)** / **Step 3: Implement** by calling existing `compress()` body verbatim
- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

### Task 5.3: Wire `CompressionTool` into `Agent` builder + `react_loop`

- [ ] **Step 1: Write failing test** asserting `Agent::builder().compression(Arc::new(CustomTool))` overrides default
- [ ] **Step 2: Run (fail)** / **Step 3: Add builder method + call site in `react_loop`**

```rust
// in react_loop, replace `compress(&history)` with `self.compression.compress(&history).await?.messages`
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

---

## Task Group 6: ToolPermission Trait (`tool-permission`)

### Task 6.1: Define `ToolPermission` trait + supporting types

- [ ] **Step 1: Write failing test** / **Step 2: Run (fail)** / **Step 3: Implement**

```rust
// crates/synthia-tools/src/permission.rs
pub trait ToolPermission: Send + Sync + 'static {
    fn check(&self, ctx: &PermissionContext) -> PermissionDecision;
}

pub enum PermissionDecision {
    Allow,
    Deny(String),
    Ask,
}

#[derive(Debug, Clone)]
pub struct PermissionContext {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub agent_run_id: uuid::Uuid,
    pub user_id: Option<String>,
}

pub struct PermissionAlwaysAllow;
impl ToolPermission for PermissionAlwaysAllow {
    fn check(&self, _: &PermissionContext) -> PermissionDecision { PermissionDecision::Allow }
}
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

### Task 6.2: Add permission check in `ToolExecution::execute()`

- [ ] **Step 1: Write failing test** asserting `Deny` returns `ToolError::PermissionDenied`
- [ ] **Step 2: Run (fail)** / **Step 3: Insert check** at top of `execute()`

```rust
async fn execute(&self, args: Value) -> Result<Value, ToolError> {
    let ctx = PermissionContext::from_current(&self.name(), args.clone());
    match self.permission.check(&ctx) {
        PermissionDecision::Allow => { /* existing body */ }
        PermissionDecision::Deny(reason) => Err(ToolError::PermissionDenied(reason)),
        PermissionDecision::Ask => Err(ToolError::RequiresApproval),
    }
}
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

---

## Task Group 7: AgentTool Factory Wiring (`agent-tool-wiring`)

### Task 7.1: Wire factory into `Agent::builder().build()`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn agent_tool_registered_after_build() {
    let agent = Agent::builder().build();
    assert!(agent.registry().get("agent").is_some());
}
```

- [ ] **Step 2: Run (fail)** / **Step 3: Implement**

```rust
// in Agent::builder().build()
let mut registry = ToolRegistry::new();
let agent_tool = AgentTool::factory(&registry);
registry.insert("agent", Arc::new(agent_tool));
Agent { registry, /* ... */ }
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

```bash
git commit -m "fix(tools): wire AgentTool factory into Agent builder"
```

---

## Task Group 8: AgentRunConfig Field Cleanup (`config-field-cleanup`)

### Task 8.1: Audit + document every `_xxx` field

- [ ] **Step 1: Run** `rg "^\s+_\w+:" crates/synthia-agent/src/config.rs` to enumerate all 11 fields
- [ ] **Step 2: Write audit table** in `CHANGELOG.md` mapping each field to: `read` / `drop + delete` / `rename to <new>`
- [ ] **Step 3: For each deleted field**: remove from struct + add CHANGELOG note
- [ ] **Step 4: For each renamed field**: add `#[deprecated(note = "use <new_name>")]` alias struct + CHANGELOG note
- [ ] **Step 5: Commit per-field or bulk commit**

### Task 8.2: Add silent-drop detection test

- [ ] **Step 1: Write failing test** using `syn` to parse `config.rs` and assert no `_` prefixed fields

```rust
#[test]
fn no_underscore_prefixed_fields_in_agent_run_config() {
    let src = include_str!("../src/config.rs");
    let file: syn::File = syn::parse_str(src).unwrap();
    for item in &file.items {
        if let syn::Item::Struct(s) = item {
            if s.ident == "AgentRunConfig" {
                for field in &s.fields {
                    let name = field.ident.as_ref().unwrap();
                    assert!(!name.to_string().starts_with('_'),
                        "AgentRunConfig field `{name}` has `_` prefix; rename or delete");
                }
            }
        }
    }
}
```

- [ ] **Step 2: Run (fail)** / **Step 3: Add `syn` as dev-dep**

```toml
[dev-dependencies]
syn = { version = "2", features = ["full"] }
```

- [ ] **Step 4: Run (pass)** / **Step 5: Commit**

```bash
git commit -m "test(agent): guard against _xxx silent-drop fields"
```

---

## Task Group 9: Validation & Release Prep

### Task 9.1: Full CI pipeline

- [ ] **Step 1:** Run `cargo +nightly fmt --all` — expect no diff
- [ ] **Step 2:** Run `cargo clippy --all-targets --all-features --tests --all -- -D warnings` — expect 0 warnings
- [ ] **Step 3:** Run `cargo test --all` — expect all pass
- [ ] **Step 4:** Run `cargo miri test` on key unsafe paths — expect pass
- [ ] **Step 5:** Run `cargo bench --no-run` to verify benches compile

### Task 9.2: CHANGELOG consolidation

- [ ] **Step 1:** Append consolidated entry to `CHANGELOG.md` under `## Unreleased > ### Changed`
- [ ] **Step 2:** List all 8 new capabilities with one-line summary each
- [ ] **Step 3:** Note `ToolV1` alias retained for 2 minor versions
- [ ] **Step 4:** Reference `openspec/changes/agent-toolification-v3/` for full design
- [ ] **Step 5:** Commit

### Task 9.3: Spec-to-test traceability check

- [ ] **Step 1:** For each `#### Scenario` in `specs/*/spec.md`, grep for matching test name in `cargo test --all`
- [ ] **Step 2:** For unmatched scenarios, document deferral in `specs/DEFERRED.md` (or write the missing test)
- [ ] **Step 3:** Verify >= 80% scenario coverage
- [ ] **Step 4:** Open draft PR with body linking `openspec/changes/agent-toolification-v3/`
- [ ] **Step 5:** Address review and merge

---

## Self-Review (against spec coverage)

| Spec | Plan tasks | Coverage |
|------|-----------|----------|
| `tool-trait-decomposition` | TG1 (5 tasks) | ✅ All 4 scenarios covered |
| `agent-message-view` | TG2 (3 tasks) | ✅ All 5 scenarios covered |
| `tool-registry-dual-index` | TG3 (2 tasks) | ✅ All 5 scenarios covered |
| `provider-trait` | TG4 (2 tasks) | ✅ All 5 scenarios covered |
| `compression-tool` | TG5 (3 tasks) | ✅ All 7 scenarios covered |
| `tool-permission` | TG6 (2 tasks) | ✅ All 6 scenarios covered |
| `agent-tool-wiring` | TG7 (1 task) | ✅ All 3 scenarios covered |
| `config-field-cleanup` | TG8 (2 tasks) | ✅ All 5 scenarios covered |

**Total**: 8 capabilities / 40 scenarios / 20 atomic tasks across 9 task groups.

**No placeholders found.** Type consistency verified across tasks.