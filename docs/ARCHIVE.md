# Archive — frozen design and optimization records

> **Read me first.** This page lists **historical** documents: design
> specs, optimization reports, and review records from rounds that
> are already merged into the code. They are intentionally **not**
> linked from the live docs (the ones indexed in
> [`INDEX.md`](INDEX.md)). The live docs cite only what is **current**.

## Why this page exists

Two project rules make a frozen archive necessary instead of a flat
directory listing:

1. **Authoritative language** (`AGENTS.md` §3.7, "claim-language
   gate"). `README.md`, the live specs in `docs/superpowers/specs/`,
   and the per-crate `lib.rs` rustdoc are scanned for absolute
   phrasing (`paradigm only` / `default tool set` / `ships no
   implementations` / etc.). Frozen records are **exempt** from that
   scan; preserving them verbatim keeps bisect honest.
2. **Review continuity.** Code that landed in round `R<n>` may be
   touched again in `R<m>`; the round report is the only place that
   describes the change set in the round's own terms. Deleting them
   loses the context for future reverts or refactors.

So this page is **catalog** — it points at what is in the archive,
not a recommendation to read any specific document. If you arrived
here looking for *current* design, go to [`INDEX.md`](INDEX.md)
§3 (`docs/superpowers/specs/`) instead.

## 1. Frozen specs (`docs/superpowers/specs/`)

The specs dated **before 2026-09-19** describe an earlier architecture
phase (v3 / unified-registry / pipeline-stage) that has since been
rewritten under the harness + interceptor seam. The names are kept
because:

- some of their vocabulary still appears in ADR / review notes;
- their rationale — *why we moved off* — is the basis for the active
  `synthia-harness` design.

Specs dated **on or after 2026-09-19** are **active** and live in the
live index. Frozen ones:

| Date | Spec | Why frozen |
| :--- | :--- | :--- |
| 2026-05-31 | [`synthia-production-ready-design`](superpowers/specs/2026-05-31-synthia-production-ready-design.md) | superseded by 2026-09 production-ready follow-ups |
| 2026-07-12 | [`synthia-v3-tool-first-architecture-design`](superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md) | pre-harness, replaced by `synthia-harness` ReAct core |
| 2026-07-18 | [`synthia-design-review`](superpowers/specs/2026-07-18-synthia-design-review.md) | round review, not a contract |
| 2026-07-18 | [`synthia-unified-registry-architecture-design`](superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md) | replaced by `synthia-core::registry::Registry` + `synthia-search::Registry` |
| 2026-07-24 | [`synthia-fullstack-integration-design`](superpowers/specs/2026-07-24-synthia-fullstack-integration-design.md) | pre-current web stack, superceded by 2026-08-13 web specs |
| 2026-07-26 | [`synthia-restful-api-v1-design`](superpowers/specs/2026-07-26-synthia-restful-api-v1-design.md) | v1 REST contract; v1.x routes now governed by `docs/interface-contract/` |
| 2026-07-29 | [`pipeline-stage-refactoring-design`](superpowers/specs/2026-07-29-pipeline-stage-refactoring-design.md) | pre-flatten design; flatten landed in `R50` |
| 2026-08-02 | [`migrate-core-tool-to-synthia-tool-design`](superpowers/specs/2026-08-02-migrate-core-tool-to-synthia-tool-design.md) | supersede trace; the move is now historical fact |
| 2026-08-02 | [`protocol-partial-migration-design`](superpowers/specs/2026-08-02-protocol-partial-migration-design.md) | superseded; full migration landed |
| 2026-08-02 | [`synthia-tool-responsibility-convergence-design`](superpowers/specs/2026-08-02-synthia-tool-responsibility-convergence-design.md) | superseded by the current `synthia-tool` + plugin split |
| 2026-08-02 | [`synthia-tool-责任收束-design`](superpowers/specs/2026-08-02-synthia-tool-责任收束-design.md) | Chinese-language counterpart of the above |
| 2026-08-04 | [`tool-registry-streaming-output-design`](superpowers/specs/2026-08-04-tool-registry-streaming-output-design.md) | superseded by current `Tool` exposure projection |
| 2026-08-05 | [`error-code-http-status-design`](superpowers/specs/2026-08-05-error-code-http-status-design.md) | superseded by `crates/synthia-server/src/error.rs` current shape |
| 2026-08-08 | [`synthia-agent-责任收束-design`](superpowers/specs/2026-08-08-synthia-agent-责任收束-design.md) | pre-`synthia-harness` rename; harness absorbed `synthia-agent` |
| 2026-08-09 | [`agent-trait-and-registry-design`](superpowers/specs/2026-08-09-agent-trait-and-registry-design.md) | superseded by harness + `AgentRegistry` current shape |
| 2026-08-12 | [`registry-center-design`](superpowers/specs/2026-08-12-registry-center-design.md) | superseded by `synthia-core::registry::Registry` |
| 2026-08-13 | [`synthia-web-light-theme-redesign-design`](superpowers/specs/2026-08-13-synthia-web-light-theme-redesign-design.md) | round R38 work; web moved on |
| 2026-08-13 | [`synthia-web-radix-themes-migration-design`](superpowers/specs/2026-08-13-synthia-web-radix-themes-migration-design.md) | round R38 work; web moved on |
| 2026-08-14 | [`chat-artifact-cards-design`](superpowers/specs/2026-08-14-chat-artifact-cards-design.md) | round R39; chat cards now in `synthia-web/src/` |
| 2026-08-15 | [`a2a-test-fetch-impl-design`](superpowers/specs/2026-08-15-a2a-test-fetch-impl-design.md) | one-shot; not part of the active seam set |
| 2026-08-20 | [`synthia-business-plugins-design`](superpowers/specs/2026-08-20-synthia-business-plugins-design.md) | superseded by the current plugin split (`-read` / `-write` / `-shell` …) |
| 2026-08-20 | [`synthia-kernel-enhancement-design`](superpowers/specs/2026-08-20-synthia-kernel-enhancement-design.md) | superseded by current `synthia-core` shape |
| 2026-08-20 | [`synthia-runtime-capabilities-design`](superpowers/specs/2026-08-20-synthia-runtime-capabilities-design.md) | superseded by `synthia-harness` runtime split |

Specs with their own subdirectory (`2026-08-09-agent-runtime-pipeline/`)
follow the same rule: keep the directory, do not promote its contents
to the live index.

## 2. Frozen optimization reports (`docs/optimization-report-R*.md`)

There are ~96 round-numbered reports covering rounds `R4` through
`R119` (gaps in the sequence — `R63`, `R65`-`R72`, `R75`, `R89` —
are real: those rounds either merged into adjacent ones or never
opened as a public record).

Filenames: `optimization-report-R<n>-YYYY-MM-DD.md` (or
`optimization-report-R<n>-YYYY-MM-DD-plan.md` for the planning half,
or `optimization-report-R<n>-YYYY-MM-DD-remaining-contract.md` for the
open-tail half). They are listed in lexicographic order on disk; the
following grouping is the read map:

| Round | Theme | Entry-point exemplar |
| :--- | :--- | :--- |
| R4–R13 | Tool / harness split, plugin extraction, paradigm vs. builtins | [`R10`](optimization-report-R10-2026-09-11.md) + [`R10-plan`](optimization-report-R10-2026-09-11-plan.md) |
| R14–R28 | Bulk follow-ups; combined in one batch | [`R14-R28`](optimization-report-R14-R28-2026-09-11.md) |
| R29 | Tool exposure + reflection work | [`R29`](optimization-report-R29-2026-09-11-plan.md) + [`R29-remaining`](optimization-report-R29-remaining-contract.md) |
| R30–R39 | Stream builder / loop detector / flat registry | [`R30`](optimization-report-R30-2026-09-12.md) · [`R32`](optimization-report-R32-2026-09-12.md) · [`R33`](optimization-report-R33-2026-09-12.md) |
| R40–R50 | Harness simplification (13 waves, ~5700 LOC removed) | [`R46`](optimization-report-R46-2026-09-12.md) |
| R51–R62 | Memory tier + working memory tier + telemetry split | [`R51`](optimization-report-R51-2026-09-12.md) · [`R58`](optimization-report-R58-2026-09-12.md) |
| R64 | Sandbox policy landed in `synthia-tool-shell` | [`R64`](optimization-report-R64-2026-09-14.md) |
| R73–R88 | MCP control tool + register-remote + protocol-eval work | [`R73`](optimization-report-R73-2026-09-15.md) · [`R74`](optimization-report-R74-2026-09-15.md) · [`R82`](optimization-report-R82-2026-09-15.md) |
| R90–R99 | Workflow runtime + replay; multi-agent simplification | [`R90`](optimization-report-R90-2026-09-16.md) · [`R94`](optimization-report-R94-2026-09-16.md) |
| R100–R110 | Facade re-export discipline + claim-language gate + claim-language evidence | [`R100`](optimization-report-R100-2026-09-17.md) · [`R104`](optimization-report-R104-2026-09-17.md) |
| R112–R119 | Tier-aware steering + tool-render contract + router + snapshot bus + RAG removal | [`R115`](optimization-report-R115-2026-09-18.md) · [`R116`](optimization-report-R116-2026-09-18.md) · [`R117`](optimization-report-R117-2026-09-18.md) |

For the current round's open tail, see [`CHANGELOG.md`](../CHANGELOG.md)
`[Unreleased]` and [`docs/ROADMAP.md`](ROADMAP.md) — those are
**live**, not frozen.

## 3. Frozen review / gap / follow-up records

| File | Why frozen |
| :--- | :--- |
| [`docs/architecture/review/p3-final-review.md`](architecture/review/p3-final-review.md) | R3 review |
| [`docs/architecture/review/p3-spec-review.md`](architecture/review/p3-spec-review.md) | R3 spec audit |
| [`docs/architecture/review/p3-test-coverage-review.md`](architecture/review/p3-test-coverage-review.md) | R3 coverage audit |
| [`docs/R3_FOLLOW_UP_RESOLVED.md`](R3_FOLLOW_UP_RESOLVED.md) | R3 follow-ups, all closed |
| [`docs/optimization-report-2026-08-15.md`](optimization-report-2026-08-15.md) | Pre-R-number baseline report |
| [`synthia-r5-gap-analysis.md`](../synthia-r5-gap-analysis.md) | R5 gap analysis, closed |
| [`docs/SERVER-ROUTES-TODO.md`](SERVER-ROUTES-TODO.md) | Snapshot of remaining route work; merged into CHANGELOG `[Unreleased]` |

## 4. Frozen ADR precursors (`docs/architecture/adr/`)

All ADRs in [`docs/architecture/adr/`](architecture/adr/) are accepted
records and remain authoritative for the decision they describe. They
are not "frozen" in the suppression sense; they are listed here only
because the `INDEX.md` already lists them as the canonical ADRs and
this section would otherwise repeat the table.

## 5. Frozen / historical interface-contract documents

`docs/interface-contract/` carries the **current** contract; its
readme is the live pointer. Older snapshots, if any, were merged into
the live versions and removed from the directory listing.

---

## What is **not** in this archive

- Active specs (2026-09-19 onward) — see [`INDEX.md`](INDEX.md) §3
- Active decisions — see [`INDEX.md`](INDEX.md) §3
- Active plans — see [`INDEX.md`](INDEX.md) §3
- The current CHANGELOG `[Unreleased]` — it is a **live** record
- The current ROADMAP — it is a **live** plan

If you cannot find a document here or in [`INDEX.md`](INDEX.md), it
either does not exist or has been intentionally removed; in the
latter case the removal commit message is the canonical pointer.