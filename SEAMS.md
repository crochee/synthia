# The seam map

Synthia is assembled from **traits**, not from a framework object. This
page is the index: every point where a consumer can substitute their own
implementation, what the workspace already ships for it, and the line of
code that installs a replacement.

Read it as a menu. Nothing here is mandatory: an agent needs the seven
pieces in §1 and nothing else, and every piece in §2–§4 is opt-in
(§3 pieces are separate crates, feature-gated through the `synthia`
facade — see [`MINIMAL.md`](MINIMAL.md)).

## 1. The seven pieces of the loop

| Seam | Trait | Ships in-tree | Install yours |
|---|---|---|---|
| the model | `ModelProvider` | `AnthropicProvider`, `OpenAICompatibleProvider` (feature-gated), `ModelProviderStub` (test-support) | `ReActAgent::new(provider, registry)` (the harness is the builder) |
| the tools | `Tool` | plugin crates, one tool each: `synthia-tool-read` / `-write` / `-shell` / `-todo` / `-web` / `-task` / `-scheduler` / `-search` (`synthia-tool` is the paradigm: registry + trait + `workspace` path confinement; a boundary beyond a path, like the shell's process policy or the search tool's `Registry`, belongs to the plugin that enforces it) | `registry.register_entry(ToolEntry::new(Arc::new(my_tool)))`; derive it with `#[derive(Tool)]` from `synthia-macros` |
| the context window | `ContextManager` | `TruncatingContextManager`, `NoopContextManager`, `SummarizingContextManager`, `DagContextManager` | `.with_context_manager(Arc<dyn ContextManager>)` |
| the policy layer | `Steering` (a container of `Guard`, `AgentHook`, `Hint`, `Tracker`, `OutputTransformer`) | `Steering::noop()`, `Steering::default_policy(root)`, `tier::auto(provider.tier(), root)` | `.with_steering(Arc::new(Steering::noop().add_guard(g).add_hook(h).add_hint(i).with_tracker(t).with_output_transformer(x)))` |
| the durable log | `SessionSink` | `JsonlSessionSink`, `InMemorySessionSink`, plus `ReplayProvider` for reading one back | the sink is installed where the run is owned (the server's controller, your own run factory) and fed through `TypedEventSink`; `.with_typed_event_sink(sink)` wires the agent's side |
| cancellation | `CancelToken` | `AtomicCancelToken` (std-only); a tokio `CancellationToken` satisfies the trait with no adapter | `agent.run(input, cancel)` — an `Arc<dyn CancelToken>` |
| **what one agent may call** | `ToolRestriction` (allow / deny) | none — the deployment's `[tools]` surface is a separate, coarser layer | `.with_tool_restriction(ToolRestriction::deny(["shell"]))` (or `agents.<name>.denied_tools`); enforced at both ends — excluded tools are not advertised *and* a call is refused with a model-facing error |
| **the reasoning loop** | `ReasoningStrategy` | `ReActStrategy` (think → act → observe, the default), `ChainOfThoughtStrategy` (one step-by-step completion, no tools), `BestOfNStrategy` (N concurrent samples, scored, winner published — plug a `CandidateScorer`) | `.with_strategy(Arc::new(MyStrategy))` — the strategy receives an `AgentRuntime` (provider, tools, steering, context manager, clock, cancel token, executor) and publishes through an `EventSink`. Reachable **from config** too: `agents.<name>.strategy = "chain-of-thought"` resolves once at boot via `strategy::from_name` (unknown name → warn + ReAct). `cargo run --example strategy_swap -p synthia-harness` runs all three against one runtime |
| the loop's outer contract | `Agent` | `ReActAgent` (which delegates to its `ReasoningStrategy`) | implement `Agent` when you need a different *session* contract rather than a different loop; register it in `AgentRegistry` for routing |

## 2. Runtime and clock

| Seam | Trait | Ships in-tree | Install yours |
|---|---|---|---|
| detached work | `Spawner` | a tokio-backed default inside `synthia-harness` / `synthia-tool` | .with_spawner(Arc::new(my_spawner))`, `ToolRegistry::with_spawner(..)` |
| wall clock | `Clock` | `SystemClock`, `FixedClock` (tests) | `SharedClock` handles threaded into the component; public "stamp now" constructors have an `_at(..., now)` sibling |
| ids | `IdGen` | `UlidGenerator`, `SequenceGenerator` (tests) | `SharedIdGen` handles, same pattern |

The runtime contract — what is neutral, what is a tokio-bound plugin,
and what needs a reactor today — is stated once in
[`crates/synthia-core/src/spawn.rs`](crates/synthia-core/src/spawn.rs)
and proven by `cargo run --example runtime_agnostic -p synthia-harness`.

## 3. Extensions (each is a crate, each is opt-in)

| Seam | Trait | Ships in-tree | Install yours |
|---|---|---|---|
| long-term / working memory | `Memory` | `InMemoryMemory`, `SessionMemory`, `FileMemory`, `SqliteMemory` (feature `sqlite`) | hand an `Arc<dyn Memory>` to the component that talks to it (the server's state, or your own) |
| embeddings | `Embedder` | `ModelProviderEmbedder` (any `ModelProvider` becomes one) | construct the embedder where you build the index |
| skills | `SkillProvider` | `FileSkillProvider`, `InMemorySkillProvider`, `RemoteSkillProvider` | `SkillRegistry::new()` + `register(provider)`; surface them with `PromptContext::with_skill(name, description)` and expose the `skill` tool via `register_skill_tool(&registry)` |
| delegation / peers | `synthia_harness::ToolInterceptor` + `AgentRegistry` | `synthia-tool-task`'s `TaskDelegator` (the `task` tool), `LeaderRouter`, `ConditionalRouter`, `SubagentPool` | `.with_interceptor(Arc::new(TaskDelegator::new(registry)))` — the loop then offers the `task` tool |
| candidate scoring | `CandidateScorer` | `LongestAnswer` (default), `LlmJudgeScorer` | `BestOfNStrategy::new(n, Arc::new(my_scorer))` — the strategy samples, your scorer decides; an LLM judge is just a scorer that calls a provider |
| subagent notification | `synthia-tool-task`'s `GroupJoin` | — (the host arms its own timer) | register the group before launching, then route every completion through `GroupJoin::on_complete` and tick it from `next_deadline()` |
| interactive steering | `RunInbox` | `MpscInbox` | `let (inbox, handle) = MpscInbox::channel();` + `.with_run_inbox(Arc::new(inbox))` |
| remote tools | `McpTransport`, `TransportFactory` | `StdioTransport`, `StreamableHttpTransport`, `InMemoryTransport` | `McpClient::new(transport)`; `McpSupervisor::supervise(name, factory)` for reconnect. Two model-facing paths over that client: `register_mcp_tools(&registry, .., NamingPolicy::Namespaced)` publishes one local tool per remote tool, `register_mcp_control_tool(&registry, supervisor)` publishes the single `mcp` tool that lists and calls remote tools at run time. Both want the same *instance* the agent loop dispatches from (a clone is a deep copy); the control tool takes an `&Arc<ToolRegistry>` because it keeps a weak handle for the privacy lookup, so it refuses a tool the registry marks hidden rather than being a way around the guard |
| command confinement | `SandboxBackend` (+ `ExecutionPolicy`), both in `synthia-tool-shell` | `BwrapBackend`, `NoopBackend` | `ShellTool::with_policy(ExecutionPolicy::…)` + `.with_sandbox(backend)`; the tool holds the policy, so what it advertises and what it enforces cannot diverge, and a confined policy without a backend fails closed |
| workflow effects | `WorkflowHost` | — (the host is the consumer's) | `WorkflowRuntime::new(host)`; the host owns `spawn_agent` / `run_gate` |
| branch scoring | `Scorer` | `HeuristicScorer` | register yours under the document's wire name: `HeuristicScorer::new(name, Arc<dyn Scorer>)` |
| eval metrics | `Metric`, `AsyncMetric`, `JudgeProvider` | `KeywordMetric`, `SchemaValidationMetric`, `LlmJudgeMetric<P>` | `EvalRunner::new().metric(Box::new(my_metric))`, `LlmJudgeMetric::new(my_judge)` |
| job store | `ScheduleStore` | the PID-locked JSON store (`ScheduleStore::new(root)`) | `Scheduler::new(store)`; the timer is the caller's (`tick(now)`). A tool-side write (or a `ScheduleStore::load`) reaches the next `tick` / `next_deadline` on its own — the pass notices the store's revision move and rebuilds its wheel — and `resync(now)` is the explicit rebuild for between passes. Share one `Arc<ScheduleStore>` with `SchedulerTool::new(store)` to put the same schedule under the model's control |
| attachment bytes | `AttachmentBackend` | `FilesystemBackend`, `InMemoryBackend` | `AttachmentStore::new(backend)` / `.with_backend(backend)` |
| **multimodal payloads** | `Tool` (returns `ToolOutput` of `ContentPart`s) | `McpTool` (MCP `image`/`audio` blocks → `ContentPart::{Image, Audio}`), `attachment_to_content_part` (a stored image → `ContentPart::Image`) | a tool that returns an image calls `ToolOutput::from_parts(vec![text, ContentPart::Image(..)])`; the loop's output transformer rewrites only the *text* projection, so the binary survives to the provider (both the Anthropic and OpenAI adapters emit a real image block). The user → model direction is `AttachmentStore::save_image` → `attachment_to_content_part` → `ContentPart::Image` on the message |
| oversized tool output | `FullOutputStore` | `InMemoryFullOutputStore` | `full_output_tool(store)` / `RetrieveFullOutputTool::new(store)` |
| provider tokenizers | `TokenCounter` | the two adapters' impls | implement it for a provider that counts more precisely |
| security redaction | `SensitiveData` | `Sensitive<T>` wrappers | implement on your config type so `Debug` never prints secrets |

## 4. Application-crate seams

`synthia-server` is the one *application* crate; its internals are not
part of the library contract, but they follow the same shape if you
replace the server with your own binary:

| Seam | Trait | Purpose |
|---|---|---|
| `RunStreamFactory` | turn a run request into an agent event stream | the place a custom deployment swaps the loop |
| `SessionLane` | admit / queue a session's work | bounded concurrency policies |
| `SessionController` | session state machine + broadcast | the server's equivalent of `ReActAgent::run` |

Two more seams exist but are **crate-internal** by design, so they are
not part of the consumer contract: `CommandRunner` (the `synthia-tool-task`
gate's process runner — `run_subagent_with_runner` is `pub(crate)`;
tests inject a recording fake) and the `Reader` trait inside the JSONL
sink. If you need either, that is a signal to shape the feature as an
issue rather than to patch around it.

## Verifying a substitution

Every seam above has an executable proof in the example ladder, and the
repo's gates keep the map honest:

```bash
make examples        # 40 examples + both standalone consumer crates
make check-mvp-deps  # the 7-feature subset pulls no HTTP/OTel/DB dependency
make check-no-runtime# the runtime-free crates stay tokio-free
make check-clock     # library code reads the wall clock through Clock
```

Trait-by-trait source of truth: each crate's `lib.rs` (module docs) and
[`docs/examples/README.md`](docs/examples/README.md) (the index with
what to look at in each example).
