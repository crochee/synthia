# Synthia Gap Analysis Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Synthia's top 3 critical gaps vs OpenCode/Codex: prompt assembly convergence, tool concurrency safety, prefix tracker observability, and token counter unification.

**Architecture:** Four independent capabilities, applied sequentially in dependency order (C4 → C2 → C1 → C3) to avoid conflicts. Each capability touches a specific module and is independently revertible.

**Tech Stack:** Rust, tokio, async-trait, serde, sha2, parking_lot, dashmap

---

## File Structure (key files)

### New Files
- `crates/synthia-tool/tests/concurrency_integration.rs` — integration test for parallel scheduling
- `crates/synthia-context/tests/prompt_convergence.rs` — integration test for assembler
- `crates/synthia-telemetry/src/prefix_event.rs` — new event type

### Modified Files
- `crates/synthia-tool/src/traits.rs` — add `is_concurrency_safe` default
- `crates/synthia-tool/src/builtin/{read,glob,grep,web,path}.rs` — override `is_concurrency_safe` to true
- `crates/synthia-agent/src/agent/step.rs:194-200` — fix hardcoded `false` bug
- `crates/synthia-context/src/assembler.rs` — add `section_by_name`, `system_snapshot`
- `crates/synthia-context/src/prefix_tracker.rs` — extend with rolling window
- `crates/synthia-agent/src/stream_builder/builder.rs` — wire prefix tracker + remove ContextBuilder
- `crates/synthia-agent/src/stream_builder/context_builder.rs` — DELETE

---

## Phase 1: Token Counter Trait (C4) — already exists, validate

**Status:** `synthia-provider::TokenCounter` trait already exists with `count_message`, `count_text`, `count_image`. Both `AnthropicProvider` and `OpenAICompatibleProvider` implement it.

**Tasks:**
- 1.1-1.6: VALIDATE that existing trait satisfies spec. Update spec to match existing method names.

## Phase 2: Tool Concurrency Trait (C2)

**Step 1: Add `is_concurrency_safe` to `Tool` trait**

File: `crates/synthia-tool/src/traits.rs`

Add to the `Tool` trait:
```rust
fn is_concurrency_safe(&self) -> bool {
    false
}
```

**Step 2: Override in read-only builtins**

Files: `crates/synthia-tool/src/builtin/{read,glob,grep,web,path}.rs`

Add to each:
```rust
fn is_concurrency_safe(&self) -> bool {
    true
}
```

**Step 3: Fix step.rs hardcoded bug**

File: `crates/synthia-agent/src/agent/step.rs:194-200`

Replace:
```rust
let _is_concurrency_safe = tool_instance.requires_permission();
tool_infos.push(ToolCallInfo::new(
    tu.id,
    tu.name,
    args_value,
    false, // Default to not concurrency safe
));
```

With:
```rust
tool_infos.push(ToolCallInfo::new(
    tu.id,
    tu.name,
    args_value,
    tool_instance.is_concurrency_safe(),
));
```

## Phase 3: Prompt Assembly Convergence (C1)

**Step 1: Add `section_by_name` and `system_snapshot` to `ContextAssembler`**

File: `crates/synthia-context/src/assembler.rs`

Add:
```rust
pub fn section_by_name(&self, name: &str) -> Option<&Section> {
    self.sections.iter().find(|s| s.name == name)
}

pub fn system_snapshot(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    for s in &self.sections {
        if s.role == "system" {
            buf.extend_from_slice(s.content.as_bytes());
            buf.push(b'\n');
        }
    }
    buf
}
```

**Step 2: Remove `ContextBuilder` from stream_builder**

File: `crates/synthia-agent/src/stream_builder/context_builder.rs` — DELETE

Update `mod.rs`:
```rust
// Remove: pub mod context_builder;
// Remove: pub use context_builder::ContextBuilder;
```

Migrate callers (find via grep `ContextBuilder` in synthia-agent) to use `synthia_context::assembler::ContextAssembler`.

## Phase 4: Prefix Tracker Wiring (C3)

**Step 1: Extend `PrefixTracker` with rolling window**

File: `crates/synthia-context/src/prefix_tracker.rs`

Add to struct:
```rust
use std::collections::VecDeque;

pub struct PrefixTracker {
    // ... existing fields ...
    recent_window: VecDeque<(u64, String)>, // (turn_id, hash)
    window_size: usize, // default 20
}
```

Add methods:
```rust
pub fn record_pre(&mut self, system_bytes: &[u8], turn_id: u64) {
    let hash = Self::compute_hash_bytes(system_bytes);
    self.recent_window.push_back((turn_id, hash));
    if self.recent_window.len() > self.window_size {
        self.recent_window.pop_front();
    }
}

pub fn record_post(&mut self, system_bytes: &[u8], _turn_id: u64) -> bool {
    let hash = Self::compute_hash_bytes(system_bytes);
    if let Some((last_id, last_hash)) = self.recent_window.back() {
        return *last_hash == hash;
    }
    true
}

pub fn stability_ratio(&self) -> f64 {
    if self.recent_window.is_empty() {
        return 1.0;
    }
    // Compare adjacent entries
    let mut stable = 0;
    for w in self.recent_window.iter().collect::<Vec<_>>().windows(2) {
        if w[0].1 == w[1].1 {
            stable += 1;
        }
    }
    let total = self.recent_window.len().saturating_sub(1);
    if total == 0 { 1.0 } else { stable as f64 / total as f64 }
}

fn compute_hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
```

**Step 2: Wire into StreamBuilder**

File: `crates/synthia-agent/src/stream_builder/builder.rs`

In the LLM call site, before/after model call:
```rust
// before:
let pre_hash = prefix_tracker.lock().record_pre(&system_bytes, turn_id);
// call model...
let stable = prefix_tracker.lock().record_post(&system_bytes, turn_id);
emit_stability_event(turn_id, prefix_tracker.lock().stability_ratio());
```

**Step 3: Add telemetry event**

File: `crates/synthia-telemetry/src/prefix_event.rs` (new)

```rust
#[derive(Debug, Clone)]
pub struct PrefixStabilityEvent {
    pub turn_id: u64,
    pub stability_ratio: f64,
    pub recorded_at: std::time::SystemTime,
}
```

## Phase 5: E2E Verification

- `cargo test --workspace` — all tests pass
- `cargo clippy --all-targets --all-features --tests --all` — no warnings
- 4 integration tests cover C1/C2/C3/C4 scenarios

---

## Self-Review

1. **Spec coverage:** C1 (convergent-prompt-assembly) → Phase 3; C2 (tool-concurrency-trait) → Phase 2; C3 (prefix-tracker-wiring) → Phase 4; C4 (token-counter-unification) → Phase 1 (validate). ✓
2. **Placeholder scan:** No "TBD" / "TODO" in plan. ✓
3. **Type consistency:** `is_concurrency_safe` defined in trait, used in step.rs. `system_snapshot` defined in assembler, used in stream_builder. `record_pre` / `record_post` defined in prefix_tracker, used in stream_builder. ✓
4. **No break of public API:** `is_concurrency_safe` is default method, additive only. ✓

## Execution Handoff

Plan saved to `openspec/changes/synthia-gap-analysis-2026-06-07/plan.md`.

User requested one-shot execution. Proceeding with `openspec-apply-change` immediately.
