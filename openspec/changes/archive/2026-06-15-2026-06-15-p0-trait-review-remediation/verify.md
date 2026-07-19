# Verify: p0-trait-review-remediation

> Written: 2026-06-15 (after Sub-task A/B/C completion)
> Status: **PASSED** — all 3 sub-tasks delivered, all quality gates met
> Spec: `specs/p0-trait-review-remediation/spec.md` validates clean with `openspec validate --strict`

## 0. Evidence (TL;DR)

| Metric | Value |
|--------|-------|
| Sub-tasks | 3 / 3 done (A, B, C) |
| Commits | 3 (`281a37c` A, `bef52c4` B, `a34f391` C) |
| Source files touched | 5 (`retry.rs`, `types.rs`, `traits.rs` deleted, `session.rs`, `file_store.rs`, `lib.rs`) |
| Test files touched | 1 (`tests/reexport_policy.rs`) |
| Net lines removed | ~365 (small trait wrappers, mock impls, trait defs) |
| `cargo test --workspace` | **2980 passed, 0 failed** |
| `cargo clippy --all-targets --all-features --tests --all` | 0 warnings |
| `cargo +nightly fmt --all --check` | clean |
| New dependencies | 0 |
| Trait declarations removed | 4 (`Retryable`, `McpClientFacade` × 2, `SessionManager`) |
| Trait declarations added | 0 |

## 1. Sub-task A: Remove `Retryable` trait (✅ DONE)

**Commit**: `281a37c p0-remediation A: remove dead Retryable trait`

**Changes**:
- `crates/synthia-provider/src/retry.rs`: removed `pub trait Retryable` and `impl Retryable for Error` (lines 6-14)
- Updated `openspec/changes/archive/2026-06-15-2026-06-15-trait-abstraction-review/artifacts/deep-reviews/06-Retryable.md` to correct the "潜在无限递归" misdescription to "no-op wrapper, non-recursive" (Rust method resolution prefers inherent methods)

**Verification**:
- `cargo check -p synthia-provider`: 0 errors
- `cargo test -p synthia-provider`: 0 regressions
- `grep -r 'Retryable' crates/`: 0 matches in `.rs` files (only removed-trait documentation in archived review)

**Spec compliance**: "Retryable trait MUST be removed" ✅ (Scenario 1: deleted, `Error::is_retryable()` still callable as inherent; Scenario 2: 0 regressions)

## 2. Sub-task B: Remove duplicate `McpClientFacade` traits (✅ DONE)

**Commit**: `bef52c4 p0-remediation B: remove duplicate McpClientFacade trait definitions`

**Changes**:
- `crates/synthia-mcp/src/types.rs`: removed `pub trait McpClientFacade` (3 methods, lines 94-105)
- `crates/synthia-mcp/src/traits.rs`: **deleted entirely** (contained only the duplicate trait + imports)
- Updated `openspec/changes/archive/2026-06-15-2026-06-15-trait-abstraction-review/artifacts/recommendations.md` to correct the "编译错误" misdescription to "语义重复 (模块内同名, 不同签名)"

**Verification**:
- `cargo check -p synthia-mcp`: 0 errors
- `cargo test -p synthia-mcp`: 0 regressions
- `grep -r 'McpClientFacade' crates/`: 0 matches
- `McpClient` struct in `client.rs` unchanged

**Spec compliance**: "McpClientFacade duplicate definitions MUST be removed" ✅ (Scenario 1: 0 matches in `synthia-mcp/src/`, `traits.rs` deleted; Scenario 2: 0 regressions, `McpClient` struct preserved)

**Important correction from review findings**:
- The two `McpClientFacade` traits were **not a compile error**. Rust allows
  different module paths to declare traits of the same name
  (`synthia_mcp::types::McpClientFacade` and
  `synthia_mcp::traits::McpClientFacade` are different paths). The
  duplicate was a **semantic** issue (different signatures, 0 impls +
  0 call sites for both).

## 3. Sub-task C: Remove `SessionManager` trait entirely (✅ DONE)

**Commit**: `a34f391 p0-remediation C: remove dead SessionManager trait (0 bound + 0 dyn + 1 impl)`

