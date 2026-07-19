# Proposal: p1-skillprovider-remediation

## Why

`crates/synthia-skill/src/traits.rs:9` declares `pub trait SkillProvider`
(10 methods: CRUD + matching + vector index). The trait was flagged in
the 2026-06-15 `trait-abstraction-review` as a P1 candidate
(REVIEW: ISP violation) with a recommended split into 3 focused traits
(`SkillReader` / `SkillWriter` / `SkillVectorIndex`).

**Re-audit on 2026-06-15** (during this change's setup) revealed the
trait is in a **structurally identical profile to the P0
`SessionManager`** that was just removed:

| Signal | Value |
|--------|-------|
| impls | 1 (`SkillRegistry` in `registry.rs:554`) |
| methods | 10 |
| trait bound usage (`T: SkillProvider`) | 0 |
| dyn dispatch (`dyn SkillProvider`) | 0 |
| `Arc<SkillProvider>` / `Box<SkillProvider>` wrapping | 0 |
| `&SkillProvider` parameter passing | 0 |
| Real call sites of methods (via trait dispatch) | 0 |

The trait appears in 7 files, but **all 6 references are `use` imports +
1 `pub use` re-export**. The only "user" of the trait is the
`impl SkillProvider for SkillRegistry` block itself.

Following the principle of "first fix critical bugs and dedup, then
discuss architectural abstractions after a stabilization period",
this change **REMOVES** `SkillProvider` entirely rather than splitting
it into 3 speculative abstractions.

## What Changes

**Remove `pub trait SkillProvider` from `synthia-skill`**
- From: `pub trait SkillProvider: Send + Sync` in
  `crates/synthia-skill/src/traits.rs:9` (10 methods, 33 lines)
- To: trait deleted. `SkillRegistry` continues to expose all 10 methods
  as inherent methods (no API change for `SkillRegistry` consumers).
- Reason: 0 trait bound usage + 0 dyn + 0 Arc/Box = pure YAGNI. Splits
  into Reader/Writer/VectorIndex would create 3 more speculative
  abstractions with the same 0-user profile.
- Impact: zero runtime behavior change. Removes `SkillProvider` from
  `synthia_skill::*` re-exports.

**Remove related references in call sites**
- 4 files import `SkillProvider` (only as `use` import). Replace each
  import with no-op (or remove the import if it was the only use).
- 1 file (`lib.rs`) re-exports `SkillProvider` at the crate root. Remove
  the `pub use traits::SkillProvider;` line.

## 4-Party Consensus

4-0 consensus to **REMOVE** (full record in `brainstorm.md`):

- **Skeptic**: 0 bound + 0 dyn + 1 impl = same as `SessionManager` = REMOVE
- **Architect**: Split was historic position; new data (0 actual users)
  makes split 3× YAGNI. REMOVE with 6-month revisit condition.
- **Production**: Trait doesn't provide differentiation; value lives in
  `SkillRegistry` impl. REMOVE.
- **Simplifier**: 100% isomorphic to `SessionManager`. REMOVE.

## Out of Scope (deferred)

- Splitting into `SkillReader` / `SkillWriter` / `SkillVectorIndex`
  (revisit only if a 2nd impl or `dyn SkillReader` use case emerges,
  ≥6 months from now)
- Adding `SkillProvider` back as a "marker trait" for documentation
  purposes (Rust has `impl Trait` for that)
- Refactoring `SkillRegistry`'s 10 methods (they're independent and
  well-named)

## Alternatives Considered

| Alternative | Rejected because |
|-------------|------------------|
| Split into 3 focused traits (Reader/Writer/VectorIndex) | 0 actual users of the 1-trait form makes 3-trait form 3× YAGNI. Defers until 2nd impl or dyn use case. |
| Keep trait as structural label | 0 bound + 0 dyn = no abstraction value. `impl Trait` or doc comments suffice. |
| Move methods to `SkillRegistry` impl block (only) | This IS the chosen path. Methods stay where they are; trait is removed. |
