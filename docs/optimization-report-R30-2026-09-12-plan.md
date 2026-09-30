# R30 Absorption Plan — 2026-09-12

Standing objective (per goal): absorb verified-absent designs from `~/workspace/traitclaw/`, `~/workspace/deepseek-harness/`, `~/workspace/pi/`, `~/workspace/pi-subagents/` so the synthia workspace stays at the leading edge of AI-agent design, runtime-neutral, and remains a Lego-style lib + assembly tutorial.

After R29 (commits `8f1821cf` + `aae5cb37`, head `c6875adb`) the agent manifold closed: steering / context / session / agent / server / attachment / MCP / RAG / scheduler / operation endpoint / chunk log / mention-clone / session lifecycle / compaction events all exist. Three scout passes re-audited the four reference repos on 2026-09-12 and surfaced **22 absence-verified candidates** (see `local://r30-contracts.md` for the per-slice contracts and the file:line evidence).

R30 ships eight of them as **two coherent waves**:

## Wave 1 — file-disjoint, parallel
- **A. Provider**: retry taxonomy completion (dsh llm-retry §error.ts) — `RetryClass::{EmptyResponse, Quota}` + budgets; typed empty-completion error at the shared parse seam (Anthropic + OpenAI); per-read stream-idle watchdog with shared `pump_sse`.
- **E. MCP**: deterministic `mcp__<server>__<raw>` namespacing with sha-suffix on lossy normalization (dsh mcp-client); `McpSupervisor` (tick(now) clock injection, per-outage budget, generation-swap resync on `tools/list_changed`); `StreamableHttpTransport`; per-server health in `AppState::mcp_health` (dsh + traitclaw multi_server).
- **F. Meter**: usage-anchored `TokenMeter` fold over `PersistedEvent` with disjoint usage buckets and projected next-request tokens (`max(0, anchor + signed surface_delta)`); persist provider usage with serde-default back-compat; opt-in `CompactionSettings::should_compact_anchored`; `OperationSnapshot::usage_buckets + context_pressure` (dsh token-meter).
- **G. Macros**: new crate `synthia-macros` with `#[derive(Tool)]` + `#[tool(name, description, mode)]` attribute; generates full `impl synthia_tool::Tool` delegating to inherent `async fn execute(&self, &Context) -> ToolOutput`; schemars type-driven `parameters()` (traitclaw-macros).
- **H. Eval**: new crate `synthia-eval` (`EvalSuite` / `TestCase` / `Metric` (sync) + `AsyncMetric` / `EvalRunner`, built-ins `KeywordMetric`, `LlmJudgeMetric` with local `JudgeProvider` seam + clamped score parse, `SchemaValidationMetric` reusing `synthia_core::schema::validate_against_schema`, JSON+CSV export); `ReplayProvider` in `synthia-test-support` implementing `synthia_provider::ModelProvider` from chunk scripts or session events.jsonl with `assert_consumed` (traitclaw-eval + dsh llm-replay).
- **B. Loop**: `RunInbox` trait + `MpscInbox` (futures mpsc, runtime-neutral, default empty) on `ReActAgent` + `AgentBuilder`; steering drained at iteration start + between tool commit and next sample + after compaction prep; follow-up revives a stopping run; `SystemEvent::SteeringInjected` + `SteeringSource::{Steering, FollowUp}` (pi agent-loop parity); **length-stop guard** — `stop_reason ∈ {length, max_tokens}` with tool calls fails the whole batch via model-facing error `ToolResult`s (pi `failToolCallsFromTruncatedMessage`).

## Wave 2 — sequential on `synthia-agent` (re_act.rs region-sharing)
- **C. Prompt**: stable system prompt — volatile `<env>` facts move out of the system message into a `prompt::RuntimeContext` snapshot seam. Rendered user-role snapshot is appended **only when the rendered body changed since last append**; chrono clock injected; framed `Current runtime context. This snapshot supersedes earlier runtime-context snapshots.` (dsh `renderContextSnapshot` §`PromptContext`). Ordering: steering drain (B) → snapshot append → provider sample.
- **D. Delegation**: `TaskSpec` gains `gate: Option<GateSpec>` and `isolation` flag. New `agent::gate` runs verification commands after the child via injectable `CommandRunner`; exactly-once verdict; killed gates report killed explicitly. New `agent::worktree` creates temp `synthia/agent-<ulid>` git worktrees via the same runner; post-child: clean → remove, dirty → commit + report `{branch, path, base_sha}`. `task_tool_definition()` schema gains the new optional fields — the `worktree_isolation` feature name `synthia-tool/schema_builder` R29 already advertises becomes real (pi-subagents agent-manager + worktree).

## Cross-slice coupling
- **B ↔ F**: LoopAgent emits `synthia_session::usage(...)` in `sample_once` so the durable fold has the bucket data; MeterAgent reads it.
- **B → C → D**: each owns a different region of `re_act.rs` and is sequenced accordingly.
- **A + E**: both touch `synthia-server/src/state/app_state.rs` (A — otel patch in a stream module that the boot path also re-exports; E — boot registration). Distinct hunks, verified non-conflicting.

## Constraints (from `local://r30-contracts.md`)
Rust 2024 / MSRV 1.95 / `max_width = 80`; `thiserror` in libs / `anyhow` in apps; deps via `[workspace.dependencies]` (major.minor); no wildcard imports / `for_each` / `map_or` / `map_or_else` / `std::mem::forget` / `std::ptr::read_unaligned`; chrono for timestamps; runtime-neutral public APIs; per-crate tests only (`cargo test -p <crate>`); file ownership strictly disjoint unless explicitly declared (and the seam in the other file is documented).

## Verification
After all eight slices land:
1. `cargo +nightly fmt --all --check` — exit 0.
2. `make lint-rust` (= `cargo clippy --all-targets --all-features --tests --all -- -D warnings`) — zero warnings.
3. `cargo test -p <each of the 17 crates>` — per-crate green.
4. `cd docs/examples/external-consumer && cargo run` — `CONSUMER-PROOF: OK`.
5. Update `CHANGELOG.md`, `README.md` primitive table rows 33–40, write `docs/optimization-report-R30-2026-09-12.md` (results + verification evidence), commit.

## Deferred (R31 candidates — recorded, not dropped)
- **SubagentWorkflow scripting + journal prefix-replay** (pi-subagents §1) — scripted orchestration layer with caps/semaphore/journal. The script-VM part is JS-shaped; the Rust port is a declarative `WorkflowSpec` + caps + journal + gate orchestration as a new `synthia-workflow` crate. Sized ~1500 LOC, deserves its own focused round.
- **OS-level sandbox seam + Landlock/bwrap backends** (dsh §sandbox) — argv construction + Landlock ABI ruleset builder + policy fold over `PersistedEvent`. New crate `synthia-sandbox`. Kernel-ABI availability makes WSL2 the gating factor for tests.
- **SQLite durable sink + FTS5** (traitclaw memory-sqlite parity) — `SqliteSessionSink` over rusqlite. New dep; build cost.
- **GroupedRegistry / dynamic tool groups** (traitclaw registries) — operator-chosen focus sets where `get_tools()` returns active groups and `find_tool()` searches ALL groups.
- **File-backed scoped long-term memory** (pi-subagents memory) — `MemoryFile` impl of `synthia_context::Memory` long-term tier.
- **Two-pool subagent concurrency manager** (pi-subagents agent-manager) — background 10-queued / foreground unbounded / nested no-slot, tombstones, invocation records.
- **Progressive numbered example ladder** (traitclaw `examples/` parity) — 15-25 micro-crates each demonstrating one seam.