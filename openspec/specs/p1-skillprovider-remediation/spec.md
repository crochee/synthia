# p1-skillprovider-remediation Specification

## Purpose
TBD - created by archiving change 2026-06-15-p1-skillprovider-remediation. Update Purpose after archive.
## Requirements
### Requirement: SkillProvider trait MUST be removed entirely

The system MUST remove the `pub trait SkillProvider` (10 methods,
defined at `crates/synthia-skill/src/traits.rs:9`) entirely. The trait
has zero trait-bound usage, zero dyn dispatch usage, zero
`Arc<SkillProvider>` / `Box<SkillProvider>` wrapping, and exactly one
real implementation (`SkillRegistry` in `registry.rs:554`).

**Decision (2026-06-15 4-party review)**: The original
`trait-abstraction-review` recommendation was to SPLIT the trait into
3 focused traits (`SkillReader` / `SkillWriter` / `SkillVectorIndex`)
per ISP. A re-audit on 2026-06-15 (during this change's setup)
discovered the trait has 0 actual users beyond its own `impl` block
(no trait bounds, no dyn dispatch, no Arc/Box wrapping). With this new
data, the 4-party consensus shifted to **REMOVE the trait entirely**
rather than introduce 3 speculative traits with the same 0-user
profile. The `SkillRegistry` struct remains unchanged; only the trait
abstraction is removed.

This decision is **100% isomorphic** to the P0 `SessionManager` removal
completed on 2026-06-15 in change
`2026-06-15-p0-trait-review-remediation` Sub-task C. Both traits
share the same profile (0 bound + 0 dyn + 0 Arc/Box + 1 impl) and
receive the same 4-0 REMOVE verdict.

The removal MUST:
- Delete `crates/synthia-skill/src/traits.rs` entirely (contains only
  the trait + imports; no other code uses it)
- Delete the `impl crate::traits::SkillProvider for SkillRegistry` block
  in `registry.rs`, moving any unique method bodies (if not already
  present as inherent methods) to the existing `impl SkillRegistry`
  block
- Remove `pub use traits::SkillProvider;` from
  `crates/synthia-skill/src/lib.rs:24`
- Remove the 4 dead `use` imports referencing the trait:
  - `crates/synthia-skill/src/installer.rs:18`
  - `crates/synthia-skill/src/watcher.rs:19`
  - `crates/synthia-skill/src/implicit_tools.rs:247` (test fixture)
  - `crates/synthia-command/src/builtin/skill.rs:7`
- Preserve `SkillRegistry`'s public method signatures byte-identical
  to the previous `impl SkillProvider for SkillRegistry` (now as
  inherent methods)

#### Scenario: SkillProvider trait is removed from the source tree

- WHEN the source tree `crates/synthia-skill/src/` is inspected
- THEN `pub trait SkillProvider` MUST NOT exist
- AND `crates/synthia-skill/src/traits.rs` MUST NOT exist (or
  contain no `pub trait` definition)
- AND `crates/synthia-skill/src/registry.rs` MUST NOT contain
  `impl ... SkillProvider for SkillRegistry`
- AND `SkillRegistry` MUST continue to expose all 10 methods
  (`list_skills`, `get_skill`, `match_skills`, `register_from_path`,
  `unregister`, `disable`, `enable`, `reload`,
  `match_skills_vector`, `rebuild_vector_index`) as inherent methods
  with identical signatures

#### Scenario: No regression in synthia-skill and dependent crates

- WHEN `cargo test --workspace` is executed after the removal
- THEN all tests in `synthia-skill` and all downstream consumers
  (notably `synthia-command` and `synthia-agent`) MUST pass with
  0 failures
- AND `cargo clippy --all-targets --all-features --tests --all` MUST
  report 0 warnings
- AND `cargo +nightly fmt --all` MUST produce no diff

#### Scenario: Crate-root re-export and dead imports are cleaned up

- WHEN the workspace is searched for `SkillProvider` references in
  `.rs` files after the removal
- THEN the result MUST be zero matches
- AND `synthia_skill::SkillProvider` (crate-root path) MUST no longer
  resolve

#### Scenario: Public API breakage is intentional and documented

- WHEN downstream consumers (outside the workspace) upgrade
- THEN the breaking change MUST be documented in the changelog
  (commit message + `verify.md`)
- AND the alternative (using `SkillRegistry` directly) MUST be
  obvious from the 10 inherent method names being unchanged

