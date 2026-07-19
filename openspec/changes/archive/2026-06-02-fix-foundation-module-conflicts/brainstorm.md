# Brainstorming: Fix Foundation Module Conflicts

## Problem Statement

The codebase (commit `5f39dee`) has pre-existing build errors that prevent any feature work:

1. **Rust module conflict** - Cannot have both `prompt.rs` and `prompt/mod.rs`
2. **Missing dependencies** - `anyhow`, `sys-locale`, `regex` not in synthia-context
3. **68+ errors in synthia-agent** - Incomplete code references non-existent types

## Root Cause

The "feat: init agent" commit appears to be partially implemented or has file naming errors.

## Proposed Solution

**Step 1: Fix synthia-context (already partially done)**
- Remove `prompt.rs` (keep `prompt/mod.rs`)
- Add missing deps to Cargo.toml
- Declare `prompt_layer` module

**Step 2: Analyze synthia-agent issues**
- Map all 68 errors to their root causes
- Determine if files are missing or code is incomplete

**Step 3: Fix synthia-agent**
- Fix module conflicts (agent.rs vs agent/mod.rs, types.rs vs types/mod.rs)
- Fix or remove incomplete code

**Step 4: Verify build passes**
- `cargo build` succeeds
- `cargo test` passes (or at least compiles)