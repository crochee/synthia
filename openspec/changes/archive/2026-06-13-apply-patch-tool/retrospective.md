# Retrospective: apply-patch-tool

> Written: 2026-06-13 (after merge to master)
> Base commit: `24c7b79`
> Final commit: `1509339` (merge)
> Branch: `feat/apply-patch-tool` (deleted post-merge)

---

## 0. Evidence

- **Commit range**: `24c7b79..1509339` (2 feature commits + 1 merge commit, all on `master`)
- **Diff size**: ~1,500 lines added across 5 impl files + 22 scenario fixture directories
- **Tasks done**: 6/6 plan task groups (V4A parser, ApplyPatchTool, registry integration, codex scenario porting, custom tests, verification)
- **Subagent dispatches**: 0 (single-agent, surgical scope, codex scenarios provided the test contract)
- **New external dependencies**: none
- **Bugs encountered post-merge**: 0
- **Codex 22-scenario port**: 22/22 passing, 0 regressions
- **OpenSpec validate state at archive**: pending (this commit)

---

## 1. Wins

- [evidence: codex `tests/suite/scenarios.rs` is a generic fixture-based runner] **Adopted codex's portable scenario test framework verbatim**: copied 22 fixture dirs from `codex-rs/apply-patch/tests/fixtures/scenarios/`, implemented a ~120-line parameterized test runner. Cross-language portability validated — synthia tests now use the **de facto V4A compatibility test set**.
- [evidence: `Hunk { lines: Vec<HunkLine> }` (Context/Insertion/Deletion variants) preserves source order] **Refactored hunk representation to fix scenario 021 (interleaved context/deletion)**: original `Vec<String>` separation couldn't reconstruct `old_text` for ` line1 / -line2 /  line3` patterns. Single-line refactor unblocked 1 scenario and future-proofed against similar edge cases.
- [evidence: `apply_hunks` handles `old_text.is_empty()` as pure addition] **Pure addition hunks (scenario 016) supported without parser changes**: empty-old-text hunks now append `new_text` to the file. Matches codex behavior; no extra V4A grammar needed.
- [evidence: `apply_one` calls `create_dir_all(parent)` for `Add` operations] **Parent directory auto-creation (scenario 002)**: a 2-line addition enabled nested-file Add operations (`nested/new.txt`). Codex has this implicitly via its shell-out path; synthia's pure-Rust impl needed it explicit.
- [evidence: 4 experts approved after D2'/D4'/D2.5/scenario expansion] **Codex/opencode adversarial review yielded 33% LoC reduction**: original D2 (snapshot + dry-run + commit + rollback) → D2' (linear 5-step pipeline); D4 (all-or-nothing rollback) → D4' (report applied + failed). Skeptic's 25 concerns resolved. Architect's "stay in synthia-tool crate" matched opencode's single-file scale.
- [evidence: `ApplyPatchTool { enable_move: bool }` with `#[derive(Default)]` → `enable_move = false`] **Move default-disabled via 1-line type design**: matches opencode's "moves not supported yet" gate-at-runtime pattern. Test runner flips the flag explicitly to cover scenarios 004/010; production stays safe.
- [evidence: 0 new warnings in clippy, 0 new clippy lint requirements] **Clean clippy pass**: all 21 workspace warnings pre-existed and were untouched by this change.
- [evidence: `Path Safety and Permission Reuse` requirement reuses `check_path_safety` + `write` policy] **No new Guardian policy surface**: apply_patch inherits the existing write policy instead of introducing a new one. Avoids policy proliferation.

---

## 2. Misses

- 📌 [evidence: codex apply-patch has a 944-line `streaming_parser.rs`] **Non-streaming parser is a known limitation**: opencode also uses non-streaming, but very large patches (10K+ lines) would benefit from streaming. Out of scope for v1 — codex/opencode both use non-streaming for the common case.
- 📌 [evidence: 4 spec requirements → 12 scenarios, but no scenario for "all operations apply across patch types simultaneously" beyond scenarios 002 and 015] **No explicit "mixed Update+Add+Delete+Move in one patch" scenario**: scenarios 002 and 015 cover this implicitly, but a dedicated scenario would be more pedagogical. Future improvement.
- 📌 [evidence: `ApplyPatchTool::description` mentions "Moves are not supported yet" but lacks a snippet example] **Description could be more example-driven**: opencode's `apply-patch.ts` includes a 5-line patch snippet in the description; synthia's is prose-only. LLM behavior is still correct (V4A protocol examples are in training data), but a snippet would reduce first-call trial-and-error.

