# Optimization report — rounds R14–R28 (2026-09-11)

> Consolidated record for the rounds after [`optimization-report-R13`](optimization-report-R13-2026-09-11.md).
> R4–R13 each have their own plan + report; from R14 on, the durable
> record is this file plus the `CHANGELOG.md` entry for each round.
>
> References: `~/workspace/traitclaw`, `~/workspace/deepseek-harness` (dsh),
> `~/workspace/pi`, `~/workspace/pi-subagents`. Every claim below is
> grounded in a commit on `master` and in the verification commands
> named at the end.

## 1. What the rounds were for

R4–R13 closed the **trait surface** axes (typed events, steering
triad, runtime-neutral cancellation, chrono, hooks, compaction
policy) and the **assembly** axis (`AgentBuilder`,
`assemble_from_scratch`). R14–R28 closed the three axes that were
still thin:

1. **Industry-standard capabilities** — Model Context Protocol
   (R19/R21), retrieval-augmented grounding (R20/R22).
2. **Production surface** — every capability reachable from
   deployment configuration, with a real end-to-end boot proof
   (R16, R21, R22, R25).
3. **Library craft** — one generic composition seam instead of one
   field per capability (R22), a deep module instead of a
   5 896-line file (R27), an audited runtime-neutrality and
   error-handling contract (R23, R24), and documentation that
   matches the tree (R28).

## 2. Round-by-round