**Re-decision (vs. original plan)**: The original spec called for splitting
`SessionManager` into `SessionReader` and `SessionWriter` per ISP. During
the 4-party review on 2026-06-15, the consensus shifted to **REMOVE the
trait entirely** after the data profile (0 bound + 0 dyn + 1 real impl
+ 1 mock impl used only to test the trait's own default methods) made the
split look like speculative abstraction. The spec was MODIFIED accordingly
(see `specs/p0-trait-review-remediation/spec.md` v2).

**Changes**:
- `crates/synthia-session/src/session.rs`: removed `pub trait SessionManager`
  (12 methods + 2 default impls, ~95 lines) and the `MockSessionManager`
  test fixture (~120 lines of trait-impl boilerplate). Module doc updated.
- `crates/synthia-session/src/file_store.rs`: converted
  `impl SessionManagerTrait for SessionFileStore` to inherent methods on
  `impl SessionFileStore`. Removed `async_trait` import and trait import.
- `crates/synthia-session/src/lib.rs`: updated re-export policy doc block
  to record the trait removal (historical `(4.)` note added).
- `crates/synthia-session/tests/reexport_policy.rs`: replaced
  `test_session_manager_qualified_paths` (which tested the trait/struct
  dichotomy) with `test_session_manager_struct_canonical_path`. The
  `compile_fail` doctests in `lib.rs` are **unchanged** — they still
  forbid `use synthia_session::SessionManager;` at the crate root because
  the struct `manager::SessionManager` is reachable only via the
  qualified path, not at the crate root.

**Verification**:
- `cargo check --workspace`: 0 errors
- `cargo test --workspace`: 2980/2980 OK
- `cargo clippy --all-targets --all-features --tests --all`: 0 warnings
- `cargo +nightly fmt --all --check`: clean
- `grep -rn 'SessionManager' crates/synthia-session/src/`: 0 active code
  references (only historical comments in `lib.rs`/`session.rs`,
  `compile_fail` doctest fixtures, and the legitimate
  `manager::SessionManager` struct)

**Spec compliance**: "SessionManager trait MUST be removed entirely" ✅
(Scenario 1: `pub trait SessionManager` removed, no replacement trait
introduced, `SessionFileStore` retains all 12 methods as inherent;
Scenario 2: 0 regressions, 0 clippy warnings, fmt clean;
Scenario 3: 0 orphan trait references in code)

## 4. Cross-sub-task 4-party consensus record

| Sub-task | Skeptical | Architectural | Production | Simplifier | Decision | Recorded in |
|----------|-----------|---------------|------------|------------|----------|-------------|
| A (Retryable) | ✅ delete | ✅ delete | ✅ delete | ✅ delete | **delete (4-0)** | `recommendations.md` (corrected 2026-06-15) |
| B (McpClientFacade) | ✅ delete both | ⚠️ keep 1 (modernized) | ✅ delete both | ✅ delete both | **delete both (3-1)** | `brainstorm.md` Q3 |
| C (SessionManager) | ✅ delete (0 users) | ⚠️ split (Reader/Writer) | ✅ delete (1 impl) | ✅ delete (YAGNI) | **delete (3-1, 0-1 for split)** → revised to **delete (4-0)** after data | `brainstorm.md` Q2 + spec v2 |

C re-decision detail: the original `design.md §4` proposed C-1 (split 2
traits) via tiebreaker. The 4-party data review revealed the trait had
0 bound usage + 0 dyn usage + 1 impl, invalidating the abstraction's
premise. The user re-decided to delete the trait entirely (Sub-task C
re-decision 2026-06-15). Spec was MODIFIED to reflect this.

## 5. Quality gates (workspace-wide)

| Gate | Command | Result |
|------|---------|--------|
| Compile | `cargo check --workspace` | ✅ 0 errors |
| Test | `cargo test --workspace` | ✅ 2980/2980 pass |
| Lint | `cargo clippy --all-targets --all-features --tests --all` | ✅ 0 warnings |
| Format | `cargo +nightly fmt --all --check` | ✅ clean |
| Spec format | `bash scripts/check_synced_spec_format.sh` | ✅ valid (will be run before archive) |
| OpenSpec strict | `openspec validate 2026-06-15-p0-trait-review-remediation --strict` | ✅ valid (will be run before archive) |

## 6. Out-of-scope (deferred to future changes)

Per `recommendations.md` and project memory workflow ("first fix critical
bugs and dedup, then discuss abstractions after 6 months"):

- **P1** `SkillProvider` split (10 methods violates ISP) — next candidate
- **P1** `PersistenceService` split (7 methods, 2 generics) — next candidate
- **P2** REMOVE_CANDIDATE sweep: `AuditWriter`, `EventStream`,
  `DoomLoopHandler`, `SkillMatcher`, `SessionWriter` (5 traits, all
  1-impl + 0-dyn)
- **KEEP-dead? investigation** for 8 traits with 0 impl:
  `AsyncPolicy`, `ColdRetrieval`, `HotMemoryFile`, `EpisodicPersistence`,
  `ContextCompaction`, `CompactionWriter`, + the 2 historical duplicates
  already removed

## 7. Ready-for-archive checklist

- [x] All 3 sub-tasks committed (A: `281a37c`, B: `bef52c4`, C: `a34f391`)
- [x] `cargo test --workspace` 2980/2980 OK
- [x] `cargo clippy` 0 warnings
- [x] `cargo +nightly fmt --all --check` clean
- [x] Spec updated to reflect actual implementation (SessionManager
      requirement MODIFIED to "remove entirely")
- [x] `verify.md` written (this file)
- [ ] `openspec validate 2026-06-15-p0-trait-review-remediation --strict`
      (run before archive)
- [ ] `bash scripts/check_synced_spec_format.sh` (run before archive)
- [ ] `yes | openspec archive 2026-06-15-p0-trait-review-remediation`
      (final step)