---

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| 1.1 (`PatchOp` / `Hunk` types) | `Hunk.lines: Vec<HunkLine>` (Context/Insertion/Deletion) instead of separate `context`/`insertions`/`deletions` vectors | Scenario 021 (interleaved context/deletion) requires source-order preservation. The 3-vector design lost ordering |
| 2.4 (Parse → Resolve → Safety → Permission → Sequential apply) | No snapshot / no dry-run / no rollback state machine | D2' simplification per Skeptic + Simplification reviews; codex scenario 015 + opencode description both reject atomic rollback |
| 2.8 (Move rejection) | Move accepted at parse-time, rejected at runtime via `enable_move: bool` | D2.5 decision — opencode pattern. Test runner flips flag for scenarios 004/010 |
| 4 (5 custom tests) | Expanded to 22 codex scenarios + 3 custom (path traversal, move-disabled default, registry verify) | D6' — reuse codex's portable test suite instead of reinventing |
| 4.6 (`test_rejects_missing_context`) | Replaced with `codex_scenario_006_rejects_missing_context` | Same coverage, codex-grade naming |
| 4.37 (`test_add_overwrites_existing_blocked`) | Renamed to `test_add_overwrites_existing` and inverted to assert ALLOWED | Code discovery: codex scenario 011 allows `*** Add File:` to overwrite; the original "block" assertion contradicted V4A spec |

---

## 4. Skill / workflow compliance

| Skill | Used |
|-------|------|
| `superpowers:brainstorming` | ✓ (brainstorm.md captures 8 design Q&As pre-D2' revision) |
| `superpowers:writing-plans` | ✓ (plan.md with 6 task groups, each with TDD-style micro-steps) |
| `superpowers:multi-expert-adversarial-review` | ✓ (4 experts, 42 original issues raised, 42 resolved) |
| `superpowers:verification-before-completion` | ✓ (verify.md produced, all gates checked) |
| `superpowers:test-driven-development` | ✓ (22 scenario tests written first via fixture copy, parser/impl written to satisfy them) |
| TDD per module | ✓ (5 inline unit tests in `apply_patch.rs` + 24 codex_scenarios.rs tests, all written before/during impl) |
| **Codex/opencode cross-reference** | ✓ (4479 lines of codex apply-patch crate + 177 lines of opencode apply-patch.ts reviewed during design) |

---

## 5. Open follow-ups

- **Fuzzy hunk matching**: `apply_hunks::find_hunk` currently only supports basic context substring matching. Codex has a 163-line `seek_sequence.rs` implementing a "find the closest matching context block" algorithm. Useful for LLMs that emit slightly imprecise context. Not needed for v1 (strict mode catches errors fast).
- **V4A grammar coverage**: parser handles the V4A spec, but doesn't yet support the `*** End of File` marker on the *first* line of a hunk (only on subsequent lines). Tested via scenario 022; no production use case.
- **Stream-parsing for large patches**: codex's `streaming_parser.rs` (944 lines) handles 10K+ line patches without loading the whole thing. Not needed for typical LLM output (a few hundred lines max). YAGNI.
- **Move enablement** (D2.5 future): requires (a) D2 atomic rollback design, (b) cross-filesystem `mv` compatibility tests, (c) Guardian policy extension. Documented in `tasks.md` §后续跟踪.
- **Adjacent opportunity** (out of scope): `turn-id-mvp` should be unfrozen — codex's #28002 + #27996 (2026-06-13) is exactly the "concrete use case" we said didn't exist 3 months ago. Tracked separately; will be a follow-up OpenSpec change.

---

## 6. Decision: archive this change

The change is well-scoped, the codex 22-scenario test contract makes it V4A-spec-compliant by construction, the 2 commits are clean and rollback-able, and the OpenSpec change artifacts (`brainstorm`, `design`, `proposal`, `plan`, `tasks`, `specs`, `review`, this `retrospective`, `verify`) trace the full design chain.

The D2' simplification (no atomic rollback) and D2.5 Move default-disabled decisions are the key call-outs that make this change small (~1500 LoC) and aligned with codex/opencode. The 4-expert adversarial review is a key artifact that should be referenced when future contributors question the "no atomic rollback" decision.

**Action**: archive to `openspec/changes/archive/2026-06-13-apply-patch-tool/` and sync delta specs to `openspec/specs/apply-patch-tool/spec.md` (cumulative format).
