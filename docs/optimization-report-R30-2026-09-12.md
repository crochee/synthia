# R30 Absorption Results — 2026-09-12

Plan: [`optimization-report-R30-2026-09-12-plan.md`](optimization-report-R30-2026-09-12-plan.md). Per-slice contracts lived in `local://r30-contracts.md` (ephemeral) — this file is the durable record of what landed, what was verified, and what is deferred.

## Source evidence (scouted 2026-09-12, absence-verified by grep over `crates/`)

| Candidate | Source | Slice |
|---|---|---|
| Retry taxonomy completion (`EMPTY_RESPONSE`, `QUOTA ≠ RATE_LIMIT`, stream-idle watchdog) | dsh `packages/llm/llm/src/error.ts`, `llm/src/retry-policy.ts`, `llm-deepseek/src/adapter.ts` | A |
| MCP reconnect supervisor + deterministic naming + generation re-sync + streamable-HTTP | dsh `packages/mcp/mcp-client/src/{connection,tools}.ts`; traitclaw `crates/traitclaw-mcp/src/multi_server.rs` | E |
| Usage-anchored token meter with projected next-request tokens | dsh `packages/llm/token-meter/src/{index,projection,usage-projection}.ts` | F |
| `#[derive(Tool)]` proc-macro | traitclaw `crates/traitclaw-macros/src/lib.rs` | G |
| Eval framework + session-log replay provider | traitclaw `crates/traitclaw-eval/src/*`; dsh `packages/test-support/llm-replay/src/index.ts` | H |
| User steering + follow-up queues; length-stop tool-call failure | pi `packages/agent/src/agent-loop.ts:167-265`, `:229-233` | B |
| Cache-stable runtime-context snapshots | dsh `packages/core/system-prompt/src/index.ts:77-239` | C |
| Verification gates; git-worktree isolation | pi-subagents `src/agent-manager.ts`, `src/worktree.ts` | D |

## What landed, per slice

### A — `synthia-provider`: retry taxonomy completion
- `RetryClass::{EmptyResponse, Quota}` + budgets. `EmptyResponse` = {2 attempts, 500ms start, 10s max interval}; `Quota` = single attempt (a dead balance never recovers in-session) while staying a *distinct* class so callers can report it. `classify_error` learns empty markers, quota/insufficient/billing markers (separator-normalizing matcher), and `402 → Quota`; `429 → RateLimit` unchanged.
- Empty terminal completions are now errors, not silent empty successes. Rule pinned at the **shared** seam: terminal stop ∈ {None, `stop`, `end_turn`, `stop_sequence`} with no text / no reasoning / no tool calls → `empty_response_error()`. `max_tokens` / `length` / `tool_use` with zero content stay `Ok` (they carry their own signal). `SamplingResult::into_completion_response` is the streaming finalize seam; `CompletionResponse::{is_empty_terminal_completion, ensure_non_empty}` the non-streaming one, checked **inside** `retry_with_classification` so the 2-attempt budget can actually fire.
- New `streaming::idle_watchdog`: `pump_sse` bounds **each outstanding read** (rearmer on every chunk), defaults to `DEFAULT_STREAM_IDLE_TIMEOUT = 120s`, expiry surfaces `Error::Timeout` with `IDLE_TIMEOUT_MARKER` (classified `Transient`). Both adapters' byte loops collapsed into the shared pump (cancellation drain grace unified).

### E — `synthia-mcp`: namespacing + supervisor + HTTP transport
- `public_tool_name(server, raw)` → `mcp__<server>__<raw>` normalized into `[A-Za-z0-9_-]` ≤ 64 chars; a lossy normalization appends a 12-hex SHA-256 of `(server, raw)` so distinct identities never collapse. Pure and stable across reconnects. `resolve_prefix` inverts it. `NamingPolicy::{Namespaced (default), Raw}`; `register_mcp_tools` returns an `McpToolGeneration` so a swap can unregister the previous set.
- `McpSupervisor` — scheduler-pattern clock injection (`tick(&registry, now) -> SupervisorTick` with `next_deadline`), doubling delays 1s→60s, **per-outage** attempt budget (default 5) reset once uptime exceeds the max delay; exhausted budget unregisters that server's tools (fail-visible). `report_failure` drives it from a call site; `notify_list_changed` performs an atomic generation swap — a fetch failure keeps the previous generation registered, a registration conflict rolls the new generation back *and restores the previous one* (never a partial surface).
- `StreamableHttpTransport` (`McpTransport` over JSON-RPC POST) + `HttpTransportFactory`; `StdioTransportFactory`/`InMemoryTransportFactory` give the supervisor a uniform `TransportFactory` seam. In-memory transport gained a kill-switch + scripted responses.
- Boot: `app_state` registers configured servers through the supervisor and exposes `AppState::mcp_health: Vec<ServerHealth>` (`Healthy` unit + 2 assertions in `boot_wiring_test.rs`).

