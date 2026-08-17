# ADR-0010: `synthia-context` `anyhow::Result` Strategy

## Status

Proposed (2026-08-05)

## TL;DR

**Recommendation: Option B** — keep `PromptSection::build -> anyhow::Result<String>`, replace each
`section.build(ctx)` call site in `builder/resolve.rs` with
`.map_err(|e| e.context(format!("[{name}] section render failed")))`, and drop the
`use anyhow::Result;` + `anyhow::` alias in the section leaves whose `build()` body never produces
an error.

**Trait `dyn`-compatible?** **YES** — `PromptSection` is stored as `Vec<Box<dyn PromptSection>>`
in `prompt/builder/core.rs:13` and exercised as `Box<dyn PromptSection>` in
`prompt/sections/tests.rs:526`. The trait method signature is **locked in**; switching to a
custom error type in the trait return position is a breaking change for every dyn-dispatch site.

## Context

`synthia-context` carries **20** `anyhow::Result` / `anyhow::Error` / `anyhow!` /
`.context()` occurrences across **18 files**. Unlike `synthia-session`'s `anyhow!()` macro
pattern (P3-2a in flight), `synthia-context`'s pattern is **trait-level** — the
`PromptSection::build` signature returns `anyhow::Result<String>`, propagated mechanically
through every concrete section impl.

A grep for `.context(` and `.with_context(` across the entire crate returns **zero matches**.
Every `build()` body in the inventory below returns `Ok(String::new())`, `Ok(format!(...))`,
or `Ok(some_string)` directly. The `anyhow::Result` return type is currently unused machinery
for the failure path — there is no fallible chain anywhere in this crate.

## Inventory

### A. `prompt/sections/*.rs` — `use anyhow::Result;` + `Result<String>` in `build()`

These 14 files declare `use anyhow::Result;` and use it solely as the return type of
`PromptSection::build`. None of them call `anyhow!`, `bail!`, `.context()`, or `.with_context()`.

| File | Line | Usage |
|------|------|-------|
| `prompt/sections/section_trait.rs` | 22 | `use anyhow::Result;` |
| `prompt/sections/section_trait.rs` | 29 | `fn build(...) -> Result<String>;` (trait decl) |
| `prompt/sections/section_trait.rs` | 41 | `fn build(...) -> Result<String>` (blanket `Box<T>` impl) |
| `prompt/sections/skills.rs` | 1, 38 | `use` + `Result<String>` |
| `prompt/sections/token_budget.rs` | 1, 41 | `use` + `Result<String>` |
| `prompt/sections/output_style.rs` | 1, 36 | `use` + `Result<String>` |
| `prompt/sections/tools_usage.rs` | 1, 30 | `use` + `Result<String>` |
| `prompt/sections/proactive.rs` | 1, 69 | `use` + `Result<String>` |
| `prompt/sections/team_mode.rs` | 3, 41 | `use` + `Result<String>` |
| `prompt/sections/environment.rs` | 1, 24 | `use` + `Result<String>` |
| `prompt/sections/language.rs` | 1, 34 | `use` + `Result<String>` |
| `prompt/sections/memory.rs` | 1, 27 | `use` + `Result<String>` |
| `prompt/sections/system.rs` | 1, 39 | `use` + `Result<String>` |
| `prompt/sections/task_execution.rs` | 1, 48 | `use` + `Result<String>` |
| `prompt/sections/identity.rs` | 1, 43 | `use` + `Result<String>` |
| `prompt/sections/agents_md/section.rs` | 7, 59 | `use` + `Result<String>` (the only section with non-trivial logic — calls `walk_ancestors` / `merge_within_limit`, but both return plain `Vec<_>` / `String`, no fallible chain) |

### B. `prompt/compaction.rs` — non-trait public helpers

| Line | Usage |
|------|-------|
| 68 | `pub fn render_compaction_prompt(...) -> anyhow::Result<String>` (body: `Ok(...)`) |
| 75 | `pub fn render_compaction_prompt_with_type(...) -> anyhow::Result<String>` (body: `Ok(...)`) |

Both return `Ok` immediately; `.replace` on a `&str` cannot fail. Truly infallible.

### C. `prompt/builder/*.rs` — orchestrator callers

