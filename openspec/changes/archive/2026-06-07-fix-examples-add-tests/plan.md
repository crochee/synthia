# Fix Examples Compilation Errors and Add Evaluation Smoke Test Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix synthia-examples compilation errors and verify synthia-evaluation has adequate test coverage.

**Architecture:** This is a straightforward bug-fix + verification task. The examples API has drifted from the current synthia-tool implementation. synthia-evaluation already has inline tests that satisfy the smoke test requirement.

**Tech Stack:** Rust (cargo), synthia-tool, synthia-evaluation

---

## Task 1: Fix tool_usage.rs Compilation Errors

**Files:**
- Modify: `examples/tool_usage.rs:1-63`
- Reference: `crates/synthia-tool/src/registry.rs` (ToolRegistry::register method)
- Reference: `crates/synthia-tool/src/types.rs` (ToolOutput type)

- [ ] **Step 1: Remove broken import**

Open `examples/tool_usage.rs` and remove line 8:
```rust
use synthia_tool::registry::RegisterableTool;  // DELETE THIS LINE
```

- [ ] **Step 2: Verify ToolEntry is not needed**

The file uses `Arc::new(FakeTool::new(...))` directly. The `register()` method on `ToolRegistry` accepts `ToolEntry`, but we can wrap directly. No code change needed here.

- [ ] **Step 3: Change register_tool() to register()**

On line 20, change:
```rust
registry.register_tool(greeting_tool);
```
to:
```rust
registry.register(greeting_tool);  // greeting_tool is Arc<FakeTool>, wrapped in ToolEntry::new() internally via register() accepting ToolEntry
```

Wait - `ToolEntry::new()` wraps `Arc<dyn Tool>`. So we need to wrap:
```rust
registry.register(ToolEntry::new(greeting_tool));
```

- [ ] **Step 4: Fix output iteration**

The `run_with_context` returns `Result<Vec<ToolOutput>>`. Lines 57-62 currently try to access `output.content` on a `Vec<ToolOutput>`.

Change lines 52-62 from:
```rust
let output = registry
    .run_with_context(vec![tool_call], context)
    .await
    .unwrap();

for part in &output.content {
    if let Some(text) = part.text() {
        println!("Tool output: {}", text);
    }
}
println!("Is error: {}", output.is_error.unwrap_or(false));
```

to:
```rust
let outputs = registry
    .run_with_context(vec![tool_call], context)
    .await
    .unwrap();  // Vec<ToolOutput>

for output in &outputs {
    // ToolOutput has text() method if it's a Text variant
    for part in &output.content {
        if let synthia_provider::types::ContentPart::Text(text_part) = part {
            println!("Tool output: {}", text_part.text);
        }
    }
    println!("Is error: {}", output.is_error.unwrap_or(false));
}
```

- [ ] **Step 5: Verify the fix compiles**

Run: `cargo build -p synthia-examples --example tool_usage`
Expected: Compiles without errors

---

## Task 2: Fix basic_chat.rs Compilation Errors

**Files:**
- Modify: `examples/basic_chat.rs:7`

- [ ] **Step 1: Remove unused AgentRunConfig import**

On line 7, change:
```rust
use synthia_agent::{Agent, AgentConfig, AgentInput, AgentRunConfig, AgentRunConfigBuilder};
```
to:
```rust
use synthia_agent::{Agent, AgentConfig, AgentInput, AgentRunConfigBuilder};
```

- [ ] **Step 2: Verify the fix compiles**

Run: `cargo build -p synthia-examples --example basic_chat`
Expected: Compiles without warnings about unused imports

---

## Task 3: Verify synthia-evaluation Test Coverage

**Files:**
- Reference: `crates/synthia-evaluation/src/lib.rs:106-156`

- [ ] **Step 1: Review existing tests**

Open `crates/synthia-evaluation/src/lib.rs` and verify the `#[cfg(test)]` module (lines 106-156) contains:
- `test_evaluator_trait` - verifies trait implementation
- `test_evaluation_registry` - verifies registry registration
- `test_evaluator_evaluate` - verifies evaluate method

These 3 tests satisfy the smoke test requirement.

- [ ] **Step 2: Run existing tests**

Run: `cargo test -p synthia-evaluation`
Expected: All tests pass

---

## Task 4: Final Verification

- [ ] **Step 1: Build all examples**

Run: `cargo build --examples`
Expected: Both tool_usage and basic_chat compile successfully

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
Expected: All tests pass (including synthia-evaluation tests)

---

## Notes

1. **synthia-evaluation already has tests** - The lib.rs file contains inline tests (lines 106-156) that cover the core functionality. This satisfies the smoke test requirement in the spec.

2. **ToolEntry::new() is the correct wrapper** - Looking at `crates/synthia-tool/src/registry.rs:39-51`, `ToolEntry::new()` takes `Arc<dyn Tool>` and creates a `ToolEntry`.

3. **Output iteration** - `run_with_context` returns `Result<Vec<ToolOutput>>`. Each `ToolOutput` has `content: Vec<ContentPart>` and `is_error: Option<bool>`.