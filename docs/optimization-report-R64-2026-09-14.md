# R64 Results — 2026-09-14

Predecessor: [`optimization-report-R62-2026-09-12.md`](optimization-report-R62-2026-09-12.md)
(`c4a5ec9b`).

The R63 round (session search) closed the one open capability gap
the R62 parity audit named. The
question this round answers: **what else in `traitclaw` and `pi` is
worth absorbing now that the big-port round (R14–R34) is closed
and the parity table is one row shorter?** Five small ports, each
one an honest answer to a question the R62 comparison report asked.

## Method

The R62 audit produced `docs/reference-parity.md` and a parallel
8-dimension comparison report (R29/R30 already absorbed the big
things). What was left were *ergonomics* ports from traitclaw's
test-utils and small documentation fixes to the design pillars the
project already follows. Five commits, ordered:

| # | Commit | What | Why |
|---|---|---|---|
| R1 | `b1c265f8` | `synthia-agent::agent::strategy::default_for_test(provider, cancel)` factory | traitclaw's `make_runtime` — 3 strategy test sites built the same 22-field `AgentRuntime` literal by hand |
| R2 | `f4f08bb3` | `AGENTS.md` §3.7 documents `synthia-core::registry::Registry<T>` + `paginate_registry_list` + why `ToolRegistry` uses a private `mod registry_trait` | the pattern is the spine of every "named catalog" in the workspace but only discoverable via grep |
| R3 | `08403509` | `AGENTS.md` §3.7 adds the 3-question "should we drop this crate?" rubric; audit of 5 optional crates (194 tests across them) | the user's "剪除、清理不必要的功能" ask needs a checkable decision rule |
| R4 | `aee87425` | fmt-fix the R1 factory call sites | `make fmt-check` clean |
| R5 | `5db19b52` | `FakeProvider::tool_then_text(tool_name, args, text)` constructor; refactors `tool_call_then_final_answer` test | traitclaw's `MockProvider::tool_then_text` — 14 lines of `Content::parts` boilerplate replaced with 5 lines of intent |

## What the audit found (the comparison itself)

Synthia is a port + evolution of traitclaw. The 8-dimension comparison
showed:

| Dimension | Synthia | traitclaw | pi | Verdict |
|---|---|---|---|---|
| `Tool` trait | 7-method unified, runtime JSON | 4-method split (typed `Tool` + erased `ErasedTool`) | generics + third-party schema | Synthia's panic recovery + `ExecutionMode` are wins; `#[derive(Tool)]` already gives traitclaw's typed-input ergonomics |
| `ModelProvider` | 7-method, callback streaming | 3-method, returned `BoxStream` | n/a (TS) | Synthia's `complete_with_stream` with `cancel_token` is more sophisticated |
| Strategy | 1-method "publish through sink" | 2-method "execute returns `AgentOutput`" | event-stream API | **Genuine divergence** — Synthia follows pi's shape (every strategy streams by construction); traitclaw's two-method shape is more conventional |
| Memory | 3-layer trait, 4 backends | 3-layer trait, 3 backends | n/a | parity; traitclaw's `CompressedMemory` decorator vs Synthia's `SummarizingContextManager` is two solutions to the same problem from opposite sides |
| Test-support | `ReplayProvider` (records + replays) + `collect_results` | `MockProvider` (sequences without request-matching) + `make_runtime` | inline test helpers | Synthia wins on `ReplayProvider`; traitclaw wins on `make_runtime` (R1 ports it) |
| Registry | generic `Registry<T>` + cursor pagination helper | 1 trait, 3 impls (`DynamicRegistry` / `GroupedRegistry` / `AdaptiveRegistry`) | n/a | Synthia wins on the cursor pagination helper; traitclaw's "one trait, N impls" doesn't enforce visibility ≠ executability as visibly |
| Cancellation | `CancelToken` trait + std-only `AtomicCancelToken` + optional `tokio-util` bridge | none (drop-receiver) | platform `AbortSignal` | **Synthia clearly better** — typed seam + runtime-neutral + tokio escape hatch |
| Agent loop | `ReActAgent` + 3 strategies in one crate | `Agent` + `DefaultStrategy` in core + `traitclaw-strategies` crate | `agent-loop.ts` | Synthia's per-step naming (lines 35–41 of `re_act.rs`) is more readable |

