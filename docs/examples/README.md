# Examples — the executable half of the manual

Every example here is a small `fn main()` program that shows **one
seam** of the framework and ends by printing a proof line. They run
offline: no network, no API key, no environment variables. Where a
seam needs an external binary (bubblewrap, git) the example detects
it and prints a `SKIPPED` line instead of failing.

Run any of them from the repository root:

```bash
cargo run -p <crate> --example <name> [--features sqlite]
```

Two things are worth knowing before reading the code:

- **Libraries are runtime-neutral.** Every `synthia-*` library takes
  `Arc<dyn CancelToken>` from `synthia-core` (std-only), drives its
  I/O through `futures` streams, and leaves HTTP to the application
  crate. The examples use `#[tokio::main]` only because a `main` needs
  *an* executor; the seams they demonstrate do not.
- **Time is chrono.** Wall-clock values are `chrono::DateTime<Utc>`;
  `std::time::Instant` is used only for monotonic durations.

## 1. Assemble an agent

| Example | Crate | Shows | Look at |
|---|---|---|---|
| `assemble_from_scratch` | `synthia-harness` | The seven lego pieces wired by hand (provider stub, default tool registry, `Steering`, truncating context manager, typed sink, std-only cancel token, `ReActAgent`) | the live event stream, then the typed structural events |
| `assemble_with_builder` | `synthia-harness` | The same agent in one `ReActAgent` chain: tier steering, `.with_output_schema` auto-tool, LLM compaction, typed sink, `@agent` router | how small the happy path is |
| `assemble_with_provider_profile` | `synthia-harness` | `ProviderProfile` as typed configuration instead of loose setters | the profile → provider translation |
| `assemble_with_skills` | `synthia-harness` | `SkillRegistry`: procedural skills bound to tools | skill lookup during assembly |
| `assemble_with_tier_steering` | `synthia-harness` | `ModelTier` → `Steering::for_tier` + `AdaptiveRegistry` tool cap | the visible tool list shrinking on `Small` |
| `strategy_swap` | `synthia-harness` | One runtime, **three** reasoning loops through the same `ReActAgent`: `ReActStrategy` (tools offered, one tool call), `ChainOfThoughtStrategy` (no tools, step-by-step prompt), `BestOfNStrategy` (three concurrent samples scored by a caller-supplied `CandidateScorer`, winner published) | `STRATEGY-SWAP: OK`, and the request logs — ReAct with tools, Chain-of-Thought without, best-of-N with three unoffered-tool samples and the scorer's pick |
| `argument_validation` | `synthia-tool` | The R74 opt-in: same `EchoTool`, two runs (off / on), the off path counts 2 tool bodies (default unchanged-by-R74 contract), the on path counts 1 (the malformed call is rejected at the registry with `is_error: true` and the dotted-path violation in the body) | `ARGUMENT-VALIDATION: OK`, and the tool's call counter (2 vs 1) |
| `best_of_n_judge` | `synthia-harness` | The same `BestOfNStrategy`, with `LlmJudgeScorer` (an `Arc<dyn ModelProvider>` + rubric) replacing the marker-grep scorer: three candidates, one judge call per candidate, the highest-scoring candidate wins | `BEST-OF-N-JUDGE: OK`, plus the per-candidate progress events (`scored 0.300 / 0.950 / 0.700 via llm-judge`) and the selection event (`selected candidate 0 (score 0.950)`) |
| `external-consumer` | `docs/examples/` | A **standalone crate** (not a workspace member) depending on synthia by relative path — the real "use it as a library" test | `CONSUMER-PROOF: OK` |
| `minimal-consumer` | `docs/examples/` | The **smallest** complete agent: one dependency (`synthia`, `default-features = false`) and the seven-feature MVP subset from [`MINIMAL.md`](../../MINIMAL.md) — a scripted provider that asks for a tool, one hand-written `Tool`, and assertions that it really ran. Its tree carries no HTTP client at all | `MVP-OK`, and `make check-mvp-deps` for the dependency promise |
| `assemble_from_zero` | `synthia` (facade) | The same assembly through **one dependency**: `use synthia::prelude::*` and nothing else | how few names a consumer actually needs; the crate docs are the longer version |

