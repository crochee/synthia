# P2+P3 Error Architecture Refactor — ADR Compliance Review

> **Reviewer**: ADR Compliance Specialist
> **Date**: 2026-08-05
> **Scope**: synthia-core + 7 other crates (72 files, +2175 / −646 LOC)
> **ADRs under review**: ADR-0007, ADR-0008, ADR-0009, ADR-0010

---

## Counts

| Severity | Count |
|----------|-------|
| **Blocker** (violates ADR) | **1** |
| **High** (silent drift)    | **2** |
| **Medium** (ADR ambiguous) | **3** |
| **Low** (minor gap)        | **3** |
| **Info** (alignment note)  | **4** |
| **Total findings**         | **13** |

**Verdict per ADR**:
- ADR-0007 (Stability tiers): **Implemented** with **Info** note on undocumented new variants.
- ADR-0008 (snafu No-Go): **Implemented**.
- ADR-0009 (OpenDAL Partial Adoption): **Deviates** — only 1 of 3 builder methods delivered.
- ADR-0010 (synthia-context Option B): **Missing** — all 3 call sites in `resolve.rs` still use bare `?`; 2 infallible helpers in `compaction.rs` unchanged.

---

## Top 3 Most Impactful Findings

1. **H1 / ADR-0009** — Only `with_context` delivered; `with_operation` and `set_source` omitted. ADR explicitly listed **3 builder methods** ("Builder Trio" 8.2 / 9.0). 2 of 3 missing.
2. **B1 / ADR-0010** — None of the 3 `.context("[name] section render failed")` wrappers in `prompt/builder/resolve.rs` are applied. The **entire** Option B migration is missing.
3. **H2 / ADR-0010** — `prompt/compaction.rs:68,72` still returns `anyhow::Result<String>` for infallible `Ok(...)` bodies. Step 2 of ADR-0010 explicitly mandates `-> String`.

---

## Findings Table