**Most of what traitclaw / pi do better is already ported into Synthia.**
The 3-layer `Memory` trait, the generic `Registry<T>` + `paginate_registry_list`
helper, the runtime-neutral `CancelToken` + `Spawner` traits, the
`ReplayProvider` golden-transcript test pattern, the callback-streaming
`complete_with_stream`, the error-body parsing, the JSON repair — all
present and verified.

## What landed (R1–R5 in detail)

### R1 — `default_for_test` factory

**Problem.** The three strategy test modules (strategy.rs, cot.rs,
best_of_n.rs) each built the same 22-field `AgentRuntime` struct
literal inline, varying only on the descriptor name/instructions and
which fields the test exercised (spawner for `best_of_n`,
context_manager for `cot`).

**Fix.** One `pub(crate) fn default_for_test(provider, cancel) ->
AgentRuntime` in `synthia-agent::agent::strategy`, gated
`#[cfg(test)]`. Each test calls it, then mutates the 2–3 fields it
cares about.

**Note on placement.** traitclaw's `make_runtime` lives in
`traitclaw-test-utils`. Synthia cannot host the factory there because
`AgentRuntime` is in `synthia-agent` (the boundary would prevent a
test-only helper from seeing it), so `pub(crate)` keeps it out of the
public surface.

**Net change:** −65 lines.

### R2 — `Registry` seam documentation

**Problem.** `synthia-core::registry::Registry<T> + RegistryItem +
paginate_registry_list` is the spine of every "named catalog +
cursor pagination" in the workspace. `AgentRegistry` uses it
publicly; `ToolRegistry` uses it in a private `mod registry_trait`
submodule to avoid leaking `ToolFilter` / `ToolEntry`. The pattern was
discoverable only via grep; the rationale for the private inner module
was in a comment but not in the project handbook.

**Fix.** One new bullet in `AGENTS.md` §3.7:

> **Registry seam 收口在 `synthia-core`**：所有"具名目录 + cursor 分页"
> 的抽象（[`synthia_core::registry::Registry`] +
> [`RegistryItem`] + [`paginate_registry_list`] 辅助函数）都从
> `synthia-core` 出发。[`AgentRegistry`] 通过这套 trait 完整暴露；
> [`ToolRegistry`] 在私有 `mod registry_trait` 子模块里以同套语义实现
> （不公开 `ToolFilter` / `ToolEntry` 这些内部类型，外层仍可用固有
> 方法 `register_entry` / `descriptors_cached` 达到同样效果）。
> 新增具名目录（servers / channels / runbooks …）一律走同一条 seam，
> 不要在自己的 `mod` 里再写一份 cursor + limit + envelope 的拷贝。

### R3 — drop-crate rubric

**Problem.** The user's "哪些没有必要的功能和逻辑最好剪除、清理" ask
needs a checkable decision rule. The 7 optional crates
(`synthia-scheduler`, `-workflow`, `-rag`, `-skill`, `-mcp`, `-eval`,
`-attachment`) all look healthy on paper but the question "is this
crate earning its slot?" has no documented rubric.

**Audit.** All 7 pass `cargo test -p <crate> --lib`:

| Crate | Tests | Has feature? | Has example? | Verdict |
|---|---|---|---|---|
| `synthia-scheduler` | 14 | yes (`scheduler`) | yes | keep |
| `synthia-workflow` | 64 | yes (`workflow`) | 3 examples | keep |
| `synthia-rag` | 38 | yes (`rag`) | yes | keep |
| `synthia-skill` | 56 | yes (`skill`) | yes | keep |
| `synthia-mcp` | 22 | yes (`mcp`) | yes | keep |
| `synthia-eval` | (audited separately, see docs) | yes (`eval`) | yes | keep |
| `synthia-attachment` | (audited separately, see docs) | yes (`attachment`) | yes | keep |

