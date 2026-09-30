# Migrate `synthia-core::tool` into `synthia-tool` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use omo-subagent-driven-development (recommended) or omo-dispatching-parallel-agents to implement this plan task-by-task. Each task specifies a `category` (quick/deep/ultrabrain/visual-engineering) and `load_skills` for oh-my-opencode's `task()` tool. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Spec:** `docs/superpowers/specs/2026-08-02-migrate-core-tool-to-synthia-tool-design.md` (commit before executing)
>
> **Test convention:** Per project rule `.trae/rules/rust.md`, run tests per crate with `cargo test -p <crate>`. Do **NOT** use `cargo test --workspace`.
>
> **Lint/format convention:** Per project rule, finish with `cargo +nightly fmt --all` then `cargo clippy --all-targets --all-features --tests --all` (must be 0 warnings).

**Goal:** Hard-delete `crates/synthia-core/src/tool/`, move all 14 child files plus 167 embedded tests into `crates/synthia-tool/src/`, resolve three pre-existing double-definitions (`Tool` trait, `ToolRegistry`, `OutputBound` + 3 enums) by collapsing onto the `synthia-tool` side, and update 11 downstream call-sites in `synthia-agent`, `synthia-server`, `synthia-cli`.

**Architecture:** Atomic file move via `git mv` followed by import-path updates. 46 KB `ToolRegistry` becomes the canonical `synthia_tool::registry::ToolRegistry`; 9 KB dispatch logic from the existing `synthia_tool::registry::ToolRegistry` is folded in. A new private `UnifiedToolAdapter` bridges the 7-method `synthia_tool::Tool` trait to the 3-method semantic shape that the 46 KB `ToolRegistry` body code uses internally. 9 builtin tools (`apply_patch`, `read`, `write`, `shell`, `glob`, `grep`, `multi_edit`, `web`, `path`) keep implementing the 7-method trait unchanged.

**Tech Stack:** Rust 1.95+, Cargo workspace, `parking_lot::RwLock`, `async_trait`, `serde`, `tokio`, `dashmap`, `schemars 0.8`. No new third-party deps. No workspace `[workspace.dependencies]` changes. No `[workspace.members]` changes.

---

## File Manifest

### New files (created in `crates/synthia-tool/src/`)

| Path | Source | Purpose |
|---|---|---|
| `tool_name.rs` | `synthia-core/src/tool/tool_name.rs` | `ToolName` struct |
| `provider.rs` | `synthia-core/src/tool/provider.rs` | `ToolProvider` trait + `ToolEvent` enum |
| `capability.rs` | `synthia-core/src/tool/capability.rs` | `ToolCapabilities` + `CapabilityBroker` (standalone, 51 lines) |
| `descriptor.rs` | `synthia-core/src/tool/descriptor.rs` (minus dropped 3-method `Tool` trait + 3 duplicated enums) | `ToolInput` / `ToolMetadata` / `ToolError` / `ToolContext` / `ToolDescriptor` / `ToolExample` / `ToolProvenance` / `ContextSource` / `ToolExposure` / `CancelBehavior` |
| `registry/registration/registry.rs` | **REPLACES** existing 9 KB file | Merged: 46 KB core content + 9 KB current dispatch methods |
| `registry/registration/adapter.rs` | NEW | `UnifiedToolAdapter` — bridges 7-method `Tool` to 3-method `ToolRegistry` shape |
| `fragment/mod.rs` | `synthia-core/src/tool/fragment.rs` | `ContextFragment` trait + `FragmentContext`/`FragmentError`/`FragmentRegistry` |
| `fragment/builtin_fragments.rs` | `synthia-core/src/tool/builtin_fragments.rs` | `FragmentPriorities` + 8 builtin fragments |
| `skill/mod.rs` | `synthia-core/src/tool/skill_registry.rs` | `Skill` trait + `SkillRegistry` + 3 supporting enums |
| `skill/builtin_skills.rs` | `synthia-core/src/tool/builtin_skills.rs` | `CodingSkill` / `SearchSkill` / `DebugSkill` + `BUILTIN_SKILLS` |
| `plugin.rs` | `synthia-core/src/tool/plugin_registry.rs` | `Plugin` trait + `PluginRegistry` + 5 supporting types |
| `extension.rs` | `synthia-core/src/tool/extension_registry.rs` | `ExtensionRegistry` + `ProviderStore` + `CommandStore` + 3 supporting types |
| `rollout.rs` | `synthia-core/src/tool/rollout.rs` | `RolloutTracker` + 4 supporting types |
| `subagent.rs` | `synthia-core/src/tool/subagent.rs` | `SubagentFactory` trait + `SubagentOutput` + `SubagentSpawnError` |

### Deleted files (from `crates/synthia-core/src/tool/`)