| Round | Commit | What landed | Reference |
|---|---|---|---|
| R14 | `57c94c3b` | `assemble_with_builder` capstone example; README composition table (bricks 7–18) | traitclaw `crates/traitclaw-core/src/agent_builder.rs` |
| R15 | `a74552a1` | `CompactionSettings` gates `SummarizingContextManager` pruning; **fixed a real estimator bug** — tool-result / tool-use parts counted as 0 tokens | pi `packages/agent/src/harness/compaction/compaction.ts` |
| R16 | `9cf28f31`→`0aa099c2` | `agents.<name>.compaction` server config → run factory; fixed a flaky `mutation_queue` test that asserted spawn order | pi `compaction.ts`; pi `file-mutation-queue.ts` |
| R17 | `0670e98d` | `Tool::output_definition()` as a **default trait method** (a blanket impl on a separate extension trait made overriding impossible); all 5 builtins override it; registry + HTTP expose it | dsh tool-output shape; pi render kinds |
| R18 | `f0771a52` | `docs/examples/external-consumer` — independent crate, empty `[workspace]`, relative path deps, prints `CONSUMER-PROOF: OK` | tutorial axis (this repo's own promise) |
| R19 | `184cf35f` | **`synthia-mcp`**: `McpTransport` trait, `StdioTransport` (io lock, inherited stderr, `kill_on_drop`, ID-matched reader), `InMemoryTransport`, `McpClient`, `McpTool`, `register_mcp_tools` | MCP spec rev `2025-06-18` |
| R20 | `89791e13` | **`synthia-rag`**: `Retriever` seam, 3 chunkers, **real BM25**, `Embedder` + `EmbeddingRetriever` + `ModelProviderEmbedder`, RRF-hybrid, `RagContextManager` | traitclaw `crates/traitclaw-rag` (divergences documented in the crate docs) |
| R21 | `e27543e1` | `mcp_servers` config → spawn + publish tools at boot, fail-soft; `AppState::mcp_clients` keeps children alive | production surface |
| R22 | `7ea71742` | `retrieval` config → BM25 index at boot; **`AgentRunConfig.context_manager` replaced R16's `compaction` field** (one generic seam); public `ground_with_retrieval` with the ordering contract | production surface + library craft |
| R23 | `fb6f5d43` | Removed the last `tokio` type from a library public signature (`start_cleanup_task` → `CleanupTask`); re-ran the audit, examples, consumer proof | R6 principle |
| R24 | `ba30c93e` | Last three hand-rolled error enums → `thiserror`; `FoldError` gained `Display` + `Error` (it had neither) | `AGENTS.md` §3.1 |
| R25 | `5aa276bf` | MCP transport `label` (logs showed a whole `sh -c` script before); boot-wiring regression test through the real router | production hygiene |
| R26 | `b8c7f029` | **`GroupJoin`** — batched completion notification for background agents, clock injected, runtime-neutral | pi-subagents `src/group-join.ts` |
| R27 | `5f63c5c2` | `re_act.rs` 5 896 → 2 116 lines: `stream.rs` (deep module, 2-item interface) + `tests.rs` | codebase-design axis |
| R28 | `5c9bced8` | Crate inventory corrected (14 members: 12 libraries + `synthia-server` + `test-support`); neutrality claim scoped to libraries | documentation accuracy |

## 3. Deliberate divergences from the references

Recorded here because a faithful port is not the same as a good
one; each of these is pinned by a test.

| Area | Reference | synthia | Why |
|---|---|---|---|
| Keyword retrieval | `Σ tf/(tf+1)`, no IDF (its BM25 comment is dead code) | real BM25 (`k1=1.2`, `b=0.75`, IDF, length norm) | Without IDF a rare term cannot outrank a common one; `synthia-rag` pins that it does |
| Hybrid merge | weighted `0.3`/`0.7` on raw scores | **RRF (`k=60`)** by default; `Weighted` still available | BM25 is unbounded, cosine is bounded — a weighted merge needs a normalisation the caller cannot see |
| Score location | `score` on the document | `ScoredDocument` | A score belongs to a *(document, query)* pair |
| Grounding insertion | `messages.insert(0, …)`, ahead of the system prompt | after leading system messages | synthia's prompt assembler keeps identity / tools / skills in the high-attention head |
| Retrieval failure | swallowed | logged `warn`, degraded to no grounding | Never break a turn, never hide a miss |
| GroupJoin timers | ambient `setTimeout` | caller-supplied `now` + `next_deadline()` | Runtime neutrality; tests are deterministic without fake timers |
| Tool output contract | extension trait with a blanket impl | default method on `Tool` | A blanket impl on a separate trait makes concrete overrides impossible (R17) |

## 4. Verification on `5c9bced8`

```bash
cargo +nightly fmt --all -- --check          # exit 0
cargo clippy --workspace --all-features --all-targets -- -D warnings   # 0 warnings
# per crate (the repo bans one-shot `--workspace` test runs):
#   core 84, telemetry 36, provider 668, context 64, tool 222, skill 56,
#   session 89, steering 58, agent 215, server 406, attachment 15,
#   mcp 16, rag 38  → 1967 passing, 0 failing
cargo test -p test-support                   # 1 passing
cargo run --example <each of 8 examples>     # 8/8 exit 0
cd docs/examples/external-consumer && cargo run   # CONSUMER-PROOF: OK
synthia-server --directory <ws> --port 18080      # boots; /livez 200,
#   /readyz 200, /api/v1/agents 200, /api/v1/tools 200 (7 tools = 6
#   builtins + a remote MCP tool); logs show the MCP spawn and the
#   retrieval index build
```

A workspace-wide `cargo test --workspace` reports 1970 passing / 0
failing; the 2-test delta over the per-crate total comes from
Cargo's feature unification enabling two extra `cfg(test)` items.

### Boundaries (not claimed)

- No live LLM turn was executed in these rounds: the provider path
  is covered by the test suite (`FakeProvider`), the external
  consumer proof, and the server boot E2E — not by a paid API call.
- `synthia-web` was not changed; the multimodal and MCP/RAG
  surfaces are HTTP/API-level, not UI-level.
- `synthia-server` is the application crate and does name `tokio`
  types (axum/broadcast). The runtime-neutrality guarantee is
  stated for the 12 library crates, verified by a visibility-aware
  audit (0 leaks).
