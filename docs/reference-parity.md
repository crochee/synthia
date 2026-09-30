# Reference parity — traitclaw & pi

The objective this repo is built against names two reference projects
(`~/workspace/traitclaw`, `~/workspace/pi`): absorb their good design and
logic, and consciously decline the rest. This file is the **checkable**
record of where that stands: every capability either maps to a synthia
symbol (verified, see "How this was checked") or is listed as declined
with the reason, or is an open gap with a plan.

The absorption *history* — what each round took, why, and which reference
bugs were fixed on the way — is
[`traitclaw-gap-analysis.md`](traitclaw-gap-analysis.md) (Chinese). This
file is the current-state map.

## traitclaw (14 crates)

| Reference | Capability | synthia counterpart |
|---|---|---|
| `traitclaw-core/agent.rs` | the ReAct loop | `synthia_agent::ReActAgent` + `synthia_agent::ReasoningStrategy` |
| `traitclaw-core/agent_builder.rs`, `factory.rs` | assembly | `synthia_agent::ReActAgent` + its `with_*` chain (R110 folded the separate `AgentBuilder` into it — one builder, not two) |
| `traitclaw-core/context_managers.rs` | window strategies | `TruncatingContextManager`, `SummarizingContextManager`, `DagContextManager` (synthia-context) |
| `traitclaw-core/memory/*` | durable tiers | `FileMemory`, `SqliteMemory` (feature `sqlite`) |
| `traitclaw-steering/*` | guards / hooks / hints / trackers / transformers | `synthia_steering::{Steering, Guard, AgentHook, Hint, Tracker, OutputTransformer}` |
| `traitclaw-strategies/{react,cot}` | reasoning paradigms | `ReActStrategy`, `ChainOfThoughtStrategy` |
| `traitclaw-strategies/mcts` | tree search | **declined as a loop** (synthia-workflow's `Step::BestOf` + R58's `BestOfNStrategy` cover candidate selection declaratively; a full MCTS search is not worth its token cost for a coding agent) |
| `traitclaw-team/conditional_router` | routing | `ConditionalRouter` (synthia-tool-task). The `group_chat` / `execution` halves were **declined**: verify-then-retry is a `CandidateScorer` behind `BestOfNStrategy` (or a `synthia-workflow` gate), and a `prompt -> text` round-robin chat has no caller in a harness whose delegation is the `task` tool — R122 |
| `traitclaw-mcp/{server,multi_server,tool}` | MCP | `McpClient`, `McpSupervisor`, `McpTool` |
| `traitclaw-rag/*` | chunk → embed → hybrid → ground | `RecursiveChunker`/`SentenceChunker`/`FixedSizeChunker`, `EmbeddingRetriever`, `KeywordRetriever` (BM25), `HybridRetriever`, `RagContextManager` |
| `traitclaw-memory-sqlite` | FTS5 memory | `SqliteMemory` (feature `sqlite`), `make test-sqlite` |
| `traitclaw-eval` | suites, metrics, judge, report | `synthia_eval::{EvalRunner, Suite, Metric}` |
| `traitclaw-test-utils` | deterministic fakes | `synthia_test_support::{FakeProvider, FakeTool, ReplayProvider}` |
| `traitclaw-macros` | tool derive | `synthia_macros` `#[derive(Tool)]` |
| `traitclaw-{anthropic,openai,openai-compat}` | adapters | `synthia-provider` features `anthropic` / `openai` |
| `traitclaw` (facade) | one dependency | `synthia` facade + `prelude` |
| `showcase/miniclaw` | end-to-end demo | `docs/examples/{minimal,external}-consumer` + 38 examples |

## pi (11 packages)

