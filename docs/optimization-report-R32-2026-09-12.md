# R32 Absorption Results — 2026-09-12

Plan: [`optimization-report-R32-2026-09-12-plan.md`](optimization-report-R32-2026-09-12-plan.md).
Commits: `c983820d` (R31 repair + credential-free suite),
`113e0755` (R32 features), plus the docs/examples commit carrying
this file. R31's own record:
[`optimization-report-R31-2026-09-12.md`](optimization-report-R31-2026-09-12.md).

R32 opened with an audit instead of new work, because R31 committed a
broken tree. It then landed five absorption slices, the progressive
example ladder, and the test-hygiene fixes that make the suite
independent of ambient credentials.

## Phase 0 — R31 debt, completed

| Item | State |
|---|---|
| NUL-corrupted `tests/sandbox_smoke.rs` | rewritten as a real six-test smoke suite (real bubblewrap denial, capability preflight) |
| Unformatted tree (~20 hunks) | `cargo +nightly fmt --all --check` exits 0 |
| `synthia-workflow` never compiled (6 errors) | fixed (`wake_by_ref`, async tests, reborrowing closure, stale `drop`) |
| `synthia-agent` non-`Copy` moves + unused imports | fixed (zero-copy `Override` resolution; imports deleted) |
| 2 tests asserting behaviour the code never had | corrected to the documented contract |
| **P** — `GroupedRegistry` (R31 plan Wave 2) | landed (see slice below) |
| **Q** — example ladder + docs README | landed (22 examples, indexed) |
| R31 results report / README rows / CHANGELOG | written |

## What landed

### `GroupedRegistry` (R31 item P) — `synthia-tool`

`GroupedRegistry` + `GroupError` (`crates/synthia-tool/src/grouped.rs`,
9 tests: 6 unit + 3 integration) and `ToolRegistry::contains` as the
wiring-side lookup the validation needs.

- The wrapper offers the tools of **active** groups plus tools no group
  claims; `inner()` remains the dispatch authority, so a deactivated
  tool **stays executable** — proven end to end by
  `tests/tool_groups.rs::deactivated_group_tools_stay_executable`,
  which dispatches through `run_stream` after deactivation.
- Declarations are validated before any mutation: empty name, unknown
  tool, duplicate in one declaration, or a tool claimed by two groups
  all raise `GroupError`, so a typo cannot silently drop a tool.
- Divergences from traitclaw, all deliberate: a **read-side wrapper**
  rather than a registry of its own; **ungrouped tools stay visible**
  (grouping is opt-in narrowing, so a deployment that declares no group
  keeps its catalog); **one group per tool** so visibility never
  depends on declaration order; the visible list follows
  `snapshot()`'s sorted order for byte-stable output.

### SQLite-backed memory tier — `synthia-context` (feature `sqlite`)

`SqliteMemory` + `SqliteMemoryError`
(`crates/synthia-context/src/memory/sqlite.rs`, 8 tests).