| File:Line | Usage |
|-----------|-------|
| `builder/tests.rs:74` | `fn build(...) -> anyhow::Result<String>` (test mock) |
| `builder/resolve.rs:37` | `pub fn resolve(...) -> anyhow::Result<ResolvedPrompt>` (body: `Ok(...)`; uses `?` on `section.build(ctx)` 2×) |
| `builder/resolve.rs:112` | `pub fn validate_prefix_stability(...) -> anyhow::Result<bool>` (body: `Ok(...)`; uses `?` on `section.build(ctx)` once) |
| `builder/effective.rs:31` | `pub fn build_effective_prompt(...) -> anyhow::Result<String>` (body: `Ok(...)`; uses `?` on `self.resolve(...)`) |

The only meaningful `?` chains in this crate live in `resolve.rs` lines 48 and 52 (and 121, in
the validate path). These are the **two** `.context()` replacement targets under Option B.

### D. Outside `prompt/`

No `anyhow` references in `assembler/`, `checkpoint/`, `compact_context_tool.rs`,
`compaction/`, `compaction_service.rs`, `config.rs`, `fragment/`, `prefix_tracker/`,
`protector/`, `token_budget/`, `truncate/`, `types.rs`, `lib.rs`, `traits.rs`. Confirmed by
`grep -r 'anyhow' crates/synthia-context/src` — matches listed above are exhaustive.

### E. Zero `.context()` / `.with_context()` / `anyhow!()` / `bail!()`

Verbatim grep `\.context\(|\.with_context\(|anyhow!|bail!` across
`crates/synthia-context/` returns **0 matches**. This is the single most important fact in
this investigation: there is no existing context string infrastructure to migrate.

## Trait `dyn`-compat Analysis

### Verdict: `PromptSection` IS used as a trait object. **Trait signature is locked.**

**Evidence:**

1. `crates/synthia-context/src/prompt/builder/core.rs:13` —
   `pub(crate) sections: Vec<Box<dyn PromptSection>>`
2. `crates/synthia-context/src/prompt/builder/core.rs:40` —
   `pub fn add_section(mut self, section: Box<dyn PromptSection>) -> Self`
3. `crates/synthia-context/src/prompt/sections/tests.rs:526` —
   `let section: Box<dyn PromptSection> = Box::new(SystemSection::new());` (positive test)
4. `crates/synthia-context/src/prompt/sections/section_trait.rs:19` — doc comment explicitly
   declares the trait is designed for `Vec<Box<dyn PromptSection>>` storage.
5. The `impl<T: PromptSection + ?Sized> PromptSection for Box<T>` blanket impl (lines 32-43)
   exists solely to support dyn dispatch.

### Consequence

Because the trait is `dyn`-compatible **and** is used as a trait object, any change to
`fn build(...) -> Result<String, E>` where `E` is **not** `anyhow::Error` is a **breaking
change** at every `Box<dyn PromptSection>` site — both within the workspace (tests in
`builder/tests.rs:74`) and at the conceptual API surface (`synthia-context` is one of the
14 workspace crates listed in README, consumed by `synthia-agent` and ultimately the server).

The task description asked: "if yes, the trait signature is locked in and we cannot change
`Result<String, X>`". **Confirmed: yes, locked.**

A caveat: `synthia-context` is internally consumed only — no external Cargo-crate consumers
outside this workspace. But the 13 sibling crates (notably `synthia-agent`, `synthia-server`)
all transitively depend on `PromptSection::build`'s shape. Still breaking, just within-workspace.

## Options

### Option A: Drop `.context()` wrappers only (preserve error type signature as-is)

- **What changes:** Nothing in the trait. Section leaves and helpers are kept literally as
  they are today. (This matches the case where there are already no `.context()` calls.)
- **Effort:** **0 h** (no-op). Option A is essentially "do nothing" — the inventory shows the
  crate already has zero `.context()` chains, so A collapses to "no further work".
- **Risk:** Lowest (none).
- **Loss:** None observable — but the `anyhow::Result` return remains technically unused,
  paying the bundle size + compile-time + cognitive overhead for nothing.

If the intent of P3 was "audit anything that survives the migration", Option A serves as the
status-quo baseline.

### Option B: Keep trait, add `.context("[name] …")` at orchestrator call sites