| # | Severity | ADR # | Finding | Implementation Status | Recommendation |
|---|----------|-------|---------|----------------------|----------------|
| F1 | **Blocker** | **ADR-0010** | `crates/synthia-context/src/prompt/builder/resolve.rs:48`, `:52`, `:129` — all three `section.build(ctx)?` call sites use bare `?`. The ADR-0010-mandated `.map_err(\|e\| e.context(format!("[{}] section render failed", section.name())))?` wrapper is **not applied anywhere** in the crate (verified: 0 matches for `.context(` / `.with_context(` / `anyhow!` / `bail!` in `crates/synthia-context/`). ADR-0010 §"Implementation steps (Option B)" step 1 mandates exactly this 3-site wrap. | **Missing** | Apply the Option B Step-1 `.context()` wrappers at the 3 call sites; add a unit test verifying section-name appears in the rendered `anyhow::Error` chain. |
| F2 | **High** | **ADR-0010** | `crates/synthia-context/src/prompt/compaction.rs:68,72` — both `render_compaction_prompt` and `render_compaction_prompt_with_type` still return `anyhow::Result<String>` with infallible `Ok(...)` bodies. ADR-0010 Implementation Step 2 explicitly mandates: "drop the `anyhow::Result` alias from the two infallible helpers (lines 68, 75). The function bodies are `Ok(.replace(...))`; change the signatures to `-> String` and update both call sites in tests (`prompt/compaction.rs:124`, `:134`). Removes 2 unnecessary `anyhow::Result` occurrences." | **Missing** | Change both signatures to `-> String`; update 2 test call sites (`.unwrap()` → bare call). |
| F3 | **High** | **ADR-0009** | The ADR-0009 §8.2 "Builder 三件套" decision calls for **3 builder methods**: `with_context(key, value)`, `with_operation(op)`, and `set_source(err)`. Implementation in `crates/synthia-core/src/error/error.rs:656` delivers **only `with_context`**. Grep for `with_operation` and `set_source` returns **0 matches** in `crates/synthia-core`. ADR-0009 §9 "决策汇总" row 2 says verbatim: "Partial: builder 三件套 (`with_context` + `with_operation` + `set_source`)" — the trio is the deliverable. | **Deviates** | Add `Error::with_operation(self, op: &'static str) -> Self` (mutates a new `operation: &'static str` field) and `Error::set_source<E: std::error::Error + Send + Sync + 'static>(self, source: E) -> Self` (stashes the foreign error in a new `source: Option<Box<dyn Error>>` field, exposed via `std::error::Error::source`). Either (a) amend ADR-0009 to reflect the 1-of-3 delivered, or (b) deliver the remaining two. |
| F4 | **Medium** | **ADR-0007** | ADR-0007 §"演进規則" item 3 says: "加新 Error 变体: minor version + ADR 记录用途". Implementation added 3 new `Error` variants (`ContextOverflow` `error.rs:241`, `DoomLoop` `:248`, `PromptInjection` `:255`) and 6 new `ErrorCode` variants (`InvalidCursor`, `InvalidSortField`, `NotImplemented`, `ContextOverflow`, `DoomLoop`, `PromptInjection`). No new ADR was written recording the purpose of these additions. ADR-0009's earlier finding count quoted 36 ErrorCode variants; the current implementation has 39. ADR-0007 Baseline counted 33 Error variants; implementation has 36. The additions themselves comply with Tier-2 stability rules (additive-only, non-breaking), but the **record purpose** half of the rule is missing. | **Partial** | Either (a) write a short ADR-0011 "ErrorCode additions: InvalidCursor / InvalidSortField / NotImplemented / ContextOverflow / DoomLoop / PromptInjection" recording the purpose of each, or (b) update ADR-0007 to retroactively incorporate the additions. Either option satisfies the "ADR 记录用途" half of the rule. |
| F5 | **Medium** | **ADR-0007** | ADR-0007 §"`#[track_caller]` 使用规范" says "`#[track_caller]` 必须放在所有 **public** helper 构造函数上 (`Error::not_found()` / `validation()` / `internal()` 等)". Implementation places `#[track_caller]` on 30+ helper methods — including many beyond the original 5 high-freq ones (`unauthorized`, `forbidden`, `parse`, `tool_execution`, `provider`, `session`, `skill`, `memory`, `guardian_violation`, `stream`, `timeout`, `model_not_found`, `model_unavailable`, `config`, `config_watcher`, `router`, `task`, `executor`, `context_err`, `telemetry`, `multiagent`, `evaluation`, `rate_limited`, `request_failed`, `edit_conflict`, `context_overflow`, `doom_loop`, `prompt_injection`, `retry_exhausted`, `stream_error`). This **exceeds** the ADR's scope. The ADR wording uses "等" = "etc." which is consistent. Status: **Exceeds-but-aligned**. | **Implemented** (exceeds ADR scope) | Confirm with team that the broadened `#[track_caller]` coverage is intentional; if so, update ADR-0007 §"`#[track_caller]` 使用规范" to say "all public helper constructors" without the "等" caveat. |
| F6 | **Medium** | **ADR-0007** | ADR-0007 Tier-2 API contract enumerates the 5 high-freq variants as `NotFound / Validation / Internal / AlreadyExists / InvalidItem` and says each must "改 struct form, 加 `location` 字段". Implementation at `crates/synthia-core/src/error/error.rs:42-58` shows `NotFound`, `AlreadyExists`, `InvalidItem` use `item: String`; `error.rs:68-72,86-90` shows `Internal` and `Validation` use `message: String`. All 5 have `context: BTreeMap<String, String>` and `location: CallSite`. **However**, the ADR did not specify the `context: BTreeMap` field — implementation added it as a new dimension beyond the ADR's `location` requirement. The `context` field is undocumented in ADR-0007 and its presence changes the struct shape (Tier 2 API — adding a field to a public struct variant is breaking in semver-strict terms, but Rust's default struct-pattern matching is permissive so callers using `..` are unaffected). | **Implemented** (deviates from Tier-2 letter) | Add a paragraph to ADR-0007 §"Stability Tiers" Tier 3 explicitly listing `context: BTreeMap<String, String>` as Tier-3 internal, NOT part of the wire contract. |
| F7 | **Low** | **ADR-0007** | ADR-0007 §"P2.3 synthia-session 双错误模型修复" specifies `From<SessionError> for synthia_core::Error` and `From<anyhow::Error> for SessionError` bridges. Both are present in `crates/synthia-session/src/error.rs:100` (`From<anyhow::Error> for SessionError`, with `#[track_caller]`) and `:122` (`From<SessionError> for synthia_core::Error`). The `From` impls are correctly placed in the synthia-session crate (not synthia-core) — semantically correct, the ADR did not specify location. | **Implemented** | None. |
| F8 | **Low** | **ADR-0007** | ADR-0007 Tier-2 API contract lists `impl From<synthia_session::SessionError>` for `synthia_core::Error`. The grep in `crates/synthia-core/src/error/error.rs` returns only `From<std::io::Error>`, `From<reqwest::Error>`, `From<serde_json::Error>`, `From<serde_yaml::Error>` — no `From<synthia_session::SessionError>` in synthia-core. This is **not** a violation because the impl is correctly located in `crates/synthia-session/src/error.rs:122` (cross-crate `From` impls are idiomatic at the source crate). The ADR's listing was a contract, not a location spec. | **Implemented** | Consider clarifying ADR-0007 Tier-2 with footnote: "From impls may be located in either crate." |
| F9 | **Low** | **ADR-0007** | ADR-0007 §"`#[track_caller]` 使用规范" says "`#[track_caller]` 必须放在 `From<reqwest::Error>` 上". Implementation at `crates/synthia-core/src/error/error.rs:1103` has `#[track_caller]` on `From<reqwest::Error>`. Also added to `From<std::io::Error>` (`:270`) and `From<serde_json::Error>` / `From<serde_yaml::Error>` (`:1135`, `:1145`) — exceeds ADR scope, consistent with the broader pattern in F5. | **Implemented** (exceeds ADR scope) | Update ADR-0007 §"`#[track_caller]` 使用规范" to clarify that all 4 external `From` impls carry `#[track_caller]`, not just reqwest. |
| F10 | **Info** | **ADR-0007** | Tier 1 wire stability — `ErrorCode` variants, Display strings, `UserError` JSON shape all preserved. Verified: `crates/synthia-core/src/error/error_code.rs:27-66` has `#[non_exhaustive]` at `:25`; Display strings (`:71-110`) match ADR baseline. `crates/synthia-core/src/error/user_error.rs:19-25` shows `UserError { code, message, result: Option<serde_json::Value> }` with `#[serde(skip_serializing_if = "Option::is_none")]` at `:23`, exactly matching ADR-0007's `UserError { code, message, result }` JSON shape. | **Implemented** | None. |
| F11 | **Info** | **ADR-0007** | `#[non_exhaustive]` on `ErrorCode` (line 25 of `error_code.rs`) and `ServerError` (line 20 of `crates/synthia-server/src/error.rs`) both verified. Both enums are marked correctly. | **Implemented** | None. |
| F12 | **Info** | **ADR-0008** | No snafu dependency in workspace. Verified: `cargo tree` returns 0 matches for snafu; `Cargo.toml` (workspace root, lines 47-49) lists only `thiserror = "2"` and `anyhow = "1"`. The ADR-0008 No-Go decision is upheld — snafu was not adopted. | **Implemented** | None. |
| F13 | **Info** | **ADR-0008** | ADR-0008 §"Appendix A: 编译基准方法" references a `/tmp/opencode/snafu-compile-bench` micro-benchmark. The directory `docs/architecture/review/` was checked and **does not exist on disk** (only `docs/architecture/adr/` exists). The Appendix A benchmark is the *historical evidence* for the snafu No-Go decision; its preservation as a review document is the spirit of the Appendix's documentation, but the ADR itself is the durable record. ADR-0008 does not mandate preserving the benchmark artifact — it mandates the *decision*, which is upheld. | **Implemented** | Optionally create `docs/architecture/review/snafu-compile-bench-2026-08-04.md` capturing the median times cited in ADR-0008 Table 1 (thiserror 2.753s / snafu 4.074s / Δ +48%) so the evidence survives the `/tmp` cleanup. |