| Reference | Capability | synthia counterpart |
|---|---|---|
| `packages/agent` (harness) | the loop + tool dispatch | `synthia_agent` (loop, dispatch, steering seams) |
| `packages/agent` (lcm/DAG context) | hierarchical condensation | `DagContextManager` |
| `packages/agent` (compaction) | LLM summarisation on overflow | `SummarizingContextManager` + `CompactionSettings` + `CompactionCheckpoint` |
| `packages/ai` (api/auth/compat/providers) | provider abstraction | `synthia_provider::{ModelProvider, AnthropicProvider, OpenAICompatibleProvider}` |
| `packages/server`, `packages/client` | RPC session attach/detach + service calls (chord framework, not HTTP routes) | `synthia-server` — the HTTP/SSE surface. Management routes under `/api/v1` (`GET /models`, `GET/POST /sessions`, `GET /sessions/{id}`, `GET /sessions/{id}/events`, `GET /sessions/{id}/status`, **`GET /sessions/search`**, `GET /memory/search`, `GET/POST /skills`, `POST /skills/reload`, `GET/DELETE /skills/{name}`, `GET/POST /agents`, `GET /agents/default`, `GET/DELETE /agents/{name}`, `GET/POST /tools`, `GET/DELETE /tools/{name}`, `POST /attachments`, `GET/DELETE /attachments/{hash}`, and the `/chat/*` family: `GET /chat/usage`, `POST /chat/sessions`, `POST /chat/sessions/{id}/messages`, `GET /chat/sessions/{id}/messages/stream` (SSE), `POST /chat/sessions/{id}/cancel`, `POST /chat/sessions/{id}/regenerate`, `PATCH /chat/sessions/{id}/messages/{message_id}`, `POST /chat/messages/{message_id}/feedback`, opt-in `POST /chat/sessions/{id}/operation`). Probes outside the auth layer: `GET /livez`, `GET /readyz`, `GET /metrics`. The generated union table (frontend × backend) is `docs/interface-contract/contract.md` |
| `packages/evals` | eval harness | `synthia-eval` |
| `packages/telemetry` | tracing / metrics | `synthia-telemetry` (console always; `metrics` / `otlp` features) |
| `packages/session-backends` | durable transcripts | `synthia-session` JSONL sink + `fold_log_surface` |
| `packages/agent` (`search`) | **session search service** | `synthia_session::{SessionSearch, JsonlSessionSearch, SearchQuery}` — runtime-neutral seam + JSONL-backed index with snippets, tombstones, and `sync()` (R63, closed; the only gap the R62 parity audit found) |
| *(traitclaw `traitclaw-mcp`)* | MCP `tools/call` content blocks | `synthia-mcp`. **Beyond the reference on multimodal**: traitclaw's `ToolCallContent` is `kind` + `text` only and its `ErasedTool` returns a `Value`, so an MCP `image`/`audio` block is structurally unreachable there. Synthia's `Tool` returns a `ToolOutput` of `ContentPart`s, and `McpCallResult::parts()` (R71) projects `image`/`audio` blocks onto `ContentPart::{Image, Audio}` so the payload reaches the provider. Namespacing (`mcp__<server>__<raw>` + normalization + hash suffix) replaces traitclaw's `{server}::{tool}` prefix; per-server health and bounded-backoff reconnect are new (`McpSupervisor`, no timers — the caller drives `tick(now)`) |
| `packages/coding-agent` (258 files: modes, extensions, bun launcher) | the shipped coding agent | declined as a *crate*: the app shape lives in `synthia-server` + examples; its tool set (`read`/`write`/`shell`/`todo`/`web`) is one plugin crate per tool — `synthia-tool-read` / `-write` / `-shell` / `-todo` / `-web` — over the `synthia-tool` paradigm |
| `packages/tui` | terminal UI | declined: this repo's UI is the web frontend (`synthia-web`); a TUI is an app, not a library seam |
| `packages/protocol` (cbor) | binary wire | declined: JSON on the wire (readable, debuggable, no schema codegen); `serde` keeps the door open for a CBOR transport |
| `packages/chord` (facets/delta/bundler) | collaborative document layers | declined: browser-oriented service bundling; the context-layer need it serves is met by `DagContextManager` |

## Declines worth restating (the interface boundaries)

- **A TUI and a bundled coding agent**: both are applications. This repo's
  promise is the *library*; `synthia-server` + `synthia-web` demonstrate
  the app shape without making it mandatory.
- **A full MCTS loop**: token-expensive, and the useful half (sample N,
  score, keep the best) is implemented (`BestOfNStrategy`, `Step::BestOf`).
- **CBOR protocol**: JSON's cost is real but so is its debuggability; a
  transport swap is a plugin, not a rewrite.
- **Runtime seams for the tool implementations**: declined on purpose
  (R40, sharpened by the plugin split) — each tool is its own
  tokio-bound plugin crate, so a consumer on another runtime simply
  does not depend on it; documented in `synthia_core::spawn`
  and `SEAMS.md` §2. What the framework owns is that no *public API* of a
  library crate names a runtime type — now enforced by
  `make check-public-api-runtime` (R62).

## How this was checked

The mappings above are not prose: a script resolves each counterpart
symbol against `crates/*/src/**/*.rs`. Every row resolves — the one
miss (`SessionSearch`, flagged by R62) was closed by R63. Re-run it any time with:

# every counterpart name must appear in a library crate's source
grep -rn "ReActAgent\|with_strategy\|TruncatingContextManager\|…" crates/*/src
```

The claims that *can* be mechanised already are, and they run in CI:

| Gate | What it proves |
|---|---|
| `make check-mvp-deps` | the seven-feature subset pulls no HTTP client, exporter, framework, or database |
| `make check-no-runtime` | six crates plus `synthia-telemetry --no-default-features` are tokio-free |
| `make check-public-api-runtime` | no public API of a library crate names a runtime type (R62) |
| `make doc-check` | no rustdoc warning anywhere (R56) |
| `make ci` | fmt, clippy, the three invariants above, the clock ratchet, and now the runtime-API gate |
| `cargo test -p synthia --doc` (default and MVP) | `MINIMAL.md` — the tutorial — compiles (R61) |
