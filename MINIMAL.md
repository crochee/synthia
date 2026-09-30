# The Minimal Synthia Agent

A from-zero guide for the consumer who wants the **smallest** working
agent and is willing to learn more pieces later. Each crate is opt-in;
nothing in `synthia` is forced on you.

## Step 0 — Decide what you actually need

| Want to … | Add this crate | Add this feature flag |
|---|---|---|
| Talk to a model (your own impl) | `synthia-provider` | `provider` |
| Use the bundled Anthropic adapter | `synthia-provider` | `provider-anthropic` |
| Use the bundled OpenAI adapter | `synthia-provider` | `provider-openai` |
| Define tools | `synthia-tool` | `tool` |
| Give the agent the offline file and process tools | `synthia-tool-read`, `-write`, `-shell`, `-todo` | `tool-read`, `tool-write`, `tool-shell`, `tool-todo` |
| Let the agent fetch URLs | `synthia-tool-web` | `tool-web` |
| Delegate to peer agents | `synthia-tool-task` | `tool-task` |
| Run a ReAct loop | `synthia-harness` | `harness` |
| Have a context window policy | `synthia-context` | `context` |
| Persist session events | `synthia-session` | `session` |
| Apply guards / hints / hooks | `synthia-steering` | `steering` |
| Cancel a run | `synthia-core` (in the others already) | `core` |
| Derive `#[derive(Tool)]` | `synthia-macros` | `macros` |
| Persist SQLite memory | `synthia-context` | `sqlite` |
| Drive MCP servers | `synthia-mcp` | `mcp` |
| Let the agent browse and call MCP tools | `synthia-mcp` | `mcp` |
| Run a scheduler (host-owned timer) | `synthia-scheduler` | `scheduler` |
| Let the agent create / pause / cancel scheduled jobs | `synthia-tool-scheduler` | `tool-scheduler` |
| Define workflows | `synthia-workflow` | `workflow` |
| Run eval suites | `synthia-eval` | `eval` |
| Bundle skills | `synthia-skill` | `skill` |
| Attach images / audio / files | `synthia-attachment` | `attachment` |
| Ship a full HTTP server | `synthia-server` | `server` |

The default feature set assembles a basic agent — including the two
bundled HTTP adapters and the `web_fetch` builtin. The **minimum**
subset you can ship an MVP with is:

```toml
[dependencies]
synthia = { version = "0.1", default-features = false, features = [
    "core", "provider", "context", "tool", "session", "steering", "harness",
] }
```

Note what that subset does **not** claim: `provider` is the
`ModelProvider` trait and the wire types, not the Anthropic/OpenAI
adapters, and `tool` is the **paradigm** — the registry plus the `Tool`
trait — not a tool set. Every agent-facing tool implementation is its
own plugin crate behind its own feature (`tool-read` / `tool-write` /
`tool-shell` / `tool-todo` / `tool-task`, and `tool-web` for the one
that talks to the network; `tool-scheduler` and `tool-search` are
opt-in like `mcp` because their hosts must construct the
`ScheduleStore` / `Registry` they adapt), so
the seven-feature subset compiles **no
agent-facing tool at all**: the only `Tool` impls it links are the
harness's own synthetic contracts (`__get_full_output`,
`structured_output`) and the
registry passthrough. That is
the whole point — this agent has your provider and no outbound fetch, so
it compiles **no HTTP client at all**: no `reqwest`, no `hyper`, no
`rustls`, no `tower`. Add `provider-anthropic`, `provider-openai`, or any
`tool-*` feature when you want them.

## Step 1 — Write the provider

The one piece the framework cannot supply. Only `complete` is required:
`complete_with_stream` has a default that calls it and emits the result
as a single delta, so overriding it is an optimisation, not an
obligation.

```rust
use synthia::prelude::*;

// Simple path: one synchronous answer per turn.
struct OneShot { answer: &'static str }

#[async_trait]
impl ModelProvider for OneShot {
    async fn initialize(&mut self, _: ProviderConfig) -> Result<(), Error> { Ok(()) }
    fn name(&self) -> &str { "one-shot" }
    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "one-shot-1".into(),
            provider: "one-shot".into(),
            context_window: 8_192,
            max_output_tokens: 1_024,
            supports_tools: false,        // MVP without tools?
            supports_streaming: false,
            supports_reasoning: false,
        }
    }
    async fn complete(&self, _: CompletionRequest)
        -> Result<CompletionResponse, Error>
    {
        Ok(CompletionResponse {
            content: Content::text(self.answer),
            ..CompletionResponse::default()
        })
    }
    // No `embed` impl needed — the trait's default returns empty
    // vectors per input. Only override when you actually have an
    // embedding endpoint (Anthropic / OpenAI adapters ship with
    // theirs pre-implemented).
}
```

For a real provider, see `synthia-provider::AnthropicProvider` and
`synthia-provider::OpenAICompatibleProvider`. They both implement the
same trait, so the rest of your code does not change.

## Step 2 — Assemble the agent