### F — token meter: usage-anchored fold
- `token_meter::TokenMeter` folds `&[SessionEvent]` into `TokenMeasurement { log_revision, baseline, surface_delta_tokens, total_tokens, surface_tokens }` with disjoint `UsageBuckets { input, output, cache_read, cache_write }`. Anchor rule: adopt the provider's prompt-side total when it is plausible against the heuristic price of the surface the request saw; otherwise `MeasurementBaseline::Estimated`. Projection: `projected_tokens = max(0, pressure_tokens + surface_delta_since_sample)` — occupancy answers for the *next* request, including across compaction.
- `SessionEvent::Usage` gains `usage: Option<UsageBuckets>` (serde-default + skip-if-none) with `provider_usage()` reading either the typed field or a pre-R30 data payload → **old logs still parse and fold** (pinned by tests).
- `OperationSnapshot` gains optional `usage_buckets` + `context_pressure` (additive; the existing required `usage: TokenUsage` field is untouched).
- `CompactionSettings::should_compact_anchored(&TokenMeasurement, context_window, messages_since_last_compaction)` is the opt-in gate: it fires on provider pressure the char heuristic cannot see, stops firing after a compaction shrinks the surface, and falls back to the heuristic when no sample exists. The default `should_compact` path is unchanged.

### G — new crate `synthia-macros`
- `#[derive(Tool)]` on the args struct generates the full `impl synthia_tool::Tool`: `name()` (default: struct snake_case), `description()` (attribute, else struct doc comment, else **compile error** — fail-visible), `parameters()` via schemars' type-driven `SchemaGenerator` (value-driven infers no `required` and renders enum fields as `true` — unusable for tool schemas), `mode()` when `mode = "sequential"|"parallel"` is given, and `call()` that deserializes and returns `ToolOutput::error("Invalid arguments: {e}")` (model-facing) before delegating to the inherent `async fn execute(&self, &Context) -> ToolOutput`. `stream` / `truncate` / `output_definition` inherit the trait defaults.
- Diagnostics (never panics): non-struct, generic struct, unknown attribute key, invalid name characters, bad `mode`, missing description, missing `execute` (rustc's own method-resolution error at the struct span).
- Verified by 16 unit tests + a 14-test integration suite consuming the derive through a self dev-dependency + 1 doctest; the 6 misuse cases were additionally checked with a throwaway out-of-tree consumer crate (deleted after the run).

### H — new crate `synthia-eval` + `ReplayProvider`
- `EvalSuite` / `TestCase` (`expect_contains`, `expect_output`) → `EvalRunner` (`metric(Box<dyn AsyncMetric>)`, `threshold`): a case passes iff **all** metric scores ≥ threshold; with no metrics the fallback is keyword match over `expect_contains` (metric name `keyword_match`). `EvalReport` carries `average_score` / `passed` / `total` / chrono `generated_at`, `summary()`, pretty-JSON + CSV export (RFC4180 escaping, sorted metric rows, header-only for empty reports).
- Metrics: `KeywordMetric` (sync `Metric`), `SyncMetricAdapter`, `LlmJudgeMetric<P: JudgeProvider>` (local async judge seam; prompt = input + output + criteria; `parse_score` accepts `Score: X` or a bare number and clamps to `0..=1`; a judge error scores `0.0`), `SchemaValidationMetric` reusing `synthia_core::schema::validate_against_schema` (zero violations → 1.0).
- `synthia-test-support::ReplayProvider` implements `synthia_provider::ModelProvider`: per-call cursor over chunk scripts or a session `events.jsonl` fixture (`from_events_jsonl` splits on `finish_reason`, rejects malformed lines and unterminated tails), optional per-chunk pacing, cancellation observed between chunks, and `assert_consumed() -> Result<(), ReplayError>` reporting unbound scripts + per-call undelivered chunk counts.

### B — `synthia-agent`: interactive seams + length-stop guard
- `RunInbox` (async trait, `take_steering` / `take_follow_up`, both default-empty) + `MpscInbox` / `RunInboxHandle` over `futures::channel::mpsc`; `ReActAgent::with_run_inbox` + `AgentBuilder::run_inbox`. Default `None` is byte-identical to pre-R30 (pinned by an event-stream equality test).
- Seams: steering drained at iteration start, again between the tool-result commit and the next sample, and again after any context re-budget (compaction) so nothing typed during a long prepare is lost; each drain appends trailing user turns and emits `SystemEvent::SteeringInjected { source, count }`. At a would-be stop, `take_follow_up` is polled — non-empty revives the run (with a re-budget before the revived sample, preserving the "every sample sees a freshly budgeted list" invariant).
- Length-stop guard: `stop_reason ∈ {"length", "max_tokens"}` (case-insensitive) **with tool calls** fails the whole batch without executing anything; each call receives a model-facing error `ToolResult` ("…hit the output token limit, so its arguments may be truncated. Re-issue the tool call with complete arguments.") and the loop continues. Falsification check: temporarily stubbing the predicate to `false` made the test fail with `left: 1` (the tool executed) before the guard was restored.

### C — cache-stable runtime-context snapshots
- Volatile environment facts (cwd / time / git status) leave the system prompt; `prompt::RuntimeContext` renders a user-role snapshot framed *"Current runtime context. This snapshot supersedes earlier runtime-context snapshots."* with a chrono clock injected (`ReActAgent::with_clock` for tests).
- The snapshot is appended **only when the rendered body changed** since the last append, so an unchanged turn adds zero tokens and the provider prefix stays cacheable — this is the assembly half of the cache policy (`cache_policy.rs` breakpoints + R29 `replay_state`). Position: after the steering drain, immediately before the provider sample.

### D — verification gate + worktree isolation
- `TaskSpec` gains `gate: Option<GateSpec>` (argv + timeout) and an `isolation` marker. `agent::gate` runs the gate after the child through an injectable `CommandRunner` (default `StdCommandRunner` = `std::process::Command`, runtime-neutral); exactly one verdict per run; a killed gate is reported as killed (never read as passing from an exit-0-looking status); failure becomes the parent-visible error `ToolOutput` carrying stdout/stderr.
- `agent::worktree` creates a temp git worktree (`synthia/agent-<ulid>` branch) via the same runner; the child's tree is inspected post-run: clean → `git worktree remove`, dirty → commit to the named branch and report `{branch, path, base_sha}` appended to the child's result text. The `worktree_isolation` feature name `synthia-tool/schema_builder` advertised since R29 is now backed by a real implementation.
- `task_tool_definition()` exposes the new optional fields so the model-facing schema matches the runtime.

## Cross-slice coordination
- **B → F**: the loop emits `synthia_session::usage(...)` from `sample_once` (the only producer call site) so the durable fold has typed buckets.
- **B → C → D**: three waves over `re_act.rs`, sequenced to avoid concurrent edits to one file; each slice's tests stayed green on landing.
- **E**: touched `synthia-server`'s boot block + one integration assertion that pinned the pre-namespacing raw tool name (updated to the namespaced name; no sibling slice owned the file).

## Verification (final state)
- `cargo +nightly fmt --all --check` — clean.
- `make lint-rust` (`cargo clippy --all-targets --all-features --tests --all -- -D warnings`) — zero warnings.
- Per-crate test totals (17 crates, `grep -c 'FAILED\|^error'` = 0 everywhere): core 84, telemetry 36, provider 711, context 73, tool 247, skill 56, session 123, steering 58, agent 270 (incl. integration + doctest), server 418, attachment 15, mcp 52, rag 38, scheduler 14, test-support 18, macros 31, eval 36 → **2280 passing / 0 failing** (R29 was 2056).
- `cd docs/examples/external-consumer && cargo run` → `CONSUMER-PROOF: OK`.
- README primitive table gained rows 33–40; inventory updated to 17 members; CHANGELOG has the R30 entry.

## Deferred to R31 (recorded, not dropped)
Same list as the plan's §Deferred: scripted workflow runtime + journal prefix-replay, OS sandbox (Landlock/bwrap), SQLite sink + FTS5, grouped/dynamic tool registry, file-backed scoped memory, two-pool subagent concurrency, progressive example ladder.