## 2. Tools and the wire

| Example | Crate | Shows | Look at |
|---|---|---|---|
| `tool_groups` | `synthia-tool` | `GroupedRegistry`: named groups with independent activation | a tool from a **deactivated** group still executes through `inner()` — visibility ≠ executability |
| `read_ranges` | `synthia-tool-read` | The `read` plugin brick: workspace file reader with 1-based `offset` / `limit` line ranges | `READ-OK`, and the line-numbered output |
| `write_modes` | `synthia-tool-write` | The `write` plugin brick: overwrite vs append, confined to the workspace | `WRITE-OK`, and the file body after both calls |
| `todo_list` | `synthia-tool-todo` | The `TodoWrite` plugin brick: the structured task list the model maintains | `TODO-OK`, and the rendered checklist |
| `allowlist` | `synthia-tool-web` | The `web_fetch` plugin brick, offline: its schema plus `is_url_allowed` for an allowed vs blocked host | `WEB-OK` (no network access) |
| `deferred_tools` | `synthia-tool` | `ToolExposure::Deferred`: a tool is advertised as name + description until the transcript shows it was called, then promoted to its full schema | the model-facing payload shrinking before the call and growing after; the tool still dispatches in both states |
| `tool_surface_policy` | `synthia-harness` | `ToolSurfacePolicy`: groups with an active set, a `max_visible` cap, and deferred advertisement — the deployment-facing half of the surface | the same registry projected three ways, and a tool the policy removes from the catalog still executing |
| `derive_tool` | `synthia-macros` | `#[derive(Tool)]`: a struct + inherent `execute` becomes a full `Tool` | the generated JSON Schema, and the model-facing `Invalid arguments` error |
| `output_transformers` | `synthia-harness` | The commit-path transformer + retrieval round trip: `JsonExtractor` → `BudgetAwareTruncator` stashes the full text, `__get_full_output` reads it back | the truncation marker naming a handle, then the tool returning the payload |
| `sandbox_policy` | `synthia-tool-shell` | `ExecutionPolicy` + `BwrapBackend` (the plugin's own OS execution policy) as consumed by `ShellTool` | the fail-closed refusal with no backend, and the real `[sandbox: file access denied …]` marker |
| `schedule_jobs` | `synthia-tool-scheduler` | The `schedule` tool driven through `Tool::call` (`list` / `create` / `pause` / `resume` / `remove`) over the **same** `Arc<ScheduleStore>` the host's `Scheduler` ticks | `SCHEDULE-TOOL: OK`, the job the tool created being the one the host tick delivers, a paused job not firing, and `kind: "cron"` being refused on a default build because the scheduler crate cannot compute a recurrence (with `--features cron` the same call schedules the expression's own first fire) |
| `search_tool_demo` | `synthia-tool-search` | The cross-domain `search` tool over one two-domain `synthia-search::Registry`: `register_search_tool` registers it `Deferred`, then one query fans out to both the skill and the memory engine | `SEARCH-TOOL: OK`, the advertised schema starting as the permissive placeholder and growing to the tool's own `query` / `limit` schema, and hits from both engines in one answer |
| `tool_arg_repair` | `synthia-provider` | Wire hygiene: `repair_json` + `ToolArgsQuality` salvage malformed tool-call arguments and report how; `parse_provider_error_body` + `classify_provider_error_body` classify a failed response from its body | a raw control character and an invalid escape recovered as `Repaired`, a truncated tail reported as `RawFallback`, and 429 quota vs 429 throttling landing in different classes |

## 3. Agents in teams

