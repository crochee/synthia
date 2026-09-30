# R32 Absorption Plan — 2026-09-12

Sequel to R31 (`75953f79`). Standing objective unchanged: keep
absorbing the best designs from `~/workspace/traitclaw/`,
`~/workspace/deepseek-harness/`, `~/workspace/pi/` and
`~/workspace/pi-subagents/` into a runtime-neutral, Lego-composable
Rust agent framework that is usable as a library and as a
production deployment.

Two things distinguish this round from R29–R31.

**It opens with an audit, not with new work.** R31 committed its four
slices with Wave 2 missing and the tree broken (a NUL-corrupted test
file, an unformatted workspace, `synthia-workflow` never compiled,
three tests asserting behaviour the code never had). The full repair
record is in
[`optimization-report-R31-2026-09-12.md`](optimization-report-R31-2026-09-12.md).
R32 fixes all of it first, then adds.

**Every slice's tests run before its report is written.** The R31
lesson was procedural: a slice is not "landed" because it compiles on
the author's machine, it is landed when its crate's tests pass on the
committed tree.

## Phase 0 — R31 debt (must complete before new absorption)

- **P. Named tool groups** (`synthia-tool`) — traitclaw
  `traitclaw-core/src/registries.rs::GroupedRegistry`. A read-side
  wrapper over `Arc<ToolRegistry>`: `visible_tool_names()` returns the
  tools of **active** groups only (what the model is offered), while
  dispatch is untouched, so a tool whose group is deactivated
  **stays executable** — the visibility ≠ executability split that
  skills, workflow steps and subagents rely on.
- **Q. Progressive example ladder** (all crates + `docs/examples/README.md`)
  — 12+ minimal one-seam examples, each runnable offline with no API
  key, indexed in a new docs README: they are the executable half of
  the "use synthia as a library" story.
- **Report + README + CHANGELOG** for R31.

## Phase 1 — new absorption (file-disjoint, parallel)

- **C. Output transformers + full-output retrieval**
  (`synthia-core`, `synthia-tool`, `synthia-steering`) — traitclaw
  `traitclaw-core/src/transformers.rs`. Three pieces:
  `FullOutputStore` + `InMemoryFullOutputStore` (bounded by entries
  **and** bytes, deterministic LRU) in `synthia-core`; the
  `__get_full_output` virtual tool in `synthia-tool` (the model-facing
  retrieval path for elided output); and the transformer library in
  `synthia-steering` (`TransformerChain`, `JsonExtractor`,
  `BudgetAwareTruncator`/`ProgressiveTransformer`). The store trait
  lives in `synthia-core` because both the tool crate and the
  steering crate need it and neither may depend on the other.
- **D. Team orchestration + rule-based routing** (`synthia-agent`) —
  traitclaw `traitclaw-team`. `BoundAgent` is a deliberately thin
  agent seam (no sessions, no provider) so a team composes without a
  runtime; `VerificationChain` is the generate-verify-retry loop with
  informed retries; `RoundRobinGroupChat` + `TerminationCondition` is
  the multi-agent conversation with an explicit stop reason; and
  `ConditionalRouter` is the ordered, first-match-wins regex rule
  table, implemented against the **existing** `Router` trait so it
  composes with `LeaderRouter` rather than beside it.
- **SQLite-backed memory tier** (`synthia-context`, feature `sqlite`)
  — traitclaw-memory-sqlite. `SqliteMemory` completes the memory
  story FileMemory started in R31: one file, FTS5 (BM25) recall
  instead of a keyword scan, a durable working-memory table, and a
  session registry. The conversation tier deliberately delegates to
  the inner sink-backed `Memory` (a divergence from traitclaw,
  documented in-module) because synthia's durable event log is
  already the conversation source of truth.

### Design decisions taken up front

| Decision | Alternative | Why this one |
|---|---|---|
| Memory backend as a **feature** of `synthia-context` (`sqlite`) | a new `synthia-memory-sqlite` crate | `MemoryError` is `Clone + PartialEq + Eq`; a separate crate would force either an opaque error variant or a cycle. A feature keeps typed errors, matches `FileMemory`'s placement, and costs the default build nothing |
| `FullOutputStore` in `synthia-core` | in `synthia-tool` | the tool crate and the steering crate both need it and neither may depend on the other; core is the shared-vocabulary crate |
| Teams over `BoundAgent` (thin closure seam) | over `Arc<dyn Agent>` | teams must compose without a session, a provider or a runtime; the thin seam is what makes them unit-testable with closures |
| `ConditionalRouter` implements the existing `Router` trait | a new routing trait | one routing vocabulary; `LeaderRouter` (mentions) and `ConditionalRouter` (rules) are two strategies behind one seam |

## Verification (final state)

- `cargo +nightly fmt --all --check` — exit 0.
- `make lint-rust` (`cargo clippy --all-targets --all-features --tests --all -- -D warnings`) — 0 warnings.
- Per-crate `cargo test -p <crate>` for all 18 crates with an explicit failure scan (AGENTS.md §3.3 forbids `cargo test --workspace`), plus `cargo test -p synthia-context --features sqlite`.
- Every example in the ladder runs offline: `cargo run --example <name> -p <crate>`.
- `docs/examples/external-consumer` → `CONSUMER-PROOF: OK`.
- README primitive rows + member inventory, CHANGELOG entries,
  `docs/optimization-report-R31` (results + repair),
  `docs/examples/README.md` (ladder index).

## Still deferred (R33 candidates)

Streaming tool-argument repair (`repairJson`/`parseStreamingJson` —
salvage truncated tool arguments instead of only refusing them);
`DeferredHandle` / cross-gateway `OperationRequest` polling;
compaction log-only events + `SurfaceOp.replace` checkpoint;
`assistant/chunk` live-resume; `SessionEventMap` extensibility;
FsAdapter error boundary; `shutdownChildSession` lifecycle
refinements; assistant-message delta frames; provider error-body
normalisation (`normalizeProviderError`); MCTS-style branch scoring;
provider auth flows (PKCE / device-code / credential store).