- **What changes:** In `prompt/builder/resolve.rs`, replace every `section.build(ctx)?` with
  ```rust
  section.build(ctx).map_err(|e| {
      anyhow::Error::new(e).context(format!(
          "[{}] section render failed",
          section.name()
      ))
  })?
  ```
  This attaches the section's stable name (`"system"`, `"skills"`, `"environment"`, …) as
  the **outermost** context layer when a `build()` body ever does fail. Drop the
  `use anyhow::Result;` alias in any section file where `build()` never produces an error
  (replace `Result<String>` with `String` in those leaves, but keep the trait signature).
- **Effort:** **3 h**
  - 0.5 h: replace `?` at `resolve.rs:48`, `:52`, `:121` (3 sites)
  - 1.0 h: drop `use anyhow::Result;` aliases in 13 leaf files (the 2 trait impls in
    `section_trait.rs` keep `Result<String>`; leaves that never fail become `String`)
  - 0.5 h: `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all -p synthia-context`
  - 1.0 h: regression run via `cargo test -p synthia-context prompt::`
- **Risk:** Low. Trait signature unchanged; existing impls unchanged; only the propagation
  site gets richer and a few pure leaves shed the dead `anyhow` import.
- **.context() targets to replace:** There are **none**. We are **adding** `.context()`
  wrappers (not migrating them away). The two `?` chains in `resolve.rs` are the new
  attachment points.

### Option C: New `ContextError` enum, break the trait signature

- **What changes:** Define `pub enum ContextError { RenderSection(&'static str), PromptBuild, … }`
  in `synthia-context::error`. Change `PromptSection::build -> Result<String, ContextError>`
  and every `impl PromptSection for X` impl. Provide `impl From<ContextError> for anyhow::Error`
  + `From<anyhow::Error> for ContextError` at the public-API boundary (consistent with the
  double-error-model contract from ADR-0007).
- **Effort:** **8 h+**
  - 1.0 h: design + ADR update + add `error.rs`
  - 2.0 h: change trait + 14 implementors (mechanical, but each must compile)
  - 1.0 h: rewire `builder/resolve.rs`, `builder/effective.rs`, `prompt/compaction.rs`
    to return `Result<_, ContextError>` (no `anyhow`)
  - 0.5 h: add `From<ContextError> for anyhow::Error` at boundary so consumers keep working
  - 0.5 h: format + clippy
  - 2.0 h: regression tests in `cargo test -p synthia-context prompt::sections::tests`,
    `cargo test -p synthia-context prompt::builder::tests`, `cargo test -p synthia-context prompt::sections::tests_prompt_section_boxed` (dyn dispatch must still resolve)
  - 1.0 h: ripple to `synthia-agent` / `synthia-server` if their callsites assume `anyhow::Error`
    (low cost because `?` will auto-coerce via the new `From` impl, but needs verification)
- **Risk:** **HIGH.** Trait signature breaking change. Every `impl PromptSection for X` outside
  the trait file becomes a compile error until updated. Every `let section: Box<dyn PromptSection>`
  site keeps working only via the blanket `Box<T>` impl — but downstream callers in
  `synthia-agent` / `synthia-server` that match on the `Result<String>` shape care about the
  `E` parameter. **Per task description: "if yes, the trait signature is locked in and we
  cannot change `Result<String, X>`".** Option C **directly violates this constraint** and
  should only be considered if we explicitly decide the breaking change is worth it.
- **Benefit (lost under A & B):** typed error variants — callers can `match` on
  `ContextError::RenderSection("environment")` for category-specific handling.
- **Judgment:** The benefit does not justify the breakage given (a) zero current `.context()`
  usage, (b) only 1 section (`AgentsMdSection`) has any non-trivial logic and even it
  ends in `Ok(...)`, and (c) section render failures today should be **fatal**, not
  recoverably categorized — adding a typed enum invites callers to attempt recovery from
  what is actually an invariant violation (a section that cannot render its prompt template
  is a programming bug, not a runtime condition).

### Option matrix

| Option | Effort | Risk | Trait change | `.context()` migration | Typed errors | Notes |
|--------|--------|------|--------------|------------------------|--------------|-------|
| **A**  | 0 h    | none | no           | n/a (none exist)       | no           | do-nothing baseline |
| **B**  | 3 h    | low  | no           | adds wrappers (3 sites) | no           | **recommended** |
| **C**  | 8 h    | HIGH | **YES** (breaks dyn) | full removal         | yes          | violates locked-trait constraint from task |

