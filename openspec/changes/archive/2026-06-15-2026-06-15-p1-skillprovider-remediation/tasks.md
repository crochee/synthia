# Tasks: p1-skillprovider-remediation

## Phase 1: Pre-flight audit (✅ DONE during change setup)

- [x] 1.1 — Grep all `SkillProvider` references in `crates/`
  → 7 hits: 1 trait def, 1 impl, 1 re-export, 4 `use` imports
- [x] 1.2 — Grep trait bound / dyn / Arc<>/Box<> usage
  → 0 hits (no abstraction consumers)
- [x] 1.3 — 4-party review with updated data
  → 4-0 REMOVE consensus (recorded in `brainstorm.md`)

## Phase 2: Remove trait definition and impl

- [ ] 2.1 — Read [crates/synthia-skill/src/registry.rs](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/registry.rs) lines 550-700 to understand the existing structure
  (separate `impl SkillRegistry` blocks vs `impl SkillProvider for SkillRegistry`)
- [ ] 2.2 — Edit [crates/synthia-skill/src/traits.rs](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/traits.rs): delete the entire file
  (it contains only the trait + `use` imports; no other code uses it)
- [ ] 2.3 — Edit [crates/synthia-skill/src/registry.rs](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/registry.rs): delete the `impl crate::traits::SkillProvider for SkillRegistry` block
  - Remove the `#[async_trait]` macro wrapper
  - Methods retain their signatures exactly
  - Verify the 10 methods still compile as inherent methods
- [ ] 2.4 — Edit [crates/synthia-skill/src/lib.rs](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/lib.rs): delete `pub use traits::SkillProvider;` (line 24)

## Phase 3: Clean up dead imports in call sites

- [ ] 3.1 — Edit [crates/synthia-skill/src/installer.rs](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/installer.rs): remove `traits::SkillProvider,` from the `use crate::{...}` block
- [ ] 3.2 — Edit [crates/synthia-skill/src/watcher.rs](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/watcher.rs): remove `traits::SkillProvider,` from the `use crate::{...}` block
- [ ] 3.3 — Edit [crates/synthia-skill/src/implicit_tools.rs](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/implicit_tools.rs): remove `traits::SkillProvider` from the test `use crate::{...}` block
- [ ] 3.4 — Edit [crates/synthia-command/src/builtin/skill.rs](file:///home/crochee/workspace/synthia/crates/synthia-command/src/builtin/skill.rs): remove `SkillProvider,` from the `use synthia_skill::{...}` block

## Phase 4: Quality gates

- [ ] 4.1 — `cargo check --workspace` → 0 errors
- [ ] 4.2 — `cargo test --workspace` → all pass (baseline: 2980/2980)
- [ ] 4.3 — `cargo clippy --all-targets --all-features --tests --all` → 0 warnings
- [ ] 4.4 — `cargo +nightly fmt --all` → format (run, don't check)
- [ ] 4.5 — `grep -rn 'SkillProvider' crates/ --include='*.rs'` → 0 matches
- [ ] 4.6 — `grep -rn 'traits::' crates/synthia-skill/src/` → 0 matches (since `traits.rs` is deleted)
- [ ] 4.7 — `bash scripts/check_synced_spec_format.sh` → OK
- [ ] 4.8 — `openspec validate 2026-06-15-p1-skillprovider-remediation --strict` → valid

## Phase 5: Verify + archive

- [ ] 5.1 — Fill [verify.md](file:///home/crochee/workspace/synthia/openspec/changes/2026-06-15-p1-skillprovider-remediation/verify.md) with execution evidence
- [ ] 5.2 — `git add crates/ && git commit -m "p1-remediation: remove dead SkillProvider trait (0 bound + 0 dyn + 0 Arc/Box + 1 impl)"`
- [ ] 5.3 — `yes | openspec archive 2026-06-15-p1-skillprovider-remediation`

## 总计: 19 个 task

- Phase 1 (pre-flight): 3 task (✅ done)
- Phase 2 (remove): 4 task
- Phase 3 (cleanup): 4 task
- Phase 4 (gates): 8 task
- Phase 5 (verify+archive): 3 task

## 依赖关系

- Phase 2 完全独立, 可单独做
- Phase 3 依赖 Phase 2 (trait 删除后, import 才是 dead)
- Phase 4 依赖 Phase 2 + 3
- Phase 5 依赖 Phase 4

## 与 P0 SessionManager 决策对齐

- 同样的 4-0 REMOVE 共识 (4 派独立审计, 同一天)
- 同样的 1 commit 模式 (单一语义 "kill the dead trait")
- 同样的清理路径 (trait + impl + re-export + dead imports)
- 同样的公开 API 破坏 (`SkillProvider` 不再可导入)