| Path | Reason |
|---|---|
| `mod.rs` | 14 `pub use` lines no longer needed; `synthia-core/src/lib.rs` deletes `pub mod tool;` |
| `tool_name.rs`, `capability.rs`, `provider.rs`, `descriptor.rs`, `registry.rs`, `fragment.rs`, `builtin_fragments.rs`, `skill_registry.rs`, `builtin_skills.rs`, `plugin_registry.rs`, `extension_registry.rs`, `rollout.rs`, `subagent.rs` | Moved to `synthia-tool` (see manifest above) |
| `output_bound.rs` | File-level near-duplicate of `synthia-tool/src/truncate/output_bound.rs`. Per Decision C1, core file is deleted outright; 7 tests already covered by `synthia-tool/src/truncate/output_bound.rs` |
| `3-method `Tool` trait + `ToolCategory` + `ExecutionMode` + `ToolOutput` inside `descriptor.rs`** | Per Decision C1 + A1, these are dropped (tool-side versions are the single source of truth) |

### Modified files

| Path | Change |
|---|---|
| `crates/synthia-core/src/lib.rs` | Delete line `pub mod tool;` |
| `crates/synthia-tool/src/lib.rs` | Add 11 new `pub mod` lines + re-exports |
| `crates/synthia-tool/src/registry/registration/mod.rs` | Add re-exports for new types |
| `crates/synthia-tool/src/registry/registration/registry.rs` | Replaced by merged 46 KB + 9 KB content |
| `crates/synthia-tool/src/sub_traits/category.rs` | Drop "mirrors synthia_core::tool::descriptor::ToolCategory" wording in doc-comments (lines 4 and 8) |
| `crates/synthia-agent/src/agent.rs` | Import path update (1 line) |
| `crates/synthia-agent/src/loop_context.rs` | Import path update (1 line) |
| `crates/synthia-agent/src/loop_services.rs` | 1 import + 3 inline path rewrites |
| `crates/synthia-agent/src/component_assembly.rs` | 1 import block + 1 inline path + drop alias |
| `crates/synthia-agent/src/config/agent_config/run_config.rs` | 1 import block |
| `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` | 2 import blocks |
| `crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs` | 1 inline path |
| `crates/synthia-server/src/session/controller.rs` | 1 import block |
| `crates/synthia-server/src/state/app_state.rs` | 2 import blocks + 2 inner blocks + 6 inline `Arc<dyn ...>` + drop alias |
| `crates/synthia-server/src/routes/skills.rs` | 1 doc-comment line |
| `crates/synthia-server/tests/e2e_registry_pipeline_test.rs` | 1 inline path |
| `synthia-cli/src/repl_core/repl/agent_message.rs` | 1 import line |

### Untouched files

- All 9 `crates/synthia-tool/src/builtin/*.rs` files. They implement the 7-method `Tool` trait and do not need changes.
- All other workspace files (no `synthia_core::tool::*` imports per the audit).

---

## Task Sequencing

Tasks are grouped into 5 phases. Phases must run sequentially; tasks within a phase may run in parallel where noted.

```
Phase 1 (foundation, sequential):     Tasks 1-3
Phase 2 (file move, mostly parallel):  Tasks 4-14
Phase 3 (collision resolution):        Tasks 15-17
Phase 4 (downstream call-sites):       Tasks 18-19
Phase 5 (delete + verify):             Tasks 20-22
```

**Single atomic commit at the end of each phase** is the rollback boundary. After Phase 5 the workspace should be fully migrated; revert the last commit to roll back to the pre-migration state.

---

## Phase 1 — Pre-flight & Open Sub-decisions

### Task 1: Resolve the 5 plan-step sub-decisions

**Files:** none (read-only)

**Why:** The spec deferred 5 sub-decisions to the plan step. This task resolves them by reading the relevant code; tasks 2 onward implement the choices.

- [ ] **Step 1: Resolve sub-decision #1 — `ToolError` location**

  Read every call-site of `synthia_core::tool::descriptor::ToolError` via Grep tool. Then read `crates/synthia-tool/src/types.rs` to see the `synthia_core::Error` variants.

  Decision rule:
  - If all `ToolError` variants map 1:1 to `synthia_core::Error` variants → fold into `synthia_core::Error`. Replace the 3-method `Tool::execute` return type with `synthia_core::Error`. The `UnifiedToolAdapter::execute` (Section 5.5) returns `synthia_core::Error`.
  - Otherwise → keep `ToolError` as a distinct enum at `synthia_tool::ToolError`, re-exported from `synthia_tool::descriptor`.

  Write the result inline in the commit message of Task 17 ("collision resolution"): either "fold ToolError into synthia_core::Error" or "keep ToolError as synthia_tool::ToolError".

- [ ] **Step 2: Resolve sub-decision #2 — `descriptor.rs` location**

  Read `crates/synthia-core/src/tool/descriptor.rs` and `crates/synthia-tool/src/traits.rs`. Sum the LOC of both files *after* the 4 dropped types (`Tool` trait, `ToolCategory`, `ExecutionMode`, `ToolOutput`) are removed from `descriptor.rs`.

  Decision rule:
  - If merged total ≤ 250 LOC → put absorbed types in `synthia-tool/src/traits.rs` directly. No new file.
  - Otherwise → create `crates/synthia-tool/src/descriptor.rs` (target file from the manifest above). Re-export the 7-method `Tool` trait from `synthia-tool/src/descriptor.rs` too, so the 9 builtin tools continue to import via `use synthia_tool::Tool;`.

  Write the choice in the Task 17 commit message: "absorb descriptor into traits.rs" or "split into descriptor.rs".

- [ ] **Step 3: Resolve sub-decision #3 — `ToolRegistry` file split**

  Read the merged file content (46 KB core + 9 KB current). Compute the merged LOC.

  Decision rule:
  - If merged ≤ 350 LOC → keep in one `registry.rs`.
  - Otherwise → split into `registry.rs` (lifecycle/identity/scope: `ToolRegistry` struct, `register_provider`, `unregister`, `Materialization`, `ToolIdentity`, `RegistrationToken`, `RegistrationScope`, `ToolGeneration`, `RegistrationError`, `StaleOrUnknown`) and `dispatch.rs` (`run_with_context`, `execute_tools`, `snapshot() -> Vec<ToolMetadataSnapshot>`, `contains`, `len`, `is_empty`). Both files in `synthia-tool/src/registry/registration/`.

  Write the choice in the Task 17 commit message: "single registry.rs" or "split into registry.rs + dispatch.rs".

- [ ] **Step 4: Resolve sub-decision #4 — `ToolEvent` location**

  Grep for `ToolEvent` and `FileChangeEvent` across `crates/`. Count usages of each.

  Decision rule:
  - If `ToolEvent` has ≥ 3 distinct call-sites in the codebase and serves a different role from `FileChangeEvent` (which is filesystem-only) → keep `ToolEvent` as its own enum at `synthia_tool::provider::ToolEvent`. Do not merge.
  - If `ToolEvent` is unused except in the trait definition → delete the enum; the trait method `on_tool_event` becomes a no-op.

  Write the choice in the Task 17 commit message.

- [ ] **Step 5: Resolve sub-decision #5 — `UnifiedToolAdapter` descriptor caching**

  Read the 9 builtin tools (`crates/synthia-tool/src/builtin/*.rs`) to see if each can cheaply build a `ToolDescriptor` (with name, description, parameters JSON schema, category, provenance, execution_mode, cancel_behavior, examples, exposure, is_hidden, is_user_invocable).

  Decision rule:
  - If all 9 builtin tools already expose `description()` + `parameters()` + a category constant cheaply → adapter stores only `Arc<dyn Tool>` and computes `ToolDescriptor` lazily via `Arc::try_unwrap`/cache pattern. Saves a memory copy per registration.
  - Otherwise → adapter stores `Arc<dyn Tool>` + cached `ToolDescriptor` (computed eagerly in `UnifiedToolAdapter::new`).

  Write the choice in the Task 17 commit message: "lazy descriptor" or "eager descriptor".

- [ ] **Step 6: Commit the pre-flight decision record**

  ```bash
  git add -A
  git commit -m "plan: pre-flight decisions for tool migration (1-5)"
  ```

  If the user wants to verify the decisions before code changes, paste the 5 sub-decision outcomes and wait for confirmation. Otherwise proceed to Task 2.

**Verification:** `git log -1` shows the pre-flight commit. `git status` is clean.

---

### Task 2: Stage the spec + plan in git

**Files:** already committed (spec at `docs/superpowers/specs/2026-08-02-migrate-core-tool-to-synthia-tool-design.md`; this plan at `docs/superpowers/plans/2026-08-02-migrate-core-tool-to-synthia-tool.md`)

- [ ] **Step 1: Confirm both files exist on disk and are tracked by git**

  Run: `ls -la docs/superpowers/specs/2026-08-02-migrate-core-tool-to-synthia-tool-design.md docs/superpowers/plans/2026-08-02-migrate-core-tool-to-synthia-tool.md`
  Expected: both paths print with non-zero size.

  Run: `git status --short docs/`
  Expected: empty (both already committed during brainstorming).

- [ ] **Step 2: If either file is untracked, commit it**

  ```bash
  git add docs/superpowers/specs/2026-08-02-migrate-core-tool-to-synthia-tool-design.md
  git add docs/superpowers/plans/2026-08-02-migrate-core-tool-to-synthia-tool.md
  git commit -m "docs: add tool migration spec + plan"
  ```

**Verification:** `git log --oneline -3` shows the spec/plan commit.

---

### Task 3: Establish the pre-migration baseline

**Why:** Before any code change, capture a green-build baseline. If the workspace does not currently pass checks, the migration is starting from a broken state and rollback is harder to reason about.

- [ ] **Step 1: Run per-crate checks for the affected crates**

  Run each, expect exit code 0:

  ```bash
  cargo check -p synthia-core
  cargo check -p synthia-tool
  cargo check -p synthia-agent
  cargo check -p synthia-server
  cargo check -p synthia-cli
  cargo check -p synthia-skill
  cargo check -p test-support
  ```

  If any fails, **STOP**. Report the failure to the user. Do not proceed with the migration on a broken workspace.

- [ ] **Step 2: Run per-crate tests for the affected crates**

  Run each, expect exit code 0:

  ```bash
  cargo test -p synthia-core
  cargo test -p synthia-tool
  cargo test -p synthia-agent
  cargo test -p synthia-server
  cargo test -p synthia-cli
  ```

  Per project rule: do NOT use `cargo test --workspace`. If any fails, **STOP**.

- [ ] **Step 3: Record the cargo tree for later comparison**

  ```bash
  cargo tree --workspace --no-default-features > /tmp/synthia-pre-migration-tree.txt
  ```

  This file is the cycle-comparison baseline for Task 22.

- [ ] **Step 4: Verify no uncommitted changes**

  Run: `git status --porcelain`
  Expected: empty output. If not, commit or stash before proceeding.

**Verification:** baseline recorded in `/tmp/synthia-pre-migration-tree.txt`. All `cargo check` and `cargo test` exit 0.

---

## Phase 2 — File Move (mostly parallel)

**Important:** Tasks 4-14 are independent `git mv` operations plus import-path updates inside the moved file. They are split for rollback granularity — a partial phase 2 failure can be reverted by reverting individual task commits.

For each task, the pattern is:
1. `git mv` the source file to its destination (or `mkdir -p` + `git mv` for new directories).
2. Update intra-file imports inside the moved file: `crate::tool::X` → `crate::X` (and the parent module, depending on where the file lands).
3. Update cross-crate imports inside the moved file: `use crate::tool::tool_name::ToolName;` inside `provider.rs` becomes `use crate::tool_name::ToolName;` (if `provider.rs` lands in `synthia-tool/src/provider.rs`).
4. Run `cargo check -p synthia-tool` to verify the moved file compiles in isolation. Do NOT proceed to the next task if this fails.

### Task 4: Move `tool_name.rs`

**Files:**
- Move: `crates/synthia-core/src/tool/tool_name.rs` → `crates/synthia-tool/src/tool_name.rs`
- Modify: `crates/synthia-core/src/lib.rs` (no change yet — Phase 5)

- [ ] **Step 1: Git-move the file**

  ```bash
  git mv crates/synthia-core/src/tool/tool_name.rs crates/synthia-tool/src/tool_name.rs
  ```

- [ ] **Step 2: Update intra-file imports**

  The file uses only `std` + `serde`. No `crate::` imports. No changes needed inside the file.

- [ ] **Step 3: Verify compilation in `synthia-core` still works** (file is no longer in core but the old path is still declared in `mod.rs`)

  Run: `cargo check -p synthia-core 2>&1 | head -30`
  Expected: error like `error[E0583]: file not found for module 'tool_name'`. This is expected — Phase 5 deletes the `pub mod tool;` line.

  Run: `cargo check -p synthia-tool 2>&1 | head -30`
  Expected: errors only about the new file not being declared in `lib.rs` (since we haven't updated `lib.rs` yet — Task 15 handles that).

  This step exists to confirm `git mv` succeeded; the actual compile check happens in Task 15.

- [ ] **Step 4: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move tool_name.rs to synthia-tool"
  ```

**Verification:** `ls crates/synthia-tool/src/tool_name.rs` exists. `ls crates/synthia-core/src/tool/tool_name.rs` does not.

### Task 5: Move `capability.rs`

**Files:**
- Move: `crates/synthia-core/src/tool/capability.rs` → `crates/synthia-tool/src/capability.rs` (standalone, 51 lines — fits the 250 LOC ceiling, no need to merge with `traits.rs`)

- [ ] **Step 1: Git-move**

  ```bash
  git mv crates/synthia-core/src/tool/capability.rs crates/synthia-tool/src/capability.rs
  ```

- [ ] **Step 2: Update intra-file imports** — file has no `crate::` imports. No changes.

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move capability.rs to synthia-tool"
  ```

### Task 6: Move `provider.rs`

**Files:**
- Move: `crates/synthia-core/src/tool/provider.rs` → `crates/synthia-tool/src/provider.rs`

- [ ] **Step 1: Git-move**

  ```bash
  git mv crates/synthia-core/src/tool/provider.rs crates/synthia-tool/src/provider.rs
  ```

- [ ] **Step 2: Update intra-file imports**

  The file imports `use crate::tool::descriptor::ToolDescriptor;` (line 7) and uses `Arc<dyn crate::tool::descriptor::Tool>` (line 22).

  Edit `crates/synthia-tool/src/provider.rs` to replace those two lines:

  ```rust
  use crate::descriptor::ToolDescriptor;
  ```

  And change the trait method signature:

  ```rust
  async fn get_tool(
      &self,
      name: &str,
  ) -> Option<Arc<dyn crate::descriptor::UnifiedToolAdapter>>;
  ```

  (Final `UnifiedToolAdapter` import path is `crate::registry::registration::adapter::UnifiedToolAdapter`; once the adapter is in place, this becomes `Arc<dyn crate::registry::registration::adapter::UnifiedToolAdapter>`. If the adapter is not yet in place at this point, use a placeholder type alias; Task 17 fixes the final form.)

  **Pragmatic choice:** keep `Arc<dyn crate::descriptor::Tool>` as a forward reference for now; the 3-method `Tool` trait is being deleted in Task 17, so the 46 KB `provider.rs` *body* keeps the 3-method shape until Task 17, at which point we switch to `UnifiedToolAdapter`. Therefore at this step, do NOT change the `get_tool` signature. Only change the import line.

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move provider.rs to synthia-tool"
  ```

### Task 7: Move `descriptor.rs` (most complex)

**Files:**
- Move: `crates/synthia-core/src/tool/descriptor.rs` → `crates/synthia-tool/src/descriptor.rs`

**Important:** This is the file that loses the 3-method `Tool` trait + 3 duplicated enums (`ToolCategory`, `ExecutionMode`, `ToolOutput`). The drop is done in Task 17, not here. At this step, move the file **as-is**, then fix intra-file imports. The 4 type deletions happen in Task 17 along with the `UnifiedToolAdapter` introduction.

- [ ] **Step 1: Git-move**

  ```bash
  git mv crates/synthia-core/src/tool/descriptor.rs crates/synthia-tool/src/descriptor.rs
  ```

- [ ] **Step 2: Update intra-file imports**

  The file imports `use crate::tool::{capability::ToolCapabilities, tool_name::ToolName};` (line 8).

  Edit to:

  ```rust
  use crate::{capability::ToolCapabilities, tool_name::ToolName};
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move descriptor.rs to synthia-tool"
  ```

### Task 8: Move `registry.rs` + merge with existing 9 KB `ToolRegistry`

**Files:**
- Modify: `crates/synthia-tool/src/registry/registration/registry.rs` (replaced by merged content)
- Move: `crates/synthia-core/src/tool/registry.rs` → `crates/synthia-tool/src/registry/registration/registry.rs` (overwrites)

**This is the most complex task.** The 9 KB `synthia_tool::registry::ToolRegistry` and the 46 KB `synthia_core::tool::registry::ToolRegistry` are merged into one file. The 9 KB version's dispatch methods (`register`, `snapshot` returning `Vec<ToolMetadataSnapshot>`, `run_with_context`, `execute_tools`, `contains`, `len`, `is_empty`, `with_max_concurrent`) are folded into the 46 KB version's struct.

**Sub-decision #3 from Task 1 determines the file split:** if the merged file is > 350 LOC, also create `dispatch.rs`. Apply that decision here.

- [ ] **Step 1: Read both files end-to-end (already done in plan generation, but re-read to confirm)**

  - `crates/synthia-core/src/tool/registry.rs` (1406 lines)
  - `crates/synthia-tool/src/registry/registration/registry.rs` (279 lines)

  Verify the imports at the top of each:

  - core `registry.rs` uses `crate::tool::{descriptor::{Tool, ToolDescriptor, ToolExposure, ToolProvenance}, provider::ToolProvider, tool_name::ToolName}`.
  - tool `registry.rs` uses `synthia_core::{Error, registry::RegistryItem}`, `super::entry::ToolEntry`, `crate::{sub_traits::ToolMetadataSnapshot, types::*}`.

- [ ] **Step 2: Build the merged file in a working file**

  Create a new file `crates/synthia-tool/src/registry/registration/registry.rs.new` with the merged content. The merge rules:

  1. **Imports:** combine the two import sets. After Task 17 finishes, the 3-method `Tool` is removed; for now (this task), keep the 46 KB's `descriptor::Tool` import (it still exists in `descriptor.rs` until Task 17).

     ```rust
     use std::{collections::HashMap, sync::Arc};

     use parking_lot::RwLock;
     use synthia_core::{Error, registry::RegistryItem};
     use tracing::Instrument;

     use super::entry::ToolEntry;
     use crate::{
         descriptor::{Tool, ToolDescriptor, ToolExposure, ToolProvenance},
         provider::ToolProvider,
         sub_traits::ToolMetadataSnapshot,
         tool_name::ToolName,
         types::*,
     };
     ```

  2. **ToolEntry struct:** the 46 KB `pub(crate) struct ToolEntry` (5 fields: `provider_id`, `provider_token`, `tool`, `identity`, `provenance`) and the 9 KB `pub struct ToolEntry` (5 fields: `tool`, `name`, `description`, `is_hidden`, `is_user_invocable`) are TWO DIFFERENT types. Keep the 9 KB version's name and shape (because `ToolEntry` is `pub` and the 9 builtin tools register through it). The 46 KB version's per-provider metadata (provider_id, provider_token, identity, provenance) becomes fields of the 46 KB `ToolRegistry`'s internal storage — not of `ToolEntry`.

     Concretely: in the 46 KB code, the local `ToolEntry` is a different struct. Rename it to `ProviderEntry` to disambiguate:

     ```rust
     /// Per-provider registration record. The 46 KB
     /// `ToolRegistry` stores `Vec<ProviderEntry>` per
     /// tool name (LIFO ordering).
     pub(crate) struct ProviderEntry {
         pub(crate) provider_id: String,
         pub(crate) provider_token: RegistrationToken,
         pub(crate) tool: Arc<dyn Tool>,
         pub(crate) identity: ToolIdentity,
         pub(crate) provenance: ToolProvenance,
     }
     ```

     The 9 KB `ToolEntry` (in `entry.rs`) keeps its current shape. The 46 KB code that currently does `inner.tools.entry(desc.name).or_default().push(entry)` (line 189) now does `inner.tools.entry(desc.name).or_default().push(ProviderEntry { ... })`.

  3. **ToolRegistry struct:** combine the 46 KB `ToolRegistry` struct fields with the 9 KB `max_concurrent` field:

     ```rust
     pub struct ToolRegistry {
         pub(crate) inner: RwLock<ToolRegistryInner>,
         next_token: RwLock<u64>,
         max_concurrent: usize,
     }

     pub(crate) struct ToolRegistryInner {
         pub(crate) tools: HashMap<ToolName, Vec<ProviderEntry>>,
         pub(crate) generation: ToolGeneration,
         pub(crate) next_registration: u64,
     }
     ```

  4. **Methods — keep all of them, naming as follows:**

     From the 46 KB version (lifecycle/identity):
     - `new()` (line 127)
     - `register_provider(provider: Arc<dyn ToolProvider>) -> Result<RegistrationToken, RegistrationError>` (line 139)
     - `unregister(token: RegistrationToken)` (line 199, see 46 KB)
     - `materialize() -> Materialization` (46 KB)
     - `resolve(name) -> Result<Arc<dyn Tool>, StaleOrUnknown>` (line 279)
     - `resolve_now(name) -> Option<Arc<dyn Tool>>` (line 312)
     - `register(...)` (provider-style, 46 KB)
     - `RegistrationScope` (RAII helper, 46 KB)
     - `Materialization` struct (line 53) — its `tools: HashMap<ToolName, Arc<dyn Tool>>` field becomes `HashMap<ToolName, Arc<crate::descriptor::UnifiedToolAdapter>>` — **but at this step, keep `Arc<dyn Tool>` since adapter is not yet in place**. Task 17 fixes this.

     From the 9 KB version (dispatch):
     - `with_max_concurrent(max)` (line 71) — keep
     - `register(item: ToolEntry)` (line 81) — **rename to `register_entry`** to disambiguate from the 46 KB `register_provider`. The 9 builtin tool registration path uses `register_entry`.
     - `snapshot() -> Vec<ToolMetadataSnapshot>` (line 113) — keep, but rename to `metadata_snapshots` to avoid confusion with the 46 KB `Materialization::snapshot()` (if it has one — verify during merge). If no name collision, keep `snapshot`.
     - `run_with_context(tool_uses, context) -> Result<Vec<ToolOutput>>` (line 121) — keep
     - `execute_tools(...)` (line 175, private) — keep
     - `contains(name)` (line 256) — keep
     - `len()` (line 261) — keep
     - `is_empty()` (line 266) — keep
     - `Clone` impl (line 271) — keep, update to also clone `next_token` and `max_concurrent`
     - `Default` impl (line 52) — keep

  5. **Tests:** the 46 KB version has `mod tests` at the bottom (lines 561-1406) with 31 test functions. The 9 KB version's tests live in `registry_trait.rs` (separate file). The 46 KB `mod tests` block stays at the bottom of the merged file. The 9 KB `Clone` test (if any in the current file) — the current `registry.rs` has no `#[cfg(test)]` block (tests live in `registry_trait.rs` and `entry.rs`).

- [ ] **Step 3: Apply the file split if sub-decision #3 chose it**

  If the merged file exceeds 350 LOC, split:

  - `registry.rs` keeps: `ToolRegistry` struct, `ToolRegistryInner` struct, `ToolGeneration`, `ToolIdentity`, `RegistrationToken`, `RegistrationError`, `ProviderEntry`, `Materialization`, `StaleOrUnknown`, `RegistrationScope`, `new()`, `default()`, `with_max_concurrent()`, `register_provider()`, `unregister()`, `materialize()`, `resolve()`, `resolve_now()`, `register_entry()`, `len()`, `is_empty()`, `contains()`, `Clone` impl.
  - `dispatch.rs` gets: `run_with_context()`, `execute_tools()`, `metadata_snapshots()` (or `snapshot()`), the `Clone` impl stays in `registry.rs`.

  Update the `mod.rs` to declare `pub(super) mod dispatch;`.

- [ ] **Step 4: Overwrite the destination file**

  ```bash
  git mv crates/synthia-core/src/tool/registry.rs /tmp/synthia-core-registry-original.rs
  mv crates/synthia-tool/src/registry/registration/registry.rs /tmp/synthia-tool-registry-original.rs
  mv /tmp/synthia-core-registry-original.rs crates/synthia-tool/src/registry/registration/registry.rs
  ```

  Then write the merged content to `crates/synthia-tool/src/registry/registration/registry.rs` (and `dispatch.rs` if split was chosen).

- [ ] **Step 5: Verify compilation will be attempted in Task 15** — defer the actual `cargo check` to Task 15 (which adds the `pub mod` declarations in `lib.rs`).

- [ ] **Step 6: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): merge core 46KB ToolRegistry into synthia-tool"
  ```

**Verification:** `wc -l crates/synthia-tool/src/registry/registration/registry.rs` matches the expected merged LOC (≈ 1400 + 280 = 1680 lines, or split between `registry.rs` and `dispatch.rs`).

### Task 9: Move `fragment.rs` + create `fragment/` directory

**Files:**
- Create directory: `crates/synthia-tool/src/fragment/`
- Move: `crates/synthia-core/src/tool/fragment.rs` → `crates/synthia-tool/src/fragment/mod.rs`

- [ ] **Step 1: Create directory and git-move**

  ```bash
  mkdir -p crates/synthia-tool/src/fragment
  git mv crates/synthia-core/src/tool/fragment.rs crates/synthia-tool/src/fragment/mod.rs
  ```

- [ ] **Step 2: Update intra-file imports**

  The file imports `crate::tool::tool_name::ToolName` (verify with grep).

  Edit to `use crate::tool_name::ToolName;` (now at the crate root, since `tool_name.rs` is at `synthia-tool/src/tool_name.rs`).

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move fragment.rs to synthia-tool::fragment"
  ```

### Task 10: Move `builtin_fragments.rs`

**Files:**
- Move: `crates/synthia-core/src/tool/builtin_fragments.rs` → `crates/synthia-tool/src/fragment/builtin_fragments.rs`

- [ ] **Step 1: Git-move**

  ```bash
  git mv crates/synthia-core/src/tool/builtin_fragments.rs crates/synthia-tool/src/fragment/builtin_fragments.rs
  ```

- [ ] **Step 2: Update intra-file imports**

  The file imports `crate::tool::fragment::{ContextFragment, FragmentContext, FragmentRegistry}` and `crate::tool::tool_name::ToolName`.

  Edit to:
  ```rust
  use crate::{
      fragment::{ContextFragment, FragmentContext, FragmentRegistry},
      tool_name::ToolName,
  };
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move builtin_fragments.rs to synthia-tool::fragment"
  ```

### Task 11: Move `skill_registry.rs` + create `skill/` directory

**Files:**
- Create directory: `crates/synthia-tool/src/skill/`
- Move: `crates/synthia-core/src/tool/skill_registry.rs` → `crates/synthia-tool/src/skill/mod.rs`

- [ ] **Step 1: Create directory and git-move**

  ```bash
  mkdir -p crates/synthia-tool/src/skill
  git mv crates/synthia-core/src/tool/skill_registry.rs crates/synthia-tool/src/skill/mod.rs
  ```

- [ ] **Step 2: Update intra-file imports**

  The file imports `super::tool_name::ToolName`.

  Edit to `use crate::tool_name::ToolName;`.

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move skill_registry.rs to synthia-tool::skill"
  ```

### Task 12: Move `builtin_skills.rs`

**Files:**
- Move: `crates/synthia-core/src/tool/builtin_skills.rs` → `crates/synthia-tool/src/skill/builtin_skills.rs`

- [ ] **Step 1: Git-move**

  ```bash
  git mv crates/synthia-core/src/tool/builtin_skills.rs crates/synthia-tool/src/skill/builtin_skills.rs
  ```

- [ ] **Step 2: Update intra-file imports**

  The file imports `crate::tool::skill_registry::{Skill, SkillRegistry}` and `crate::tool::tool_name::ToolName`.

  Edit to:
  ```rust
  use crate::{
      skill::{Skill, SkillRegistry},
      tool_name::ToolName,
  };
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move builtin_skills.rs to synthia-tool::skill"
  ```

### Task 13: Move `plugin.rs`, `extension.rs`, `rollout.rs`, `subagent.rs` (parallel)

**Files:**
- Move: `crates/synthia-core/src/tool/plugin_registry.rs` → `crates/synthia-tool/src/plugin.rs`
- Move: `crates/synthia-core/src/tool/extension_registry.rs` → `crates/synthia-tool/src/extension.rs`
- Move: `crates/synthia-core/src/tool/rollout.rs` → `crates/synthia-tool/src/rollout.rs`
- Move: `crates/synthia-core/src/tool/subagent.rs` → `crates/synthia-tool/src/subagent.rs`

- [ ] **Step 1: Git-move all four**

  ```bash
  git mv crates/synthia-core/src/tool/plugin_registry.rs crates/synthia-tool/src/plugin.rs
  git mv crates/synthia-core/src/tool/extension_registry.rs crates/synthia-tool/src/extension.rs
  git mv crates/synthia-core/src/tool/rollout.rs crates/synthia-tool/src/rollout.rs
  git mv crates/synthia-core/src/tool/subagent.rs crates/synthia-tool/src/subagent.rs
  ```

- [ ] **Step 2: Update intra-file imports in each**

  **`plugin.rs`** — was `plugin_registry.rs`:
  ```bash
  grep -n "crate::tool::" crates/synthia-tool/src/plugin.rs
  ```

  Replace `crate::tool::fragment::ContextFragment` → `crate::fragment::ContextFragment`, etc., for every `crate::tool::` line. The file's `use super::{...}` lines point to other `synthia-core/src/tool/*` files — those now have new homes:

  - `super::provider::ToolProvider` → `crate::provider::ToolProvider`
  - `super::registry::{RegistrationScope, ToolRegistry}` → `crate::registry::{RegistrationScope, ToolRegistry}`
  - `super::skill_registry::{Skill, SkillRegistry}` → `crate::skill::{Skill, SkillRegistry}`
  - `super::fragment::{FragmentContext, FragmentRegistry, ...}` → `crate::fragment::{...}`

  **`extension.rs`** — was `extension_registry.rs`:
  Same pattern. `super::fragment::FragmentRegistry` → `crate::fragment::FragmentRegistry`, `super::registry::ToolRegistry` → `crate::registry::ToolRegistry`, etc.

  **`rollout.rs`**:
  No `crate::tool::` imports (verify with grep). If clean, no edits needed.

  **`subagent.rs`**:
  No `crate::tool::` imports. If clean, no edits needed.

- [ ] **Step 3: Commit all four in one commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): move plugin, extension, rollout, subagent modules to synthia-tool"
  ```

**Verification:** `git log --oneline -1` shows the move commit. `ls crates/synthia-core/src/tool/` shows only `mod.rs` and `output_bound.rs` left.

---

## Phase 3 — Collision Resolution

### Task 14: Delete `output_bound.rs` (Decision C1)

**Files:**
- Delete: `crates/synthia-core/src/tool/output_bound.rs` (file-level near-duplicate of `synthia-tool/src/truncate/output_bound.rs`)

- [ ] **Step 1: Verify no callers**

  ```bash
  git grep "synthia_core::tool::output_bound" -- 'crates/*' 'synthia-cli/*' 'test-support/*'
  ```

  Expected: empty (no callers). If any caller exists, fix the import path and re-run. (The audit confirmed this — the only inline path is `synthia_core::tool::OutputBound` without the `output_bound::` qualifier, addressed in Task 18.)

- [ ] **Step 2: Git-delete**

  ```bash
  git rm crates/synthia-core/src/tool/output_bound.rs
  ```

- [ ] **Step 3: Commit**

  ```bash
  git commit -m "refactor(tool): delete core output_bound (single source: synthia_tool::truncate)"
  ```

### Task 15: Add `pub mod` declarations to `synthia-tool/src/lib.rs`

**Files:**
- Modify: `crates/synthia-tool/src/lib.rs`

This task declares all the new modules and re-exports. It is the first task that actually compiles the moved code.

- [ ] **Step 1: Read the current `crates/synthia-tool/src/lib.rs`**

  Current content (24 lines):
  ```rust
  pub mod builtin;
  pub mod events;
  pub mod registry;
  pub mod sub_traits;
  pub mod traits;
  pub mod truncate;
  pub mod types;

  #[cfg(test)]
  mod tool_test;
  #[cfg(test)]
  mod types_test;

  pub use events::FileChangeEvent;
  pub use registry::{ToolEntry, ToolRegistry};
  pub use sub_traits::{
      ToolCategory,
      ToolDefinition,
      ToolExecution,
      ToolLifecycle,
      ToolMetadataSnapshot,
  };
  pub use traits::*;
  pub use types::*;
  ```

- [ ] **Step 2: Add the new modules in alphabetical order (after `builtin` and before `events`)**

  Replace the entire `pub mod` block:

  ```rust
  pub mod builtin;
  pub mod capability;
  pub mod descriptor;
  pub mod events;
  pub mod extension;
  pub mod fragment;
  pub mod plugin;
  pub mod provider;
  pub mod registry;
  pub mod rollout;
  pub mod skill;
  pub mod sub_traits;
  pub mod subagent;
  pub mod tool_name;
  pub mod traits;
  pub mod truncate;
  pub mod types;
  ```

- [ ] **Step 3: Extend the re-export list**

  The default `pub use ...` lines should expose the same public API the codebase currently has via `synthia_core::tool::*`. The re-exports are:

  ```rust
  pub use capability::{CapabilityBroker, ToolCapabilities};
  pub use descriptor::{
      CancelBehavior, ContextSource, ToolContext, ToolDescriptor, ToolError,
      ToolExample, ToolExposure, ToolInput, ToolMetadata, ToolProvenance,
  };
  // ToolError is conditional on sub-decision #1 (see Task 17)
  pub use events::FileChangeEvent;
  pub use extension::{
      CommandStore, ExtensionError, ExtensionRegistry, ExtensionState,
      HealthCheckResult, ProviderStore,
  };
  pub use fragment::{ContextFragment, FragmentContext, FragmentError, FragmentRegistry};
  pub use plugin::{
      DiscoveredPlugin, Plugin, PluginCapabilitySummary, PluginDescriptor,
      PluginDescriptorId, PluginError, PluginRegistry, PluginState,
  };
  pub use provider::{ToolEvent, ToolProvider};
  pub use registry::{RegistrationScope, ToolGeneration, ToolIdentity, ToolRegistry, ToolEntry};
  pub use rollout::{ChangeType, FileChange, RolloutSummary, RolloutTracker, TokenBudget};
  pub use skill::{Skill, SkillActivation, SkillError, SkillProvenance, SkillRegistry};
  pub use sub_traits::{
      ToolCategory,
      ToolDefinition,
      ToolExecution,
      ToolLifecycle,
      ToolMetadataSnapshot,
  };
  pub use subagent::{SubagentFactory, SubagentOutput, SubagentSpawnError};
  pub use tool_name::ToolName;
  pub use traits::*;
  pub use types::*;
  ```

  Conditional based on sub-decisions:
  - If `sub-decision #1` chose "fold into `synthia_core::Error`": drop `ToolError` from the `descriptor` re-export.
  - If `sub-decision #3` chose "split into `registry.rs` + `dispatch.rs`": no change to re-exports (just internal file split).

- [ ] **Step 4: Verify compilation**

  Run: `cargo check -p synthia-tool 2>&1 | head -100`
  Expected: many errors. **This is expected at this point** — Task 16 introduces `UnifiedToolAdapter`, Task 17 deletes the 3-method `Tool` trait + 3 duplicated enums. Do not proceed to next task until `cargo check -p synthia-tool` succeeds. If it succeeds, great. If it fails with errors, the errors should be limited to:
  - `descriptor::Tool` not existing (because Task 17 hasn't deleted it yet — at this point it should still exist; errors here mean an import path is wrong).
  - `Arc<dyn Tool>` mismatches because `UnifiedToolAdapter` is not yet defined (Task 16).

  Resolve errors one at a time, then re-run.

- [ ] **Step 5: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): declare moved modules in synthia-tool lib.rs"
  ```

### Task 16: Introduce `UnifiedToolAdapter` (Section 5.5)

**Files:**
- Create: `crates/synthia-tool/src/registry/registration/adapter.rs`

This is the bridge that lets the 46 KB `ToolRegistry` keep using `Arc<dyn Tool>`-shaped types while the 7-method `synthia_tool::Tool` is the only canonical trait.

- [ ] **Step 1: Decide cache strategy (sub-decision #5)**

  Apply the decision from Task 1 step 5.

- [ ] **Step 2: Create the file**

  Write `crates/synthia-tool/src/registry/registration/adapter.rs`:

  ```rust
  //! Adapter wrapping a 7-method [`Tool`] so the 46 KB
  //! `ToolRegistry` body code can hold it as a 3-method-style
  //! tool.
  //!
  //! The 7-method `Tool` trait is the single canonical trait
  //! for the entire codebase. The 46 KB `ToolRegistry`
  //! (formerly `synthia_core::tool::registry`) was originally
  //! written against a 3-method `Tool` trait that has been
  //! deleted. Rather than rewrite the 46 KB body, this
  //! adapter implements the same shape the 46 KB code
  //! expects, by delegating to the 7-method `Tool`.

  use std::sync::Arc;

  use crate::{
      descriptor::{
          ToolContext, ToolDescriptor, ToolError, ToolInput, ToolOutput,
      },
      traits::Tool,
  };

  /// Adapter: 7-method `Tool` + cached descriptor → 3-method shape.
  pub struct UnifiedToolAdapter {
      inner: Arc<dyn Tool>,
      descriptor: ToolDescriptor,
  }

  impl UnifiedToolAdapter {
      /// Construct an adapter. `descriptor` is either computed
      /// eagerly here (sub-decision #5 = eager) or computed
      /// lazily in `descriptor()` (sub-decision #5 = lazy).
      pub fn new(inner: Arc<dyn Tool>, descriptor: ToolDescriptor) -> Self {
          Self { inner, descriptor }
      }

      /// The 3-method name. Returns the full namespaced name.
      pub fn name(&self) -> &str {
          &self.descriptor.name.full_name()
      }

      /// The 3-method descriptor accessor.
      pub fn descriptor(&self) -> &ToolDescriptor {
          &self.descriptor
      }

      /// The 3-method execute. Bridges to 7-method `call`.
      pub async fn execute(
          &self,
          input: ToolInput,
          ctx: &ToolContext,
      ) -> Result<ToolOutput, ToolError> {
          // Build the 7-method Context from the 3-method ToolContext.
          let sctx = crate::types::Context {
              session_id: ctx.session_id.clone(),
              workspace_root: ctx.workspace_root.clone(),
              output_bound: None,
              ..crate::types::Context::default()
          };
          // Delegate to 7-method call.
          let out = self.inner.call(input.raw.clone(), &sctx).await;
          // Bridge the error flag.
          if out.is_error.unwrap_or(false) {
              let msg = out
                  .content
                  .iter()
                  .filter_map(|p| p.text())
                  .collect::<Vec<_>>()
                  .join("\n");
              Err(ToolError::ExecutionFailed(msg))
          } else {
              Ok(out)
          }
      }
  }
  ```

  The exact `Context` field initialization depends on the `Context` struct's public API. Read `crates/synthia-tool/src/types.rs` to confirm field names. If `Context` has a builder, use it.

- [ ] **Step 3: Add the new module to `registry/registration/mod.rs`**

  Edit `crates/synthia-tool/src/registry/registration/mod.rs` to add:

  ```rust
  pub mod adapter;
  pub use adapter::UnifiedToolAdapter;
  ```

- [ ] **Step 4: Verify compilation of the adapter alone**

  Run: `cargo check -p synthia-tool 2>&1 | head -50`
  Expected: errors are limited to the 46 KB `registry.rs` body code (still using `Arc<dyn Tool>` from `descriptor`) and to the `provider.rs` body (still using `Arc<dyn Tool>`). Adapter itself compiles.

  If the adapter has errors, fix them. The most likely issue is the `Context` field names — read `types.rs` to confirm.

- [ ] **Step 5: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): add UnifiedToolAdapter (7↔3 method bridge)"
  ```

### Task 17: Delete 3-method `Tool` + 3 duplicated enums (Section 5.1, 5.3, 5.4)

**Files:**
- Modify: `crates/synthia-tool/src/descriptor.rs` (delete `Tool` trait, `ToolCategory`, `ExecutionMode`, `ToolOutput` definitions)
- Modify: `crates/synthia-tool/src/registry/registration/registry.rs` (replace `Arc<dyn Tool>` with `Arc<UnifiedToolAdapter>` in 9 places)
- Modify: `crates/synthia-tool/src/provider.rs` (replace `Arc<dyn Tool>` return type with `Arc<UnifiedToolAdapter>`)
- Modify: `crates/synthia-tool/src/plugin.rs` (same)

- [ ] **Step 1: Apply sub-decision #1 (`ToolError`)**

  Either:
  - Drop `ToolError` from `descriptor.rs` and use `synthia_core::Error` in `UnifiedToolAdapter::execute`; **OR**
  - Keep `ToolError` and ensure `UnifiedToolAdapter::execute` returns `Result<ToolOutput, ToolError>`.

  Apply the choice from Task 1 step 1.

- [ ] **Step 2: Delete from `descriptor.rs`**

  Edit `crates/synthia-tool/src/descriptor.rs` and remove:
  1. The 3-method `pub trait Tool: Send + Sync + 'static { ... }` block (lines 101-119 of the original).
  2. The 3-method `pub enum ToolCategory { ... }` (lines 192-205 of the original).
  3. The 3-method `pub enum ExecutionMode { ... }` (lines 207-217 of the original).
  4. The 3-method `pub struct ToolOutput { ... }` + its `impl ToolOutput` (lines 22-41 of the original).

  Important: do NOT delete `ToolError`, `ToolContext`, `ToolInput`, `ToolDescriptor`, etc. — those are absorbed types that the adapter and the 46 KB body still use.

- [ ] **Step 3: Replace `Arc<dyn Tool>` in `registry.rs` (9 places)**

  In `crates/synthia-tool/src/registry/registration/registry.rs`:

  - Line 47: `pub(crate) tool: Arc<dyn Tool>` → `pub(crate) tool: Arc<UnifiedToolAdapter>` (inside the `ProviderEntry` struct renamed in Task 8).
  - Line 58: `tools: HashMap<ToolName, Arc<dyn Tool>>` → `HashMap<ToolName, Arc<UnifiedToolAdapter>>` (inside `Materialization`).
  - Line 146: `let resolved: Vec<(ToolDescriptor, Arc<dyn Tool>)>` → `Vec<(ToolDescriptor, Arc<UnifiedToolAdapter>)>`.
  - Line 279: `Result<Arc<dyn Tool>, StaleOrUnknown>` → `Result<Arc<UnifiedToolAdapter>, StaleOrUnknown>`.
  - Line 312: `Option<Arc<dyn Tool>>` → `Option<Arc<UnifiedToolAdapter>>`.
  - Lines 579 and 627 (test only): the local `TestTool` struct implements the 3-method `Tool` trait. Rewrite it to implement the 7-method `Tool` trait (or implement a small inline `TestTool7` that wraps a `TestTool` via `UnifiedToolAdapter::new`). Read both call-sites to understand the test's `TestTool` shape, then rewrite.

  Add to the imports at the top of the file:

  ```rust
  use super::adapter::UnifiedToolAdapter;
  ```

- [ ] **Step 4: Replace `Arc<dyn Tool>` in `provider.rs` (1 place)**

  In `crates/synthia-tool/src/provider.rs`:

  - Line 22: `Option<Arc<dyn crate::tool::descriptor::Tool>>` → `Option<Arc<crate::registry::registration::UnifiedToolAdapter>>`.

  Add to the imports:

  ```rust
  use crate::registry::registration::UnifiedToolAdapter;
  ```

  Note: this changes the public signature of `ToolProvider::get_tool`. It is a breaking change for any external implementor. Per the audit (bg_b936be29), the only implementor is `PluginRegistry` in `synthia-plugin`, plus the test mocks. The plugin registry implementation is updated in step 5.

- [ ] **Step 5: Replace `Arc<dyn Tool>` in `plugin.rs` (1 place)**

  In `crates/synthia-tool/src/plugin.rs`:

  - Line 697: `Option<Arc<dyn Tool>>` → `Option<Arc<UnifiedToolAdapter>>` (inside the `PluginRegistry::get_tool` method).

  The method body currently returns `provider.get_tool(name).await`. Wrap the inner `Arc<dyn 7-method Tool>` in `UnifiedToolAdapter::new(...)` before returning.

  Add to the imports:

  ```rust
  use crate::registry::registration::UnifiedToolAdapter;
  ```

- [ ] **Step 6: Verify compilation**

  Run: `cargo check -p synthia-tool 2>&1 | head -100`
  Expected: 0 errors. If errors, fix them — most likely the `TestTool` test struct in `registry.rs` needs rewriting.

- [ ] **Step 7: Run tool tests**

  Run: `cargo test -p synthia-tool 2>&1 | tail -50`
  Expected: 0 failures. The 31 tests from the 46 KB `registry.rs` and 12+ tests from `fragment.rs`, `plugin.rs`, `extension.rs`, `rollout.rs`, `tool_name.rs`, `builtin_fragments.rs`, `builtin_skills.rs`, `skill/mod.rs` all run.

- [ ] **Step 8: Run clippy**

  Run: `cargo clippy -p synthia-tool --all-targets --all-features --tests 2>&1 | tail -30`
  Expected: 0 warnings. Fix any.

- [ ] **Step 9: Commit**

  ```bash
  git add -A
  git commit -m "refactor(tool): collapse 3-method Tool + 3 enums into synthia_tool (with adapter)"
  ```

  In the commit message, document the 5 sub-decisions from Task 1 (e.g., "folded ToolError into synthia_core::Error; absorbed descriptor into traits.rs; single registry.rs; ToolEvent kept; lazy descriptor").

**Verification:** `cargo check -p synthia-tool` passes. `cargo test -p synthia-tool` passes. `cargo clippy -p synthia-tool` clean.

---

## Phase 4 — Downstream Call-Site Updates

### Task 18: Update `synthia-agent` call-sites (7 files)

**Files:**
- Modify: `crates/synthia-agent/src/agent.rs` (1 line)
- Modify: `crates/synthia-agent/src/loop_context.rs` (1 line)
- Modify: `crates/synthia-agent/src/loop_services.rs` (1 import + 3 inline paths)
- Modify: `crates/synthia-agent/src/component_assembly.rs` (1 import block + 1 inline path + drop alias)
- Modify: `crates/synthia-agent/src/config/agent_config/run_config.rs` (1 import block)
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` (2 import blocks)
- Modify: `crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs` (1 inline path)

- [ ] **Step 1: Update each file in order**

  For each file, perform the edits listed below.

  **`crates/synthia-agent/src/agent.rs`** (L4):
  ```rust
  // before
  use synthia_core::tool::extension_registry::ExtensionRegistry;
  // after
  use synthia_tool::extension::ExtensionRegistry;
  ```

  **`crates/synthia-agent/src/loop_context.rs`** (L4):
  ```rust
  // before
  use synthia_core::tool::registry::RegistrationScope;
  // after
  use synthia_tool::registry::RegistrationScope;
  ```

  **`crates/synthia-agent/src/loop_services.rs`** (L11, L49, L71, L186):
  ```rust
  // before (L11)
  use synthia_core::tool::rollout::RolloutTracker;
  // after
  use synthia_tool::rollout::RolloutTracker;

  // before (L49, L71, L186 inline path)
  Option<synthia_core::tool::OutputBound>
  // after
  Option<synthia_tool::truncate::OutputBound>

  // Also: L186 `OutputBound::default()` stays as `synthia_tool::truncate::OutputBound::default()`.
  ```

  **`crates/synthia-agent/src/component_assembly.rs`** (L7-10, L18, L112):
  ```rust
  // before (L7-10)
  use synthia_core::tool::{
      extension_registry::{ExtensionRegistry, ProviderStore},
      fragment::FragmentRegistry,
  };
  // after
  use synthia_tool::{
      extension::{ExtensionRegistry, ProviderStore},
      fragment::FragmentRegistry,
  };

  // before (L18) `use synthia_tool::ToolRegistry;` — no change.

  // before (L112)
  Arc::new(synthia_core::tool::registry::ToolRegistry::new());
  // after
  Arc::new(synthia_tool::registry::ToolRegistry::new());

  // Drop `as CoreToolRegistry` alias: search for `CoreToolRegistry` in this file
  // and remove the alias definition (if any). The alias was in the import block.
  ```

  **`crates/synthia-agent/src/config/agent_config/run_config.rs`** (L7-10):
  ```rust
  // before
  use synthia_core::tool::{
      extension_registry::ExtensionRegistry,
      rollout::RolloutTracker,
  };
  // after
  use synthia_tool::{
      extension::ExtensionRegistry,
      rollout::RolloutTracker,
  };
  ```

  **`crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`** (L14-17, L1273-1279):
  ```rust
  // before (L14-17)
  use synthia_core::tool::{
      fragment::FragmentContext,
      rollout::{ChangeType, FileChange},
  };
  // after
  use synthia_tool::{
      fragment::FragmentContext,
      rollout::{ChangeType, FileChange},
  };

  // before (L1273-1279 — test inner block)
  use synthia_core::tool::{
      builtin_fragments::SystemPromptFragment,
      extension_registry::ExtensionRegistry,
      fragment::FragmentRegistry,
      registry::ToolRegistry as CoreToolRegistry,
      rollout::{ChangeType, FileChange, RolloutTracker},
  };
  // after
  use synthia_tool::{
      extension::ExtensionRegistry,
      fragment::{FragmentRegistry, SystemPromptFragment},
      registry::ToolRegistry,
      rollout::{ChangeType, FileChange, RolloutTracker},
  };
  ```

  Note: `SystemPromptFragment` is in `synthia_tool::fragment::builtin_fragments::SystemPromptFragment` (moved in Task 10). Adjust the import.

  **`crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs`** (L89):
  ```rust
  // before
  output_bound: Option<&synthia_core::tool::OutputBound>,
  // after
  output_bound: Option<&synthia_tool::truncate::OutputBound>,
  ```

- [ ] **Step 2: Verify compilation**

  Run: `cargo check -p synthia-agent 2>&1 | head -50`
  Expected: 0 errors. If errors, fix them — most likely a missed import or a remaining `CoreToolRegistry` reference.

- [ ] **Step 3: Run agent tests**

  Run: `cargo test -p synthia-agent 2>&1 | tail -50`
  Expected: 0 failures. (The audit listed 7 src + 16 test files; only 7 src need import updates.)

- [ ] **Step 4: Run clippy**

  Run: `cargo clippy -p synthia-agent --all-targets --all-features --tests 2>&1 | tail -30`
  Expected: 0 warnings.

- [ ] **Step 5: Commit**

  ```bash
  git add -A
  git commit -m "refactor(agent): migrate synthia_core::tool imports to synthia_tool"
  ```

### Task 19: Update `synthia-server` and `synthia-cli` call-sites (4 files)

**Files:**
- Modify: `crates/synthia-server/src/session/controller.rs` (1 import block)
- Modify: `crates/synthia-server/src/state/app_state.rs` (2 import blocks + 2 inner blocks + 6 inline `Arc<dyn ...>` + drop alias)
- Modify: `crates/synthia-server/src/routes/skills.rs` (1 doc-comment line)
- Modify: `crates/synthia-server/tests/e2e_registry_pipeline_test.rs` (1 inline path)
- Modify: `synthia-cli/src/repl_core/repl/agent_message.rs` (1 import line)

- [ ] **Step 1: Update `controller.rs` (L30-33)**

  ```rust
  // before
  use synthia_core::tool::{
      extension_registry::ExtensionRegistry,
      rollout::RolloutTracker,
  };
  // after
  use synthia_tool::{
      extension::ExtensionRegistry,
      rollout::RolloutTracker,
  };
  ```

- [ ] **Step 2: Update `app_state.rs` (L12-19, L28, L169-173, L176/181/186, L278-282, L285/290/295)**

  ```rust
  // before (L12-19)
  use synthia_core::tool::{
      builtin_fragments::SystemPromptFragment,
      extension_registry::ExtensionRegistry,
      fragment::FragmentRegistry,
      plugin_registry::PluginRegistry,
      registry::ToolRegistry as CoreToolRegistry,
      rollout::RolloutTracker,
      skill_registry::SkillRegistry,
  };
  // after
  use synthia_tool::{
      extension::ExtensionRegistry,
      fragment::{FragmentRegistry, SystemPromptFragment},
      plugin::PluginRegistry,
      registry::ToolRegistry,
      rollout::RolloutTracker,
      skill::SkillRegistry,
  };

  // before (L28) `use synthia_tool::registry::ToolRegistry;` — no change, but the alias `CoreToolRegistry` is now unused. Remove the alias from the import block above (already done by the rewrite).

  // before (L169-173, L278-282)
  use synthia_core::tool::builtin_skills::{
      CodingSkill, DebugSkill, SearchSkill,
  };
  // after
  use synthia_tool::skill::builtin_skills::{
      CodingSkill, DebugSkill, SearchSkill,
  };

  // before (L176/181/186, L285/290/295)
  Arc<dyn synthia_core::tool::skill_registry::Skill>
  // after (6 places)
  Arc<dyn synthia_tool::skill::Skill>
  ```

- [ ] **Step 3: Update `routes/skills.rs` doc-comment (L271)**

  Find the doc-comment containing `synthia_core::tool::SkillRegistry` and replace with `synthia_tool::skill::SkillRegistry`.

- [ ] **Step 4: Update `e2e_registry_pipeline_test.rs` (L236)**

  ```rust
  // before
  synthia_core::tool::fragment::FragmentContext::new(session_id, user_id);
  // after
  synthia_tool::fragment::FragmentContext::new(session_id, user_id);
  ```

- [ ] **Step 5: Update `synthia-cli/src/repl_core/repl/agent_message.rs` (L20)**

  ```rust
  // before
  use synthia_core::tool::extension_registry::ExtensionRegistry;
  // after
  use synthia_tool::extension::ExtensionRegistry;
  ```

- [ ] **Step 6: Verify compilation across all touched crates**

  Run each, expect exit 0:
  ```bash
  cargo check -p synthia-server
  cargo check -p synthia-cli
  ```

- [ ] **Step 7: Run tests**

  Run each, expect exit 0:
  ```bash
  cargo test -p synthia-server
  cargo test -p synthia-cli
  ```

- [ ] **Step 8: Run clippy**

  Run each, expect exit 0:
  ```bash
  cargo clippy -p synthia-server --all-targets --all-features --tests
  cargo clippy -p synthia-cli --all-targets --all-features --tests
  ```

- [ ] **Step 9: Commit**

  ```bash
  git add -A
  git commit -m "refactor(server,cli): migrate synthia_core::tool imports to synthia_tool"
  ```

---

## Phase 5 — Delete the Empty `tool` Module + Final Verification

### Task 20: Delete `synthia-core::tool` module declaration

**Files:**
- Modify: `crates/synthia-core/src/lib.rs` (delete `pub mod tool;` line)
- Delete: `crates/synthia-core/src/tool/mod.rs`
- Delete: `crates/synthia-core/src/tool/` (now empty directory)

- [ ] **Step 1: Delete the module declaration**

  Edit `crates/synthia-core/src/lib.rs` and remove line 13 (`pub mod tool;`).

- [ ] **Step 2: Git-delete the empty directory**

  ```bash
  git rm -r crates/synthia-core/src/tool/
  ```

  This removes `mod.rs` and the (now-empty) `tool/` directory.

- [ ] **Step 3: Verify the audit grep is clean**

  Run: `git grep "synthia_core::tool::" -- 'crates/*' 'synthia-cli/*' 'test-support/*'`
  Expected: zero matches in source files. The only acceptable match is in `crates/synthia-tool/src/sub_traits/category.rs` (doc-comment, to be updated next).

- [ ] **Step 4: Update the stale doc-comment in `sub_traits/category.rs`**

  Edit `crates/synthia-tool/src/sub_traits/category.rs` lines 4 and 8. Replace "mirrors synthia_core::tool::descriptor::ToolCategory" with "The canonical ToolCategory for Synthia tool categorization." (or remove the line entirely if the surrounding comment becomes redundant).

- [ ] **Step 5: Verify the audit grep is now fully clean**

  Run: `git grep "synthia_core::tool::" -- 'crates/*' 'synthia-cli/*' 'test-support/*'`
  Expected: zero matches.

- [ ] **Step 6: Commit**

  ```bash
  git add -A
  git commit -m "refactor(core): delete tool module; finalize tool subsystem migration"
  ```

### Task 21: Workspace-wide verification

**Files:** none (verification only)

- [ ] **Step 1: Format**

  Run: `cargo +nightly fmt --all`
  Expected: 0 changes on re-run (i.e., already formatted). If changes are made, this is a no-op for our migration — fmt would have caught indentation issues, but since we have not edited any whitespace-sensitive code, it should be a no-op.

  Verify: `cargo +nightly fmt --all -- --check`
  Expected: exit 0.

- [ ] **Step 2: Lint**

  Run: `cargo clippy --all-targets --all-features --tests --all 2>&1 | tail -50`
  Expected: 0 warnings. Fix any.

- [ ] **Step 3: Per-crate checks (all 13 workspace members)**

  Run each, expect exit 0:
  ```bash
  cargo check -p synthia-core
  cargo check -p synthia-tool
  cargo check -p synthia-telemetry
  cargo check -p synthia-hook
  cargo check -p synthia-provider
  cargo check -p synthia-context
  cargo check -p synthia-skill
  cargo check -p synthia-session
  cargo check -p synthia-agent
  cargo check -p synthia-a2a
  cargo check -p synthia-server
  cargo check -p synthia-cache-mark
  cargo check -p test-support
  ```

- [ ] **Step 4: Per-crate tests (8 crates with tests)**

  Run each, expect exit 0:
  ```bash
  cargo test -p synthia-core
  cargo test -p synthia-tool
  cargo test -p synthia-provider
  cargo test -p synthia-context
  cargo test -p synthia-skill
  cargo test -p synthia-agent
  cargo test -p synthia-server
  cargo test -p test-support
  ```

  Per project rule: do NOT use `cargo test --workspace`.

- [ ] **Step 5: CLI workspace check**

  Run: `cargo check -p synthia-cli`
  Expected: exit 0 (the `synthia-cli` crate is a separate workspace; build it directly).

- [ ] **Step 6: Cycle confirmation**

  Run: `cargo tree --workspace --no-default-features > /tmp/synthia-post-migration-tree.txt`
  Then: `diff /tmp/synthia-pre-migration-tree.txt /tmp/synthia-post-migration-tree.txt`
  Expected: 0 differences in the `synthia-(core|tool|agent|server)` subgraph. Any difference is a cycle or new edge — STOP and investigate.

- [ ] **Step 7: Commit any formatting fixes**

  If `cargo +nightly fmt --all` made changes, commit them:

  ```bash
  git add -A
  git commit -m "style: cargo fmt after tool migration"
  ```

  (This is a no-op commit if fmt did not change anything.)

- [ ] **Step 8: Final audit**

  Run:
  ```bash
  git grep "synthia_core::tool::" -- 'crates/*' 'synthia-cli/*' 'test-support/*'
  ```

  Expected: zero matches.

  Run: `ls crates/synthia-core/src/tool/ 2>&1`
  Expected: `No such file or directory`.

  Run: `grep -c "pub mod" crates/synthia-core/src/lib.rs`
  Expected: 11 (was 12 before, now missing `tool`).

**Verification:** all commands exit 0, all files deleted, no stale `synthia_core::tool` references.

### Task 22: Tag the migration commit

**Files:** none

- [ ] **Step 1: Tag the migration**

  ```bash
  git tag -a tool-migration-2026-08-02 -m "Migrate synthia-core::tool to synthia-tool"
  ```

  This is a soft tag for easy rollback reference. The user can revert via `git reset --hard tool-migration-2026-08-02~1` if needed.

- [ ] **Step 2: Final summary**

  Write a final summary to the user with:
  - Total commits made (count from `git log tool-migration-2026-08-02~1..tool-migration-2026-08-02 --oneline`).
  - Lines moved (count from `git diff --stat tool-migration-2026-08-02~1..tool-migration-2026-08-02`).
  - Test pass count (count from `cargo test -p synthia-tool 2>&1 | grep "test result"`).
  - Cycle check: pass/fail.
  - `synthia_core::tool` audit grep: 0 matches.

**Verification:** tag exists. `git show tool-migration-2026-08-02` shows the migration summary.

---

## Open Items Resolved by This Plan

| Sub-decision | Where resolved | What was decided |
|---|---|---|
| #1 — `ToolError` location | Task 17 step 1 | Recorded in commit message |
| #2 — `descriptor.rs` location | Task 15 step 3 + Task 17 step 2 | Single file vs. split, per LOC check |
| #3 — `ToolRegistry` file split | Task 8 step 3 + Task 15 | Single `registry.rs` or split into `dispatch.rs`, per LOC check |
| #4 — `ToolEvent` location | Task 17 step 1 | Kept or deleted, per usage count |
| #5 — Adapter descriptor cache | Task 16 step 1 | Eager vs. lazy, per 9 builtin tool cost analysis |

## Rollback

If any task in Phase 5 (Task 20, 21, 22) fails:
```bash
git reset --hard tool-migration-2026-08-02~1
```

If an earlier task in Phase 1-4 fails:
```bash
# Find the last green commit:
git log --oneline | head -30
# Revert the failing commit:
git revert <failing-commit-sha>
```

If a partial state must be preserved for investigation, do not reset. Create a branch first:
```bash
git branch debug-tool-migration
git reset --hard <last-green-commit>
```

## Out of Scope (deferred)

Per spec Section 12:
- Splitting the tool subsystem into a separate `synthia-extension` crate.
- Removing the `synthia_tool::subagent::SubagentFactory` trait in favor of a typed `Subagent` enum.
- Unifying `ToolDefinition` / `ToolExecution` / `ToolLifecycle` sub-traits into a single `Tool` interface.
- Adding `no_std` support to any of the moved modules.
