# Synthia Code Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate code duplication across Synthia codebase by unifying ReAct implementations, core types, compaction/checkpoint logic, and registry patterns.

**Architecture:** Consolidation via delegation pattern — canonical implementations in one location, others delegate to it. Orphan crates evaluated for deletion or migration. Registry replaced with core::Registry<T> across all crates.

**Tech Stack:** Rust 2024, cargo workspace, tokio async runtime

---

## Phase 1: Orphan Crate Evaluation

### Task 1.1: Evaluate synthia-agent-core

**Files:**
- Read: `crates/synthia-agent-core/src/react.rs`
- Read: `crates/synthia-agent-core/src/component.rs`
- Compare with: `crates/synthia-agent/src/agent/react.rs`

- [ ] **Step 1: Read synthia-agent-core/src/react.rs**

```bash
cat crates/synthia-agent-core/src/react.rs
```
Expected: 106 lines defining ReActLoop, ReactState, AgentEvent

- [ ] **Step 2: Read synthia-agent-core/src/component.rs**

```bash
cat crates/synthia-agent-core/src/component.rs
```
Expected: Defines AgentConfig, AgentContext traits

- [ ] **Step 3: Compare with synthia-agent/react.rs**

Check if synthia-agent/src/agent/react.rs already covers the same functionality
Expected: If yes → mark for deletion

- [ ] **Step 4: Decision**

If functionality covered elsewhere → delete the crate
If unique functionality → note for migration

```bash
git rm -r crates/synthia-agent-core
```

---

### Task 1.2: Evaluate synthia-react

**Files:**
- Read: `crates/synthia-react/src/react.rs`
- Read: `crates/synthia-react/src/traits.rs`
- Compare with: `crates/synthia-agent/src/agent/react.rs`

- [ ] **Step 1: Read synthia-react/src/react.rs**

```bash
cat crates/synthia-react/src/react.rs
```
Expected: 169 lines with ReactState, SessionConfig definitions

- [ ] **Step 2: Read synthia-react/src/traits.rs**

```bash
cat crates/synthia-react/src/traits.rs
```
Expected: Defines ReActExecutor, StepResult, ToolCallInfo traits

- [ ] **Step 3: Compare functionality**

Check if these traits are already in synthia-agent
Expected: If yes → mark for deletion

---

### Task 1.3-1.6: Evaluate remaining orphan crates

**Files:**
- Read: `crates/synthia-so/src/`
- Read: `crates/synthia-guardian/src/sandbox.rs`
- Read: `crates/synthia-model-router/src/`
- Read: `crates/synthia-tracing/src/`

- [ ] **Step 1: Evaluate each crate similarly to Task 1.1**
- [ ] **Step 2: Delete crates with duplicate functionality**
- [ ] **Step 3: Note unique crates for potential migration**

---

## Phase 2: Core Type Unification

### Task 2.1: Verify types/event.rs completeness

**Files:**
- Read: `crates/synthia-agent/src/types/event.rs`
- Read: `crates/synthia-agent/src/events.rs`
- Read: `crates/synthia-agent-core/src/react.rs` (if exists)

- [ ] **Step 1: Read types/event.rs and count variants**

```bash
grep -A 100 "pub enum AgentEvent" crates/synthia-agent/src/types/event.rs | head -50
```
Expected: Complete enum with all event variants

- [ ] **Step 2: Read events.rs for comparison**

```bash
grep -A 100 "pub enum AgentEvent" crates/synthia-agent/src/events.rs | head -50
```
Expected: May have fewer variants

- [ ] **Step 3: Verify types/event.rs covers all events from events.rs**

If types/event.rs is superset → it is canonical
If not → add missing variants

- [ ] **Step 4: Search codebase for AgentEvent usages**

```bash
grep -r "AgentEvent" crates/synthia-agent/src/ --include="*.rs" | head -30
```
Expected: All should import from types/event.rs

---

### Task 2.2: Update all references to canonical AgentEvent

**Files:**
- Modify: All files importing AgentEvent from wrong location
- Delete: `crates/synthia-agent/src/events.rs` (after verification)

- [ ] **Step 1: Find all files with wrong import**

```bash
grep -r "events::AgentEvent\|use.*events.*AgentEvent" crates/synthia-agent/src/ --include="*.rs"
```
Expected: List of files to update

- [ ] **Step 2: Update each file to use types::event::AgentEvent**

In each file, change:
```rust
use crate::events::AgentEvent;
```
to:
```rust
use crate::types::event::AgentEvent;
```

- [ ] **Step 3: Verify compilation**

```bash
cargo build -p synthia-agent 2>&1 | head -50
```
Expected: No AgentEvent-related errors

- [ ] **Step 4: Delete events.rs**

```bash
git rm crates/synthia-agent/src/events.rs
```

---

## Phase 3: ReAct Implementation Consolidation

### Task 3.1: Identify unique functionality in top-level react.rs