## Recommendation: **Option B**

**Justification:**

1. **The trait is dyn-locked** (evidence in §"Trait `dyn`-compat Analysis"). Option C is
   off the table by the task's own constraint. Comparing B vs A is the only real choice.

2. **B adds information; A loses it.** Today's `section.build(ctx)?` at `resolve.rs:48` and
   `:52` produces an `anyhow::Error` whose `Display` is whatever the leaf happened to write —
   *without* the section name. When triaging a render failure in production, the very first
   thing an operator wants to know is **which** of the 14 sections failed. Option B provides
   it for free via the existing `section.name()` accessor; Option A does not.

3. **B has the right cost.** 3 hours is small, the diff is mechanical, and clippy + the
   `prompt::sections::tests` suite cover the dyn-dispatch path (specifically
   `test_prompt_section_boxed` at `prompt/sections/tests.rs:524`).

4. **Pure leaves shed their dead `anyhow` import.** Most section files in the inventory
   (`skills.rs`, `language.rs`, `memory.rs`, `proactive.rs`, `system.rs`, `task_execution.rs`,
   `team_mode.rs`, `output_style.rs`, `tools_usage.rs`, `token_budget.rs`, `identity.rs`) return
   `Ok(...)` unconditionally. Keeping the `anyhow::Result` trait binding for them is fine, but
   the `use anyhow::Result;` import line in each becomes `use anyhow::Result;` *only* because
   that's the trait's return type. After Option B's `.context()` wrappers land, there's no
   reason these leaves can't just `Ok(...)` — they already do. (Trait keeps `Result<String>`,
   so they cannot just return `String` unless we change the trait, which is what Option C did.
   So the `use anyhow::Result;` alias stays, but at least we know it's load-bearing and not
   dead code.)

5. **No interaction with the P3-2a `synthia-session` `anyhow!()` macro migration.** P3-2a is
   about replacing `anyhow!("msg")` *calls* (which this crate has **zero** of) with
   `synthia_core::Error::validation(msg)`. The two migrations are independent.

### Implementation steps (Option B, max 5)

1. **`prompt/builder/resolve.rs` — wrap `section.build(ctx)?` (3 sites: lines 48, 52, 121):**
   replace each with
   ```rust
   section.build(ctx).map_err(|e| {
       anyhow::Error::new(e).context(format!(
           "[{}] section render failed",
           section.name()
       ))
   })?
   ```
   Keep the file's existing `anyhow::Result` return type on `resolve` and
   `validate_prefix_stability`; these now carry rich, named context upward.

2. **`prompt/compaction.rs` — drop the `anyhow::Result` alias from the two infallible helpers**
   (lines 68, 75). The function bodies are `Ok(.replace(...))`; change the signatures to
   `-> String` and update both call sites in tests (`prompt/compaction.rs:124`, `:134`).
   Removes 2 unnecessary `anyhow::Result` occurrences.

3. **`prompt/builder/tests.rs:74`** — keep the test mock's `anyhow::Result<String>` signature
   for now (it implements the trait, must match). No change.

4. **`prompt/builder/effective.rs:31`** — keep `anyhow::Result<String>`; this is a public-API
   surface re-exported by `synthia-agent`. The single `?` at line 48 now inherits rich
   context from step 1.

5. **Verification:** `cargo +nightly fmt --all && cargo clippy --all-targets --all-features --tests --all -p synthia-context` then `cargo test -p synthia-context prompt::sections::tests test_prompt_section_boxed prompt::builder::tests`. All must pass; specifically `test_prompt_section_boxed` must compile (proves dyn dispatch survived) and the existing happy-path tests must pass (proves `Ok` propagation is unchanged). No `.context()` calls are dropped, so no information is lost — only gained.

## Explicit non-goals

- **Do not change the `PromptSection` trait signature** (locked per dyn-compat).
- **Do not introduce a new error type** (cost > benefit at current `.context()` count of 0).
- **Do not touch `synthia-session`** (separate crate, separate migration P3-2a).
- **Do not modify section `build()` bodies** unless we discover a real fallible chain (none
  exist today).