---

## Cross-ADR Notes

- **F1, F2** together constitute a full failure to implement ADR-0010. Both severities are at the "Blocker / High" level because the ADR explicitly mandated specific mechanical changes in 3 files (`resolve.rs` 3 sites, `compaction.rs` 2 sites). The ADR-0010 §"Implementation steps (Option B, max 5)" is a verbatim recipe.
- **F3** is a partial ADR-0009 implementation. ADR-0009 §8.2 sketch pseudocode (lines 491-510) literally shows all 3 builder methods in the same code block; only the first is implemented.
- **F4** is a procedural gap (the "record purpose" half of the rule) but not a semantic violation — the additions are Tier-2 safe (additive-only).
- **F5, F6, F9** are "exceeds ADR scope" findings: the implementation goes beyond what ADR-0007 specified, in ways that are consistent with the spirit of the ADR but not the letter. None are blockers; all warrant an ADR amendment for precision.
- **F10, F11, F12** confirm the highest-stability contracts (Tier 1 wire format, `non_exhaustive` annotations, snafu No-Go) are intact. These are the items most likely to break downstream consumers; all three are clean.

---

## Verification Commands Used

```bash
# ADR-0007 / 0008 snafu check
cargo tree 2>/dev/null | grep -i snafu               # 0 matches
grep -i snafu crates/synthia-core/Cargo.toml         # 0 matches

# ADR-0007 Tier 1 — ErrorCode + UserError shape
grep -E "non_exhaustive" crates/synthia-core/src/error/error_code.rs     # line 25
grep -E "non_exhaustive" crates/synthia-server/src/error.rs             # line 20
grep -E "skip_serializing_if" crates/synthia-core/src/error/user_error.rs  # line 23

# ADR-0007 Tier 2 — 5 high-freq struct form
grep -E "^\s+(NotFound|AlreadyExists|InvalidItem|Internal|Validation)\s*\{" \
     crates/synthia-core/src/error/error.rs

# ADR-0009 — builder trio
grep -E "with_context|with_operation|set_source" crates/synthia-core/src/  # only with_context

# ADR-0010 — `.context()` wrappers
grep -E "\.context\(|\.with_context\(|anyhow!|bail!" crates/synthia-context/   # 0 matches
rg "section\.build\(ctx\)" crates/synthia-context/src/                          # 3 bare-? sites
```

