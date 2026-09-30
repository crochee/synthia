# Token Counting + Compaction Config Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** (1) Replace chars/4 estimation with tiktoken precise counting; (2) Expose compaction threshold via `AgentConfig.compaction_threshold`.

**Architecture:** Two separate features sharing `compact.rs`. tiktoken is added as a dependency; the counting logic in `compact.rs:15` is replaced. The threshold config is a new field on `AgentConfig` read by `StepCompact::check`.

**Tech Stack:** Rust, tiktoken-rs 0.5, synthia-session TokenBudget.

---

## Task 1: Add tiktoken Dependency

**Files:**
- Modify: `crates/synthia-agent/Cargo.toml`

- [ ] **Step 1: Add tiktoken-rs to Cargo.toml**

Find the `[dependencies]` section in `Cargo.toml` and add:
```toml
tiktoken-rs = "0.5"
```

This is a regular dependency (not optional).

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p synthia-agent 2>&1 | tail -10`
Expected: compiles successfully (no missing dependency errors)

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/Cargo.toml
git commit -m "deps(agent): add tiktoken-rs for precise token counting"
```

---

## Task 2: Implement tiktoken Token Counting

**Files:**
- Create: `crates/synthia-agent/src/stream_builder/token_counter.rs` (new module)
- Modify: `crates/synthia-agent/src/stream_builder/steps/compact.rs:15`
- Modify: `crates/synthia-agent/src/config/agent_config.rs`

- [ ] **Step 1: Create token_counter module**

Create `crates/synthia-agent/src/stream_builder/token_counter.rs`:

```rust
use tiktoken_rs::{BpeEncoder, Cl100kBase};
use crate::config::AgentConfig;

/// Counts tokens in a message list using tiktoken.
///
/// Uses the model from config to select encoding.
/// Falls back to cl100k_base for unknown models.
pub fn count_tokens(messages: &[synthia_provider::types::Message], config: &AgentConfig) -> usize {
    let encoding = match config.model.as_str() {
        "gpt-4o" | "gpt-4o-mini" | "gpt-4-turbo" => BpeEncoder::new("gpt-4o").unwrap(),
        "gpt-3.5-turbo" => BpeEncoder::new("gpt-3.5-turbo").unwrap(),
        _ => {
            tracing::warn!("Unknown model {} for tiktoken, using cl100k_base", config.model);
            Cl100kBase::new()
        }
    };

    let mut total = 0usize;
    for msg in messages {
        total += encoding.encode(&msg.content).len();
    }
    total
}
```

- [ ] **Step 2: Add module to stream_builder/mod.rs**

In `crates/synthia-agent/src/stream_builder/mod.rs`, add:
```rust
pub mod token_counter;
```

- [ ] **Step 3: Replace estimate_messages_token_count in compact.rs**

In `compact.rs:15`, replace:
```rust
let token_count = estimate_messages_token_count(&ctx.messages);
```
With:
```rust
let token_count = crate::stream_builder::token_counter::count_tokens(&ctx.messages, config);
```

Remove the import `use synthia_provider::estimate_messages_token_count;` from `compact.rs`.

- [ ] **Step 4: Verify compilation**

Run: `cargo build -p synthia-agent 2>&1 | tail -10`
Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/token_counter.rs
git add crates/synthia-agent/src/stream_builder/mod.rs
git add crates/synthia-agent/src/stream_builder/steps/compact.rs
git commit -m "feat(agent): add tiktoken token counting in compact.rs"
```

---

## Task 3: Add compaction_threshold to AgentConfig

**Files:**
- Modify: `crates/synthia-agent/src/config/agent_config.rs`
- Modify: `crates/synthia-agent/src/stream_builder/steps/compact.rs:10-23`

- [ ] **Step 1: Add compaction_threshold field to AgentConfig**

In `agent_config.rs`, find the struct definition and add:
```rust
pub struct AgentConfig {
    // ... existing fields ...
    pub compaction_threshold: Option<usize>,  // None = default 100_000
}
```

- [ ] **Step 2: Add validation**

In `AgentConfig::validate()` (if it exists), add:
```rust
if let Some(threshold) = self.compaction_threshold {
    if threshold == 0 {
        return Err(Error::Validation("compaction_threshold must be > 0".to_string()));
    }
}
```

- [ ] **Step 3: Read from config in StepCompact::check**

In `compact.rs:10-23`, replace the threshold logic:
```rust
pub fn check(&self, ctx: &LoopContext, config: &AgentConfig) -> CompactAction {
    let Some(budget) = &config.context_token_budget else {
        return CompactAction::None;
    };

    let token_count = /* tiktoken count */;
    let threshold = config.compaction_threshold.unwrap_or(100_000);

    // Check against configured threshold
    if token_count >= threshold {
        return CompactAction::MustCompact;
    }

    let status = budget.check(token_count);
    match status {
        TokenBudgetStatus::MustCompact => CompactAction::MustCompact,
        TokenBudgetStatus::Warning => CompactAction::Warning,
        _ => CompactAction::None,
    }
}
```

Note: The budget check from `config.context_token_budget` is separate from `compaction_threshold`. They can coexist — budget check is for `TokenBudgetWarning`, threshold check is for triggering compaction.

- [ ] **Step 4: Verify compilation**

Run: `cargo build -p synthia-agent 2>&1 | tail -10`
Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/config/agent_config.rs
git add crates/synthia-agent/src/stream_builder/steps/compact.rs
git commit -m "feat(agent): add configurable compaction_threshold to AgentConfig"
```

---

## Task 4: Integration Tests + Verification

- [ ] **Step 1: Write tiktoken test vector**

Add to `compact.rs` test module or a new test file:
```rust
#[test]
fn test_tiktoken_counts_correctly() {
    // "hello" = 1 token in cl100k_base
    let encoder = Cl100kBase::new();
    let tokens = encoder.encode("hello");
    assert_eq!(tokens.len(), 1);
}
```

- [ ] **Step 2: Write threshold integration test**

```rust
#[test]
fn test_compaction_threshold_respected() {
    let config = AgentConfig {
        compaction_threshold: Some(50_000),
        // ... other fields ...
    };
    let ctx = LoopContext::new("test".to_string(), SpanContext::new("test"));
    let action = StepCompact.check(&ctx, &config);
    // With messages totaling 50K+ tokens, should return MustCompact
}
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p synthia-agent 2>&1 | tail -20`
Expected: all tests pass

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -p synthia-agent 2>&1 | grep -E "error|warning" | head -20`
Expected: clean

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "test(agent): add token counting and compaction threshold tests"
```

---

## Self-Review Checklist

- [ ] Spec coverage: tiktoken counting → Task 1+2 ✅, threshold config → Task 3 ✅, test vectors → Task 4 ✅
- [ ] No placeholders: all file paths, function names exact ✅
- [ ] Type consistency: `token_counter::count_tokens(messages, config)` matches usage ✅
- [ ] Test isolation: tiktoken test uses known encoding values ✅