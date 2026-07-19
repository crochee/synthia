## 1. Fix synthia-context Dependencies

- [x] 1.1 Add `anyhow.workspace = true` to synthia-context/Cargo.toml
- [x] 1.2 Add `sys-locale = "0.3"` dependency to synthia-context/Cargo.toml
- [x] 1.3 Add `regex.workspace = true` dependency if needed
- [x] 1.4 Remove standalone `prompt.rs` (keep `prompt/` directory)
- [x] 1.5 Declare `prompt_layer` module in lib.rs
- [x] 1.6 Export `PromptLayer` correctly in lib.rs
- [x] 1.7 Verify `cargo build -p synthia-context` passes

## 2. Analyze synthia-agent Errors

- [x] 2.1 Run `cargo build -p synthia-agent 2>&1` to get error list
- [x] 2.2 Categorize errors by root cause (module conflict vs missing code)
- [x] 2.3 Document each module conflict found

## 3. Fix synthia-agent Module Conflicts

- [ ] 3.1 Fix agent module conflict (agent.rs vs agent/mod.rs) - **BLOCKED**: 68 pre-existing errors
- [ ] 3.2 Fix types module conflict (types.rs vs types/mod.rs) - **BLOCKED**: 68 pre-existing errors
- [ ] 3.3 Update lib.rs exports as needed
- [ ] 3.4 Verify `cargo build -p synthia-agent` shows reduced errors

## 4. Fix synthia-agent Remaining Errors

- [ ] 4.1 Address missing type imports - **BLOCKED**: Code is incomplete scaffolding
- [ ] 4.2 Fix API mismatches (rmcp, tool outputs) - **BLOCKED**: Requires completing missing code
- [ ] 4.3 Fix or remove incomplete code - **BLOCKED**: Would take days of work
- [ ] 4.4 Verify `cargo build` passes for entire workspace

## 5. Verification

- [x] 5.1 Run `cargo build` - synthia-context builds (synthia-agent blocked)
- [ ] 5.2 Run `cargo test --lib` - **BLOCKED**: synthia-agent doesn't compile
- [x] 5.3 Commit changes - synthia-context fix committed

## Note

**synthia-context fix is complete and committed** (commit 0b8b39c).

**synthia-agent is blocked**: The 68 errors are from incomplete scaffolding code that references non-existent types. This requires significant additional work before any optimization can proceed.