The minimum four pieces, plus the drive loop. This is a function rather
than a `main` so it reads as the assembly and nothing else: hand it the
provider from Step 1 (`assemble(Arc::new(OneShot { answer: "hi" }))`) and
it runs one turn.

```rust
use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use synthia::prelude::*;

async fn assemble(provider: Arc<dyn ModelProvider>) {
    // 1) tools      — the paradigm ships no agent-facing set: the
    //                 `tool` feature is the registry + the `Tool`
    //                 trait. Register your own here, and add the
    //                 plugin crates (`tool-read`, `tool-shell`, …)
    //                 as you want them.
    let registry = Arc::new(ToolRegistry::new());

    // 2) steering   — noop means no guards, no hints, no hooks
    let steering = Arc::new(Steering::noop());

    // 3) context    — truncating drops the oldest pair until it fits
    let cm: Arc<dyn ContextManager> = Arc::new(TruncatingContextManager);

    // 4) cancel     — std-only AtomicCancelToken; runtime is your choice
    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();

    let agent = ReActAgent::new(provider, registry)
        .with_workspace(".")
        .with_steering(steering)
        .with_context_manager(cm)
        .with_max_iterations(4);

    // Drive one turn.
    let mut stream = agent.run(AgentInput::text("hello"), cancel).await;

    while let Some(event) = stream.next().await {
        // AgentEvent::Model / ModelDone / System variants.
        // Drain and process or just print.
        let _ = event;
    }
}
```

That is a working agent. There is no `axum`, no `tokio::*` in the
public signature, no HTTP client, no OTel, no SQLite.

That claim is **executable**, not aspirational:

```bash
# the smallest agent, compiled and run as an independent crate
cd docs/examples/minimal-consumer && cargo run
# → MVP-OK  (two model calls, one tool call, 12 structural events)
# its whole dependency tree is 94 crates (the consumer's own tokio
# included — the framework itself names no runtime), and `cargo tree`
# in that directory lists no reqwest / hyper / rustls / tower /
# opentelemetry

# and the dependency promise, asserted
make check-mvp-deps
# → OK: core,provider,context,tool,session,steering,agent pulls no
#      (opentelemetry|tonic|axum|rusqlite|sqlx|reqwest|hyper|rustls|h2|tower|webpki)
```

`docs/examples/minimal-consumer` is the guide above turned into a
program: one dependency (`synthia`, `default-features = false`), the
seven features, a scripted provider that asks for a tool, a hand-written
`Tool`, and an assertion that the tool really ran. It compiles no tool
implementation at all — the paradigm ships the registry and the trait,
and the program composes its own. `make
check-mvp-deps` fails the build if the HTTP client, the OTLP exporter,
the HTTP framework, a database driver, or the gRPC transport ever
re-enters that feature set.

## Step 3 — Add pieces only when you need them

