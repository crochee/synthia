## Why

The codebase cannot build due to module conflicts and missing dependencies. Fixing these foundation issues is a prerequisite for any feature work.

## What Changes

**synthia-context fixes:**
- From: Missing `anyhow`, `sys-locale`, `regex` dependencies
- To: All dependencies declared in Cargo.toml
- Reason: Required for compilation

**synthia-context module fix:**
- From: Both `prompt.rs` and `prompt/mod.rs` exist (conflict)
- To: Only `prompt/mod.rs` exists
- Reason: Rust doesn't allow both

**synthia-context exports:**
- From: `prompt.rs` exports `PromptLayer` (which was in `prompt_layer.rs`)
- To: `prompt_layer.rs` is a declared module, `PromptLayer` exported properly
- Reason: Fix unresolved import

**synthia-agent fixes (TBD after analysis):**
- Fix module conflicts (agent.rs vs agent/mod.rs, types.rs vs types/mod.rs)
- Fix or remove incomplete code

## Capabilities

### New Capabilities
- `foundation-module-fix`: Fix Rust module naming conflicts

### Modified Capabilities
- None yet - pending analysis of synthia-agent errors

## Impact
- **Code**: synthia-context/Cargo.toml, synthia-context/src/lib.rs
- **API**: None (internal fixes only)
- **Dependencies**: None (adding missing deps only)