**Files:**
- Read: `crates/synthia-agent/src/react.rs` (1179 lines)
- Read: `crates/synthia-agent/src/agent/react.rs` (725 lines)

- [ ] **Step 1: Read top-level react.rs structure**

```bash
head -100 crates/synthia-agent/src/react.rs
```
Expected: Module structure and public API

- [ ] **Step 2: Read agent/react.rs structure**

```bash
head -100 crates/synthia-agent/src/agent/react.rs
```
Expected: Similar structure but more modular

- [ ] **Step 3: Identify functions/types in react.rs not in agent/react.rs**

```bash
diff <(grep "^pub " crates/synthia-agent/src/react.rs) <(grep "^pub " crates/synthia-agent/src/agent/react.rs)
```
Expected: List of items unique to react.rs

---

### Task 3.2: Merge unique functionality into agent/react.rs

**Files:**
- Modify: `crates/synthia-agent/src/agent/react.rs`
- Delete: `crates/synthia-agent/src/react.rs` (after merge)

- [ ] **Step 1: Copy unique functions from react.rs to agent/react.rs**
- [ ] **Step 2: Update module structure in agent/ to match**
- [ ] **Step 3: Update Cargo.toml to remove top-level react.rs module entry**

In `Cargo.toml`, remove or comment out:
```toml
# modules that only exist in top-level react.rs
```

- [ ] **Step 4: Verify compilation**

```bash
cargo build -p synthia-agent 2>&1 | head -50
```
Expected: Successful build

- [ ] **Step 5: Delete top-level react.rs**

```bash
git rm crates/synthia-agent/src/react.rs
```

---

## Phase 4: AgentConfig Layer Separation

### Task 4.1: Implement From conversions

**Files:**
- Modify: `crates/synthia-cli/src/config.rs`
- Modify: `crates/synthia-server/src/config/agent.rs`
- Modify: `crates/synthia-agent/src/config/agent_config.rs`

- [ ] **Step 1: Read current AgentConfigYaml structure**

```bash
grep -A 30 "struct AgentConfigYaml" crates/synthia-cli/src/config.rs
```

- [ ] **Step 2: Read Server AgentConfig structure**

```bash
grep -A 30 "pub struct AgentConfig" crates/synthia-server/src/config/agent.rs
```

- [ ] **Step 3: Read Runtime AgentConfig structure**

```bash
grep -A 30 "pub struct AgentConfig" crates/synthia-agent/src/config/agent_config.rs
```

- [ ] **Step 4: Implement From<AgentConfigYaml> for Server AgentConfig**

In `crates/synthia-server/src/config/agent.rs`:
```rust
impl From<AgentConfigYaml> for AgentConfig {
    fn from(yaml: AgentConfigYaml) -> Self {
        // conversion logic
    }
}
```

- [ ] **Step 5: Implement From<ServerAgentConfig> for Runtime AgentConfig**
- [ ] **Step 6: Verify with cargo build**

---

## Phase 5: MemoryStore Trait Refactor

### Task 5.1: Define read/write sub-traits

**Files:**
- Modify: `crates/synthia-memory/src/types.rs`
- Modify: `crates/synthia-memory/src/memory_pipeline/file_store.rs`
- Modify: `crates/synthia-memory/src/cold/store.rs`

- [ ] **Step 1: Read current MemoryStore trait**

```bash
grep -A 50 "pub trait MemoryStore" crates/synthia-memory/src/types.rs
```

- [ ] **Step 2: Define MemoryStoreRead and MemoryStoreWrite sub-traits**

```rust
pub trait MemoryStoreRead {
    fn get(&self, key: &str) -> Option<String>;
    fn search(&self, query: &str) -> Vec<MemoryResult>;
}

pub trait MemoryStoreWrite {
    fn set(&mut self, key: &str, value: &str);
    fn delete(&mut self, key: &str);
}
```

- [ ] **Step 3: Update file_store.rs to implement MemoryStoreRead**
- [ ] **Step 4: Update cold/store.rs to implement MemoryStoreWrite**
- [ ] **Step 5: Verify with cargo build**

---

## Phase 6: LoopDetector Centralization

### Task 6.1: Verify agent/loop_detector.rs completeness

**Files:**
- Read: `crates/synthia-agent/src/agent/loop_detector.rs`
- Read: `crates/synthia-agent/src/stream_builder/loop_detection.rs`
- Read: `crates/synthia-guardian/src/loop_detector.rs`

- [ ] **Step 1: Read agent/loop_detector.rs**

```bash
cat crates/synthia-agent/src/agent/loop_detector.rs
```
Expected: Full LoopDetector struct implementation

- [ ] **Step 2: Read stream_builder/loop_detection.rs**

```bash
cat crates/synthia-agent/src/stream_builder/loop_detection.rs
```
Expected: trait LoopDetector and LoopDetectorSet