| Need | How |
|---|---|
| A bundled model adapter | add the `provider-anthropic` / `provider-openai` feature → `AnthropicProvider` / `OpenAICompatibleProvider` |
| The offline file and process tools | add `tool-read` / `tool-write` / `tool-shell` / `tool-todo` and register the bricks you want: `registry.register_entry(ToolEntry::new(Arc::new(ReadTool::new())))` |
| `web_fetch` | add the `tool-web` feature |
| Your own executor | `.with_spawner(Arc::new(MySpawner))` — the loop's per-run detach goes wherever you say (`cargo run --example runtime_agnostic -p synthia-harness`) |
| Typed event sink | `.with_typed_event_sink(TypedEventSink::channel(64).0)` |
| Structured output schema | `.with_output_schema(json!({...}))` |
| LLM-driven compaction | `.with_compaction_settings(CompactionSettings::default())` |
| Tier-aware steering | `tier::auto(provider.tier(), ".")` instead of `Steering::noop()` |
| Skills | `SkillRegistry::new()` + `reg.register(FileSkillProvider::discover(dir))` for the catalog; surface them with `PromptContext::with_skill(name, description)` and register the `skill` tool with `register_skill_tool(&registry)` |
| Multi-agent delegation | `synthia-tool-task`: `agent.with_interceptor(Arc::new(TaskDelegator::new(peers)))` — the loop then offers the `task` tool |
| Remote MCP tools the host discovered | `synthia-mcp`: pre-register the catalog with `register_mcp_tools(&registry, client, "server", NamingPolicy::Namespaced)` (one local tool per remote tool), or publish the dynamic path with `register_mcp_control_tool(&registry, supervisor)` — the model then lists and calls remote tools itself. Both want the *same instance* the agent loop dispatches from (a `ToolRegistry` clone is a deep copy); the control tool takes it as `&Arc<ToolRegistry>` because it keeps a weak handle to consult the privacy flags later, so a tool the registry marks hidden is neither listed nor callable |
| Scheduling, model-side | add `tool-scheduler` and share one store: `let store = Arc::new(ScheduleStore::new(dir)); register_scheduler_tool(&registry, SchedulerTool::new(Arc::clone(&store)));` — the host still owns delivery (`Scheduler::new(store).tick(now)`), and a tool-side write (or a `ScheduleStore::load` that ingests another process's rows) reaches the next `tick` / `next_deadline` on its own: the pass sees the store's revision move and rebuilds its wheel. `Scheduler::resync(now)` stays available as the explicit rebuild, for a host that needs the wheel correct between passes |
| Interactive steering | `let (inbox, handle) = MpscInbox::channel();` + `.with_run_inbox(Arc::new(inbox))` |

Each one is **opt-in**: if you do not add the feature or call the
`with_*` setter, that code is not compiled into your binary.

The table's recipes are inline code spans, which rustdoc cannot see — so
they are transcribed below as a **compiled fence**, and this is the copy
CI checks. Two earlier rotations shipped wrong names here
(`AgentBuilder::skill_registry`, and `.spawner` / `.typed_event_sink` /
`.output_schema` / `.compaction_settings` / `.run_inbox` before the `with_`
prefix); if a setter is renamed, this fence stops compiling instead of the
table silently lying.

```rust
use std::sync::Arc;

use serde_json::json;
use synthia::harness::{MpscInbox, ReActAgent};
use synthia::context::CompactionSettings;
use synthia::core::spawn::{BoxFuture, Spawner};
use synthia::prelude::*;
use synthia::tool::ToolRestriction;

// Everything the Step-3 table names, in one chain. Each call is the
// real mutator name: the bare `.spawner(..)` / `.strategy(..)` /
// `.run_inbox(..)` forms are *getters*, so they do not take an argument.
#[allow(dead_code)]
fn every_opt_in(
    provider: Arc<dyn ModelProvider>,
    registry: Arc<ToolRegistry>,
) -> ReActAgent {
    // Your own executor: the loop's per-run detach goes wherever you say.
    struct MySpawner;
    impl Spawner for MySpawner {
        fn spawn(&self, task: BoxFuture<()>) {
            drop(task);
        }
        fn spawn_blocking(&self, _f: Box<dyn FnOnce() + Send + 'static>) {}
    }

    let (typed_sink, _rx) = TypedEventSink::channel(64);
    let (inbox, _handle) = MpscInbox::channel();

    ReActAgent::new(provider, registry)
        .with_spawner(Arc::new(MySpawner))
        .with_typed_event_sink(typed_sink)
        .with_output_schema(json!({"type": "object"}))
        .with_compaction_settings(CompactionSettings::default())
        .with_run_inbox(Arc::new(inbox))
        .with_tool_restriction(ToolRestriction::deny(["shell"]))
        .with_context_manager(Arc::new(TruncatingContextManager))
        .with_steering(Arc::new(Steering::noop()))
}
```

## Step 4 — When you outgrow the prelude

The prelude is the **minimum** vocabulary for the canonical
7-piece assembly. Beyond that, name types by module path:

```rust
use synthia::{
    context::DagContextManager, // pi-lcm hierarchical DAG
    context::SummarizingContextManager, // pi-style context prune
    session::jsonl::JsonlSessionSink, // production durable sink
    tool::GroupedRegistry,     // named tool groups
};
```

The plugin crates add their own modules once you enable the feature —
`synthia::tool_read::ReadTool`, `synthia::tool_web::WebFetchTool`,
`synthia::tool_task::{TaskDelegator, SubagentPool, LeaderRouter}`, and so on:
if a name is not in the prelude, the module that owns it has it.

Every `pub use` at the facade's module roots is a lego brick; the
full long-tail stays reachable behind the module path even when not
in the prelude.

## Anti-patterns

| Don't | Do |
|---|---|
| Reach into `synthia_provider::types::Content` directly from your app | Use `Content` through the prelude or `synthia::prelude::Content` |
| Hard-code `Arc<tokio_util::sync::CancellationToken>` as a `CancelToken` | Use `synthia_core::AtomicCancelToken` and keep the runtime choice yours |
| Implement `Agent` directly for a custom paradigm | Compose: change the loop, keep `Steering` / `ContextManager` / `ToolRegistry` the same |
| Build `Vec<Arc<dyn Tool>>` by hand | Use `ToolRegistry` — it gives you scoping, grouping, and the deferred-exposure seam for free |
| Pass `&dyn ModelProvider` across an `await` | Wrap it in `Arc<dyn ModelProvider>` once, share the `Arc` |

## Where to read next

- [`SEAMS.md`](SEAMS.md) — every extendable trait in one table, with the line that installs a replacement
- [`crates/synthia/examples/assemble_from_zero.rs`](crates/synthia/examples/assemble_from_zero.rs) — the full MVP code as a runnable program
- [`docs/examples/README.md`](docs/examples/README.md) — every example, organised by seam
- [`crates/synthia/Cargo.toml`](crates/synthia/Cargo.toml) — the canonical feature → module map
- [`docs/architecture/adr/`](docs/architecture/adr/) — the decisions that shaped this shape