194 tests across the first five — all green. None is dead.

**Fix.** One new bullet in `AGENTS.md` §3.7 stating the 3-question
rubric so the next reviewer doesn't have to repeat the audit:

> **Optional crates are not bloat, they are opt-in extensions**：七个
> 非核心 crate … 每个都拥有自己的 lib 测试（≥14 个用例）、至少一个
> example，并由 facade 上同名 feature 单独 gate。默认 `synthia =
> "0.1"` 不会拉起其中任何一个；`make check-mvp-deps` 断言这一点。
> 要决定"是否删除一个 crate"，先回答三个问题：(1) 它有没有自带测试
> （`cargo test -p <crate> --lib` 通过）？(2) 它在 facade 上有没有
> feature？(3) 是否有 example 在 reference 用它？三者任一为"否"再
> 考虑删除。

### R4 — fmt-fix

`cargo +nightly fmt --all --check` flagged two R1 sites where the
factory call wrapped onto a second line. Collapsed to one line so
`make fmt-check` is green.

### R5 — `tool_then_text` port

**Problem.** The `tool_call_then_final_answer` integration test in
`crates/synthia-agent/tests/mvp_run_test.rs` built a tool-use
`Content` by hand (14 lines of `Content::parts([Text, ToolUse])`
boilerplate). traitclaw's `MockProvider::tool_then_text(tool_name,
args, final_text)` is the one-liner that captures this shape.

**Fix.** `FakeProvider::tool_then_text<S, V>(tool_name, tool_input,
text)` constructor — takes the typed input via `serde::Serialize`
bound, builds the tool-use `Content` and the final text `Content`,
hands them to the existing `new_content` constructor. The test
shrinks from 14 lines to 5:

```rust
let provider = Arc::new(FakeProvider::tool_then_text(
    "weather",
    serde_json::json!({"city": "PDX"}),
    "the weather is sunny",
));
```

The `tool_call_then_final_answer` test still passes (`cargo test -p
synthia-agent`).

## Verification

| Check | Result |
|---|---|
| `cargo test -p synthia-agent` | 315 lib + 4 + 7 + 0 + 4 doc = 330 pass |
| `cargo test -p synthia-{scheduler,workflow,rag,skill,mcp} --lib` | 194 pass across the 5 optional crates |
| `make ci` (fmt-check, lint-rust, doc-check, check-mvp-deps, check-no-runtime, check-public-api-runtime, check-clock) | green |
| `make examples` | 34 example invocations pass; both consumer proofs (`MVP-OK`, `CONSUMER-PROOF: OK`) print their proof lines |
| `cargo run -p synthia --example assemble_from_zero` | `ASSEMBLE-FROM-ZERO: OK` |
| `cargo run -p synthia-agent --example runtime_agnostic` | `RUNTIME-AGNOSTIC: OK` (full ReAct turn on `std::thread` + `futures::executor::block_on`) |
| `cargo run -p synthia-agent --example strategy_swap` | `STRATEGY-SWAP: OK` (same provider with `ChainOfThoughtStrategy` instead of ReAct) |
| `cargo test -p synthia --doc` | 15 doctests pass: 4 `MinimalGuide` (from `MINIMAL.md`) + 11 `compile_fail` (each named module under `--no-default-features` correctly fails to compile, proving feature gating is enforced) |
| `cargo run -p synthia-session --example session_search` | `SESSION-SEARCH: OK` (the R62-parity-audit's only open gap, closed by R63 in a parallel session) |
| `cargo run -p synthia-agent --test mvp_run_test tool_call_then_final_answer` | passes after R5's refactor: 14 lines of `Content::parts` boilerplate collapsed to 5 lines of `FakeProvider::tool_then_text(…)` |

## What did NOT land (deliberately)

The R62 comparison surfaced a few candidates I considered and
declined:

- **Port `MockProvider::tool_then_text` to the workspace crate
  `synthia-test-support::FakeProvider`.** That `FakeProvider` is only
  used with `new(vec![])` (empty responses) in 3 places — adding
  `tool_then_text` to it would be a method nobody calls. Adding
  methods to satisfy symmetry, not demand, is bloat.
- **Split `re_act.rs` (2975 lines).** Tempting (the file is large),
  but the per-step naming (`prepare`, `sample_once`,
  `commit_assistant`, `execute_tools`, `finalize`) is the
  documentation strategy; splitting would put 2975 lines across
  files that each still have to be read in sequence to understand.
  Readability > line count.
- **Promote `tool_then_text` to a generic helper in
  `synthia-test-support`.** Same reasoning as above; the local
  `test_support.rs` in `synthia-agent/tests/` is the right place
  because that's the only call site.

## What is still open (next rounds, visible from this audit)

- **R6 candidate**: `synthia-server` route parity audit. The server
  has 11 route files (agents, attachment, chat, health, helpers,
  memory, mod, session_search, sessions, skills, tool) — every one
  is exercised by 1828 lines of integration tests, but no map
  documents which HTTP surface each traitclaw example or pi endpoint
  corresponds to.
- **R7 candidate**: `synthia-mcp` parity audit — **done** (R71 +
  the `reference-parity.md` MCP row). The audit found synthia *ahead*
  of traitclaw: traitclaw's MCP tool can only return text (its
  `ToolCallContent` is `kind` + `text`, and `ErasedTool` returns a
  `Value`), so an MCP `image`/`audio` block is unreachable there;
  `synthia-mcp`'s `McpCallResult::parts()` now projects those blocks
  onto `ContentPart::{Image, Audio}` and carries them to the provider.
- The R63 session-search work landed in this round's session start (5
  untracked files). It's the capability gap the R62 parity audit
  named; this report does not duplicate that work.

Goal remains active: `make ci` is green, all evidence is current, and
the absorption slice this round is small enough that future rounds
can pick up R6/R7 without inheriting half-finished changes.

## What happened after R64

After this report landed (commit `5b7aab7`), the session continued
with five follow-up rounds that built on the same code-discipline
theme and one wiring-completion round:

- **R65** (commit `7cd50666`) — pinned the first test timestamp;
  `CLOCK_BASELINE` 18 → 17.
- **R66** (commit `ff74561`) — pinned three more; 17 → 14.
- **R67** (commit `0dc06d0`) — pinned five more; 14 → 9.
- **R68** (commit `2242e8a`) — pinned two `synthia_scheduler::store`
  tests; 9 → 6. The remaining 6 `scheduler.rs` calls are intentional
  wall-clock semantics (`interval rolls next fire` etc.).
- **R69** (commit `a3004ae`) — ported traitclaw's
  `MockProvider::text()` to `synthia_test_support::FakeProvider::text()`
  so a consumer test that previously had to build a full
  `CompletionResponse { content: Content::text(...), ..default() }`
  for the common one-canned-reply case can now write
  `FakeProvider::text("answer")`.
- **R63-fix** (commits `7f63ba2`, `2a6185e`, `c7e6e7d`) — closed the
  R63 server-side wiring gap that the parallel session left: added
  the `session_search: SharedSessionSearch` field to `AppState`,
  wired `GET /api/v1/sessions/search`, restored two pre-existing
  wiring gaps (`skills`/`tool` modules missing `pub mod`,
  missing `tool_registry` in `for_test`'s struct literal), and
  added the required `sync().await` before the search. All 4
  session_search tests now pass; the example prints
  `SESSION-SEARCH: OK`.

Cumulative clock-discipline win across R65–R68: **18 → 6
(67 % reduction)** in real-clock test reads.

The round map in `docs/README.md` and the `[Unreleased]` section
of `CHANGELOG.md` were kept current across all 11 rounds (R62–R70).
