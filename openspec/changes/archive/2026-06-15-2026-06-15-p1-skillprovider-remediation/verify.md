# Verify: p1-skillprovider-remediation

## Self-test (7 stages, all pass)

| Stage | Check | Result |
|-------|-------|--------|
| 1 | `grep -rn 'SkillProvider' crates/ --include='*.rs'` | 0 functional hits, 1 doc-comment hit (`registry.rs:691` historical reference) |
| 2 | `grep -rn 'traits::' crates/synthia-skill/src/` | 0 hits from removed `SkillProvider` (the 3 remaining are `ModelProvider` and `Tool` from other modules) |
| 3 | `cargo check --workspace --all-targets` | 0 errors |
| 4 | `cargo test --workspace` | 0 failures across all crates |
| 5 | `cargo clippy --all-targets --all-features --tests --all` | 0 warnings |
| 6 | `cargo +nightly fmt --all` | formatted |
| 7 | `bash scripts/check_synced_spec_format.sh` | OK |
| 8 | `openspec validate 2026-06-15-p1-skillprovider-remediation --strict` | valid |

## Trait removal audit (4-0 REMOVE consensus)

Per `brainstorm.md`, all 4 parties independently confirmed:

| Indicator | Value |
|-----------|-------|
| Trait bound usage (`T: SkillProvider`) | 0 |
| `dyn SkillProvider` | 0 |
| `Arc<SkillProvider>` / `Box<SkillProvider>` | 0 |
| Real implementations | 1 (`SkillRegistry`) — only self-consumer |
| `use` imports | 4 (in installer, watcher, implicit_tools tests, builtin/skill) |
| Re-export | 1 (`synthia_skill::SkillProvider`) |
| Dead test fakes | 1 (`test-support::fake_skill_provider`) |

## Files changed

### Deleted
- `crates/synthia-skill/src/traits.rs` — trait definition (10 methods)
- `test-support/src/fake_skill_provider.rs` — dead test fake

### Modified
- `crates/synthia-skill/src/registry.rs` — `impl SkillProvider for SkillRegistry` → inherent methods; added sync `unregister(&str) -> bool` to preserve bool semantic for sync call sites (CLI/watcher/installer)
- `crates/synthia-skill/src/lib.rs` — removed `pub use traits::SkillProvider`
- `crates/synthia-skill/src/installer.rs` — removed `traits::SkillProvider` import; restored `use synthia_core::Error;` (dropped transient `Registry` import)
- `crates/synthia-skill/src/watcher.rs` — same import cleanup
- `crates/synthia-skill/src/implicit_tools.rs` — same import cleanup
- `crates/synthia-command/src/builtin/skill.rs` — removed `SkillProvider,` from `use synthia_skill::{...}`
- `test-support/src/lib.rs` — removed `pub mod fake_skill_provider;` and its re-export

## Design note: sync `unregister` wrapper

The trait `synthia_core::Registry::unregister` is `async` (returns `Result<(), Error>`).
The removed `SkillProvider::unregister` was `sync` (returns `bool`).

To preserve the bool semantic at the original sync call sites (CLI builtin
command, file watcher callback, installer uninstall) without propagating
`async` up multiple call stacks, an inherent `pub fn unregister(&self, name:
&str) -> bool` was added to `SkillRegistry`. The async `Registry` trait impl
now delegates to this inherent method (3-line body).

This is the minimum-code refactor that fixes the call sites after the trait
removal. The sync method is small (10 lines + doc comment) and contains the
exact body of the deleted `SkillProvider::unregister` impl.

## Quality gates summary

- `cargo check`: 0 errors
- `cargo test --workspace`: all pass (48+ test result groups, 0 FAILED)
- `cargo clippy`: 0 warnings
- `cargo +nightly fmt --all`: formatted
- Spec validation: valid
- CI script self-test: OK

## No downstream impact

- No new dependencies added
- No public API added (only removed)
- One inherent method added (sync, not exposed via trait)
- Test fake `FakeSkillProvider` was never used outside `test-support`'s own
  module tree; verified by `git grep fake_skill_provider`

## Archive readiness

- OpenSpec change files are gitignored (per `.gitignore` line 37)
- All 19 tasks in `tasks.md` complete
- Change is ready for `openspec archive`