- [ ] **Step 3: Update stream_builder to delegate to agent's LoopDetector**

In `stream_builder/loop_detection.rs`, change trait implementation to:
```rust
impl LoopDetector for MyLoopDetector {
    fn detect(&self, state: &AgentState) -> DetectionResult {
        // delegate to agent::loop_detector::LoopDetector
        agent::loop_detector::LoopDetector::new().detect(state)
    }
}
```

- [ ] **Step 4: Update guardian/loop_detector.rs similarly**
- [ ] **Step 5: Verify with cargo build**

---

## Phase 7: Compaction Centralization

### Task 7.1: Verify context/compaction/ completeness

**Files:**
- Read: `crates/synthia-context/src/compaction/compactor.rs`
- Read: `crates/synthia-context/src/compaction_service.rs`
- Read: `crates/synthia-agent/src/compaction.rs`

- [ ] **Step 1: Read context compaction implementation**
- [ ] **Step 2: Update agent/compaction.rs to delegate to context**
- [ ] **Step 3: Delete agent compaction duplicates**
- [ ] **Step 4: Verify with cargo build**

---

## Phase 8: Checkpoint Centralization

### Task 8.1: Verify context/checkpoint.rs completeness

**Files:**
- Read: `crates/synthia-context/src/checkpoint.rs`
- Read: `crates/synthia-agent/src/checkpoint.rs`

- [ ] **Step 1: Read context checkpoint implementation**
- [ ] **Step 2: Update agent checkpoint to delegate to context**
- [ ] **Step 3: Delete agent/checkpoint.rs**
- [ ] **Step 4: Verify with cargo build**

---

## Phase 9: Sandbox Centralization

### Task 9.1: Verify exec/sandbox.rs completeness

**Files:**
- Read: `crates/synthia-exec/src/sandbox.rs`
- Read: `crates/synthia-guardian/src/sandbox.rs`

- [ ] **Step 1: Read exec sandbox implementation**
- [ ] **Step 2: Update guardian to delegate to exec for actual sandbox**
- [ ] **Step 3: Guardian keeps only policy checks**
- [ ] **Step 4: Delete guardian/sandbox.rs**
- [ ] **Step 5: Verify with cargo build**

---

## Phase 10: Registry Consolidation

### Task 10.1: Verify core::Registry<T> API

**Files:**
- Read: `crates/synthia-core/src/registry.rs` (or wherever Registry<T> is)
- Read: `crates/synthia-tool/src/registry/mod.rs`

- [ ] **Step 1: Find and read core::Registry<T>**

```bash
grep -r "pub trait Registry" crates/synthia-core/src/
```

- [ ] **Step 2: Read tool registry as example**

```bash
cat crates/synthia-tool/src/registry/mod.rs | head -100
```

- [ ] **Step 3: Replace tool registry with core::Registry<T>**

Change:
```rust
pub struct ToolRegistry { ... }
```
To:
```rust
pub type ToolRegistry = Registry<Tool>;
```

- [ ] **Step 4: Repeat for all other registries**

For each crate with a registry:
- synthia-skill
- synthia-provider
- synthia-command
- synthia-plugin
- synthia-task
- synthia-hook
- synthia-mcp

- [ ] **Step 5: Verify with cargo build --all**

---

## Phase 11: Verification

### Task 11.1: Full workspace build

- [ ] **Step 1: Run cargo build --all**

```bash
cargo build --all 2>&1 | tail -30
```
Expected: Successful build

### Task 11.2: Run tests

- [ ] **Step 1: Run cargo test --all**

```bash
cargo test --all 2>&1 | tail -50
```
Expected: All tests pass

### Task 11.3: Final review

- [ ] **Step 1: List all deleted files**

```bash
git diff --name-status HEAD~1 | grep "^D"
```

- [ ] **Step 2: List all modified files**

```bash
git diff --name-status HEAD~1 | grep "^M"
```

- [ ] **Step 3: Commit the consolidation**

```bash
git add -A
git commit -m "refactor: consolidate duplicate code across synthia crates

- Unified ReAct to agent/react.rs
- AgentEvent to types/event.rs
- AgentConfig layer separation via From/Into
- MemoryStore read/write trait split
- LoopDetector centralized to agent/
- Compaction to context/
- Checkpoint to context/
- Sandbox to exec/, guardian delegates
- All registries use core::Registry<T>
- Deleted orphan crates with duplicate code"
```

---

**Plan complete.** Artifacts created:
- `openspec/changes/synthia-code-consolidation/brainstorm.md`
- `openspec/changes/synthia-code-consolidation/proposal.md`
- `openspec/changes/synthia-code-consolidation/design.md`
- `openspec/changes/synthia-code-consolidation/tasks.md`
- `openspec/changes/synthia-code-consolidation/plan.md`
- `openspec/changes/synthia-code-consolidation/specs/**/spec.md` (10 spec files)