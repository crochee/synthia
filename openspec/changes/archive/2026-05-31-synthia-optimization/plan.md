# Synthia Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean up clippy errors, refactor the 1193-line registry.rs, audit architecture after recent refactors, and analyze performance bottlenecks.

**Architecture:** Three-stream parallel execution — (1) quality cleanup with clippy fixes + registry split, (2) architecture review of permission/multi-agent/task boundaries, (3) performance analysis with proposal output.

**Tech Stack:** Rust, cargo, clippy, sqlx (sqlite), workspace with 22 crates.

---

## Task 1: Clippy Cleanup

**Files:**
- Modify: `crates/synthia-agent/src/agent_tools.rs:290`
- Modify: `crates/synthia-agent/src/agent_tools.rs:336`

- [ ] **Step 1: Examine line 290 in agent_tools.rs**

Run: `sed -n '285,295p' crates/synthia-agent/src/agent_tools.rs`
Expected: Shows `.or_insert_with(HashSet::new)` pattern

- [ ] **Step 2: Fix unwrap_or_default — change or_insert_with to or_default**

Run: `sed -i 's/\.or_insert_with(HashSet::new)/\.or_default()/g' crates/synthia-agent/src/agent_tools.rs`

- [ ] **Step 3: Examine line 336 in agent_tools.rs**

Run: `sed -n '330,350p' crates/synthia-agent/src/agent_tools.rs`
Expected: Shows `.and_then(|result| { Some(...) })` pattern

- [ ] **Step 4: Fix bind_instead_of_map — change and_then to map**

Run: `sed -i 's/\.and_then(|result| { Some(/)\.map(|result| {/g' crates/synthia-agent/src/agent_tools.rs`
Note: Verify and manually fix any unbalanced braces if needed

- [ ] **Step 5: Run clippy to verify fixes**

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -20`
Expected: No errors, clean output

- [ ] **Step 6: Commit clippy fixes**

```bash
git add crates/synthia-agent/src/agent_tools.rs
git commit -m "fix(clippy): resolve unwrap_or_default and bind_instead_of_map errors"
```

---

## Task 2: Registry Refactor

**Files:**
- Modify: `crates/synthia-tool/src/registry.rs`
- Create: `crates/synthia-tool/src/registry/*.rs` (submodules)
- Modify: `crates/synthia-tool/src/lib.rs`

- [ ] **Step 1: Read current registry.rs structure**

Run: `wc -l crates/synthia-tool/src/registry.rs && head -100 crates/synthia-tool/src/registry.rs`
Expected: File is ~1193 lines; identify natural break points (struct definitions, impl blocks, trait implementations)

- [ ] **Step 2: Identify module boundaries — look for commented sections and natural groupings**

Run: `grep -n "^//\|^pub\|^struct\|^impl" crates/synthia-tool/src/registry.rs | head -50`
Expected: List of structural elements to guide module split

- [ ] **Step 3: Create registry subdirectory**

Run: `mkdir -p crates/synthia-tool/src/registry`

- [ ] **Step 4: Create modular files — split by natural boundaries (registration, validation, metadata)**

Run: `touch crates/synthia-tool/src/registry/mod.rs`
Run: `touch crates/synthia-tool/src/registry/registration.rs`
Run: `touch crates/synthia-tool/src/registry/validation.rs`
Run: `touch crates/synthia-tool/src/registry/metadata.rs`

- [ ] **Step 5: Move code to submodules — extract registration logic first**

Extract content from registry.rs into registry/registration.rs, preserving all pub items

- [ ] **Step 6: Update registry/mod.rs to re-export from submodules**

```rust
pub mod registration;
pub mod validation;
pub mod metadata;
```

- [ ] **Step 7: Verify build succeeds**

Run: `cargo build -p synthia-tool 2>&1 | tail -20`
Expected: Compiles without errors

- [ ] **Step 8: Run integration tests for API compatibility**

Run: `cargo test -p synthia-tool 2>&1 | tail -30`
Expected: All tests pass

- [ ] **Step 9: Commit registry refactor**

```bash
git add crates/synthia-tool/src/registry/
git commit -m "refactor(synthia-tool): split registry.rs into modular submodules"
```

---

## Task 3: Architecture Audit

**Files:**
- Modify: `crates/synthia-permission/src/lib.rs` (review only)
- Modify: entire codebase (search for multi-agent references)

- [ ] **Step 1: Review permission system structure**

Run: `cat crates/synthia-permission/src/lib.rs`
Expected: Unified Permission enum with clear variants

- [ ] **Step 2: Search for multi-agent references**

Run: `grep -r "synthia-multiagent" crates/ --include="*.rs" 2>/dev/null`
Expected: No results (or confirm any found references are safe)

- [ ] **Step 3: Audit task/scheduler responsibilities**

Run: `echo "=== synthia-agent/task/scheduler.rs ===" && wc -l crates/synthia-agent/src/task/scheduler.rs && echo "=== synthia-task/src ===" && ls crates/synthia-task/src/`
Expected: Show file sizes and list task-related files

- [ ] **Step 4: Document boundary findings**

Create a summary of task scheduling vs dispatching responsibility split

- [ ] **Step 5: Commit architecture audit notes**

```bash
git add docs/ # if documenting
git commit -m "docs: record architecture audit findings"
```

---

## Task 4: Performance Analysis

**Files:**
- Analyze: `crates/synthia-memory/src/cold.rs`
- Analyze: `crates/synthia-skill/src/embedding.rs`

- [ ] **Step 1: Measure baseline build time**

Run: `cargo clean > /dev/null 2>&1 && time cargo build --workspace 2>&1 | tail -5`
Expected: Record elapsed time for baseline

- [ ] **Step 2: Analyze memory cold storage patterns**

Run: `cat crates/synthia-memory/src/cold.rs | head -80`
Expected: Identify query patterns, potential N+1 issues

- [ ] **Step 3: Evaluate embedding computation**

Run: `cat crates/synthia-skill/src/embedding.rs | head -80`
Expected: Identify batching, caching, or parallelization opportunities

- [ ] **Step 4: Write performance optimization proposal**

Document findings in `performance-proposal.md` with:
- Identified bottlenecks
- Quantified impact
- Recommended optimizations (prioritized)
- Estimated improvement

- [ ] **Step 5: Commit performance analysis**

```bash
git add performance-proposal.md  # if created
git commit -m "analysis: performance bottlenecks and optimization proposal"
```

---

## Task 5: Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: All tests pass

- [ ] **Step 2: Run clippy on workspace**

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -10`
Expected: Clean output, no warnings

- [ ] **Step 3: Final regression check**

Run: `cargo build --examples 2>&1 | tail -5`
Expected: All examples compile

- [ ] **Step 4: Commit verification**

```bash
git commit -m "chore: verify all optimizations pass tests and clippy"
```