- Long-term recall is `FTS5`/BM25 over a sanitised query: alphanumeric
  runs are quoted and joined with `OR`, so `NEAR(` and `*` are literal
  terms instead of syntax errors, and a term-less query falls back to
  newest-first (matching the trait's "an empty query matches all").
  Updating an id keeps the FTS index consistent through triggers.
- Working memory and the session registry are durable tables; the
  conversation tier delegates to the inner sink-backed `Memory`
  (documented divergence from traitclaw, which also stores messages:
  synthia's durable event log is already the conversation source of
  truth, and a second copy could only diverge).
- Read-only opens refuse every write with a typed error; a read-only
  open without a schema fails loudly instead of pretending to be empty.
- Placement: a **feature of `synthia-context`**, not a new crate,
  because `MemoryError` is `Clone + PartialEq + Eq` (a separate crate
  would force an opaque variant or a dependency cycle) and because
  `FileMemory` already lives there. New optional dependency `rusqlite`
  with `bundled` (FTS5 compiled in, no system `libsqlite3`); the
  default build is untouched.

### Output transformers + full-output retrieval — core / tool / steering

`FullOutputStore` + `InMemoryFullOutputStore` (`synthia-core`),
`__get_full_output` + `full_output_tool` (`synthia-tool`), and
`TransformerChain` / `JsonExtractor` / `BudgetAwareTruncator`
(`synthia-steering`). 29 tests (9 + 5 + 15).

- The store is bounded by entries **and** bytes with true LRU (a `get`
  refreshes recency; handles are never reused), and the newest entry
  survives a byte ceiling it alone exceeds.
- The published marker names the handle
  (`[truncated: N chars elided; full output via __get_full_output("out-1")]`),
  and the tool returns the stashed text or a model-facing error for an
  unknown handle. The store trait lives in `synthia-core` because
  `synthia-tool` and `synthia-steering` both need it and neither may
  depend on the other.
- Chain order is part of the contract: extract the JSON payload first,
  then budget it, so the stash holds the useful form — asserted by the
  chain test and by `examples/output_transformers.rs`, which recovers
  the payload through the tool.

### Teams and rule routing — `synthia-agent`

`agent::team` (`BoundAgent`, `FnAgent`, `Verifier`/`Verdict`,
`VerificationChain`/`ChainOutcome`, `RoundRobinGroupChat`,
`TerminationCondition`, `MaxRoundsTermination`, `MarkerTermination`,
`Turn`, `StopReason`) and `agent::route_rules::ConditionalRouter`
(+ `RoutePatternError`). 15 tests + 3 doctests.

- `BoundAgent` is deliberately the thinnest possible seam — a name and
  one `prompt -> text` call — so a team composes with no session, no
  provider and no runtime, and tests drive it with closures (the
  recording agent in the tests proves the retry prompt carries the
  verifier's rejection reason).
- A rejection is data (`ChainOutcome { attempts, accepted, rejections }`);
  only a transport failure is an `Err`, and it stops the chain.
- `ConditionalRouter` implements the **existing** `Router` trait, so
  rule-based routing and `@agent:` mention routing are two strategies
  behind one seam; a bad pattern is a typed error, never a panic.
- The group chat attributes every turn (`Turn { round, speaker, message }`)
  and reports *why* it stopped, including a failing participant and an
  empty participant list.

## The example ladder (R31 item Q)

22 examples across 8 crates, every one offline and credential-free,
indexed in [`docs/examples/README.md`](examples/README.md) with a
"what to look at" column per row. New this round: `tool_groups`,
`sandbox_policy`, `sqlite_memory`, `output_transformers`,
`agent_teams`, `workflow_fanout`, `derive_tool`, `eval_suite`,
`replay_provider`, `run_inbox_steering`, `runtime_context`,
`token_meter`, `delegation_gate`, `worktree_isolation`.

Observed proof lines (clean environment, no API keys):

```text
TOOL-GROUPS: OK          SANDBOX-POLICY: OK     SQLITE-MEMORY: OK
OUTPUT-TRANSFORMERS: OK  AGENT-TEAMS: OK        WORKFLOW-FANOUT: OK
DERIVE-TOOL: OK          EVAL-SUITE: OK         REPLAY-PROVIDER: OK
RUN-INBOX: OK            RUNTIME-CONTEXT: OK    TOKEN-METER: OK
DELEGATION-GATE: OK      WORKTREE-ISOLATION: OK CONSUMER-PROOF: OK
```

`sandbox_policy` exercised bubblewrap for real on this host
(`bwrap 0.12.0`, a refused write with `EROFS`, the denial marker, and
the probe file never created) and its `SKIPPED` branch was verified
separately by running the binary with `PATH=/nonexistent`.

## Test hygiene: the suite no longer needs credentials

The audit run that produced the numbers below ran with
`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, their `*_BASE_URL`/`*_MODEL`
companions **unset**. That surfaced twelve tests and one example that
only passed on a machine with credentials exported:

- `synthia-server` lib: `routes::health::{list_models_emits_etag_and_cache_control,
  list_models_returns_304_when_if_none_match_present,
  readyz_is_200_after_state_initialized}` and
  `server::tests::test_server_creation` boot the production path, which
  deliberately refuses to start with no provider configured. They now
  build an in-memory `for_test` state over a temp workspace.
- `tests/boot_wiring_test.rs` (2) and `tests/operation_endpoint_test.rs`
  (4) exercise the real boot path and must keep doing so; they now
  install a hermetic provider — `.agents/config.toml` whose
  `api_key_env` names a variable the test process sets — through the new
  `tests/support/mod.rs`. Nothing is ever sent anywhere: the provider is
  constructed, never called.
- `examples/assemble_with_provider_profile.rs` selected its default
  profile by insertion order (Anthropic) and so required a credential
  despite documenting the opposite; it now selects the keyless stub
  explicitly and the demo of the fail-fast error path is unchanged.

This is a real robustness fix, not a cosmetic one: the old suite would
fail on any clean CI runner that does not export model credentials.

## Environment repair (recorded, not part of the repo)

The consumer proof initially failed to build `cc 1.4.5` with
`E0583`/`E0599` inside the crate's own source. Cause: the extracted
registry directory was **missing four files the archive contains**
(`src/target/{apple,generated,llvm,parser}.rs`) — an incomplete
extraction of the same class as R31's NUL-byte file. The crate was
purged, re-extracted (cargo verifies the checksum and restores all 28
files), and the proof then passed. A full registry integrity scan
(569 extracted crates, archive file list vs extracted tree) reported
**zero** remaining incomplete extractions.

## Verification (final state)

| Gate | Result |
|---|---|
| `cargo +nightly fmt --all --check` | exit 0 |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | exit 0 (0 warnings) |
| Per-crate `cargo test -p <crate>`, credential-free env | **2521 passed / 0 failed**, every crate exit 0 |
| `cargo test -p synthia-context --features sqlite` | 101 passed / 0 failed |
| Example ladder | 22/22 exit 0, proof lines above |
| `docs/examples/external-consumer` | `CONSUMER-PROOF: OK` |

Per-crate totals (credential-free): core 94, telemetry 36, provider
711, context 93 (101 with `sqlite`), tool 288, skill 56, session 125,
steering 74, agent 301, server 418, attachment 15, mcp 52, rag 38,
scheduler 14, macros 31, eval 36, workflow 20, test-support 18.

New tests this round: 61 (grouped 9, sqlite 8, core store 9, tool
five-tool 5, transformers 15, team 15) plus 3 doctests.

R31 comparison: 2280 tests at R30, 2056 at R29; R32 reaches 2521 with
the repair and the new slices.

## Deliberate divergences from the references

| Area | Reference | synthia | Why |
|---|---|---|---|
| SQLite memory placement | separate `traitclaw-memory-sqlite` crate | feature `sqlite` of `synthia-context` | `MemoryError` is `Clone + Eq`; a separate crate would force an opaque error variant or a cycle. `FileMemory` already lives beside it |
| SQLite conversation tier | traitclaw stores messages in SQLite | delegates to the sink-backed `Memory` | synthia's durable event log is already the conversation source of truth; two copies can only diverge |
| Group membership | traitclaw `HashMap<String, Vec<Tool>>` | one group per tool, ungrouped tools visible | deterministic visibility independent of declaration order; grouping stays opt-in narrowing |
| Full-output store bounds | traitclaw `FullOutputRetriever` (count-based) | entries **and** bytes, true LRU, newest never evicted | a byte ceiling is what actually bounds memory for large tool results |
| Teams | traitclaw `Team` + roles | `BoundAgent` + `VerificationChain` + `RoundRobinGroupChat` | the thin seam composes without a session or provider, which is what makes it testable and runtime-neutral |
| Rule routing | traitclaw standalone `ConditionalRouter` | implements synthia's existing `Router` trait | one routing vocabulary; mentions and rules are two strategies behind the same seam |

## Deferred to R33 (recorded, not dropped)

Streaming tool-argument repair (`repairJson`/`parseStreamingJson` —
salvage truncated arguments instead of only refusing them, which pairs
with R30's length-stop guard); `DeferredHandle` / cross-gateway
`OperationRequest` polling; compaction log-only events +
`SurfaceOp.replace` checkpoint; `assistant/chunk` live-resume;
`SessionEventMap` extensibility; FsAdapter error boundary;
`shutdownChildSession` lifecycle refinements; assistant-message delta
frames; provider error-body normalisation (`normalizeProviderError`);
wire `ToolExposure::Deferred` (declared but consumed by nothing today);
MCTS-style branch scoring; provider auth flows (PKCE / device-code /
credential store).