---

## Summary by ADR

| ADR | Title | Verdict | Status |
|-----|-------|---------|--------|
| 0007 | Error 架构稳定性契约 (P2 阶段) | Tier 1 wire stable, Tier 2 variants stable, Tier 3 helpers broadened. All `#[non_exhaustive]` markers correct. **3 new ErrorCode + 6 new ErrorCode additions undocumented in ADR (F4)**. | **Implemented** (with 4 minor alignment gaps) |
| 0008 | snafu 整体迁移可行性评估 (No-Go) | No snafu in tree. Workspace `Cargo.toml` clean. ADR-0008 Appendix A evidence preserved only in ADR text; the `/tmp` bench artifact not in repo. | **Implemented** |
| 0009 | OpenDAL `ErrorKind + Error` 模式评估 (Partial) | Only `with_context` delivered out of the 3-builder-method trio. `with_operation` and `set_source` absent. | **Deviates** |
| 0010 | synthia-context `anyhow::Result` Strategy (Option B) | All 5 implementation steps from §"Implementation steps (Option B, max 5)" are **unmet**: 0 of 3 `.context()` wrappers applied (F1, Blocker), 2 of 2 infallible helpers still use `anyhow::Result` (F2, High). | **Missing** |

---

**File**: `/home/crochee/workspace/synthia/docs/architecture/review/p3-spec-review.md`
**Created**: 2026-08-05