| Example | Crate | Shows | Look at |
|---|---|---|---|
| `fan_out_with_group_join` | `synthia-tool-task` | `GroupJoin`: batch background-agent completions into one notification | one delivery instead of one interrupt per child |
| `delegation_gate` | `synthia-tool-task` | `gate`: a verification command that must pass for a delegated child's work to count | the pass path, then `failed gate …` with the command's output |
| `worktree_isolation` | `synthia-tool-task` | `worktree`: a temp git worktree on a reserved `synthia/agent-<ulid>` branch, kept when the child left changes | the branch and base SHA reported back |
| `workflow_fanout` | `synthia-workflow` | A declarative `WorkflowSpec` (fan-out + gated call) executed through a fake `WorkflowHost`, then replayed from the JSONL journal | statuses per call; the second run spawning nothing |
| `workflow_best_of` | `synthia-workflow` | `Step::BestOf`: run N candidate attempts, keep the first whose gate passes | the winner's text as the step output, the losers recorded as `Superseded` without failing the run, and the replay run spawning nothing |

## 4. Interactivity and prompt hygiene

| Example | Crate | Shows | Look at |
|---|---|---|---|
| `run_inbox_steering` | `synthia-harness` | `RunInbox` / `RunInboxHandle`: messages typed mid-run become steering, follow-ups revive a stopping run | `SteeringInjected { source: Steering }` then `{ source: FollowUp }` |
| `runtime_context` | `synthia-harness` | `prompt::RuntimeContext`: volatile facts render as a user-role snapshot, appended only when it changed | the snapshot being byte-identical for the same clock — the provider prefix stays cacheable |
| `token_meter` | `synthia-harness` | `TokenMeter` folding session events into a usage-anchored measurement, plus anchored compaction gating | `projected_tokens` rising with the surface, then falling after compaction |

## 5. Memory and remote tools

| Example | Crate | Shows | Look at |
|---|---|---|---|
| `sqlite_memory` | `synthia-context` (feature `sqlite`) | `SqliteMemory`: one file holding long-term (FTS5/BM25 recall), durable working memory, and the session registry | BM25 ordering; the read-only refusal |
| `register_remote_tools` | `synthia-mcp` | Driving an MCP server over stdio and publishing its tools into the local registry | remote tools appearing as ordinary `Tool`s |
| `mcp_control_tool` | `synthia-mcp` | The other model-facing path: ONE `mcp` tool (`servers` / `tools` / `call`) over a live supervisor — a real `sh` MCP server is spawned, its catalog read at run time, and a remote tool called by its server-side name | `MCP-CONTROL-TOOL: OK`, the argument schema the model is handed, a remote `isError` arriving as a correctable error, and an unknown server name not reaching the wire |

## 6. Measuring and replaying

| Example | Crate | Shows | Look at |
|---|---|---|---|
| `eval_suite` | `synthia-eval` | `EvalSuite` + metrics + pass/fail thresholds, with JSON **and** CSV export | a passing and a failing case side by side |
| `replay_provider` | `synthia-test-support` | `ReplayProvider`: deterministic `ModelProvider` over a chunk script or a session log, with `assert_consumed()` | the completeness check at teardown |

## 7. Durability

| Example | Crate | Shows | Look at |
|---|---|---|---|
| `compaction_checkpoint` | `synthia-session` | A compaction that survives a resume: the checkpoint's span is resolved through the **log**, not through the manager's in-memory indices | the written `surface_op` citing the shadowed event's seq, the folded surface before vs after (replaced, not replayed), and an unprovable record being dropped while the log stays foldable |

## Reading order suggestion

Start with `assemble_with_builder` (what a normal consumer writes),
then `tool_groups` and `strategy_swap` (the two most useful extension
points: what the model may call, and how it reasons), then
`workflow_fanout` and `workflow_best_of` (how to make a
multi-agent run resumable, and how to pick among attempts).
`sandbox_policy` and `delegation_gate` are the two examples to read
before putting an agent in front of a real filesystem, and
`tool_arg_repair` is the one to read before trusting a provider's
streamed arguments or its error text.

## Verification

Every example in this table runs in CI's example job (R32 verification
records the observed proof line for each). An example that needs a
capability the host lacks prints `SKIPPED` and exits 0 — it never
asserts behaviour the host cannot produce.
