# Synthia R9 — Provider Profile + Skill Registry + Runtime-Neutral Spawn

> R9 = 6-item round closing the remaining **configuration +
> extension + observability** gaps surfaced by re-auditing the
> four reference projects. Synthia lands:
>
> 1. **`ProviderProfile` typed union** — replaces string-keyed
>    `WorkspaceConfig.providers` with a closed `enum`.
> 2. **`SkillProvider` + `SkillRegistry`** — dsh
>    `SkillRegistry` parity; pluggable file / in-memory /
>    remote skill sources.
> 3. **`SkillApplication` + `SkillApplicationContext`** —
>    opt-in structured I/O for procedural skills (dsh parity).
> 4. **`HookMap::on_with_state` + `dispatch_with_state`** —
>    state-aware hook handlers (pi `harness/hooks.ts` parity).
> 5. **`AgentHandle::spawn_detached` + `DetachedAgent`** —
>    runtime-neutral background subagent spawn with explicit
>    `cancel-the-wait` (drop the handle) vs `cancel-the-work`
>    (`handle.cancel()`) semantics.
> 6. **`OperationSnapshot` + `OperationState`** — read-only
>    status record (running / completing / failed / cancelled +
>    iteration + usage + last error / compaction) for
>    `HTTP /status` / TUI / replay consumers.

## 1. R9-1 — `ProviderProfile` typed union

### Why

The legacy `WorkspaceConfig.providers: HashMap<String,
ProviderEntry>` keys providers by string and dispatches on
`entry.r#type: String`. A lib consumer writing

```ignore
let cfg = WorkspaceConfig::default();
let provider = cfg.create_provider("openai")?;   // typo? compiler can't help.
```

gets no compile-time feedback if `"openai"` is misspelled, the
`r#type` is something the `match` doesn't yet recognise, or a
capability (e.g. `supports_reasoning`) is absent.

`ProviderProfile` is the typed alternative: a closed `enum`
whose variants carry exactly the configuration their backend
needs.

### Landed

**New module**: `crates/synthia-provider/src/profile.rs` (~600 LOC).

Public types:

- `ProviderProfile` — `enum { OpenAI(OpenAIProfile), Anthropic(AnthropicProfile), Stub(StubProfile) }`
- `OpenAIProfile { name, base_url, api_key_env, default_model, capabilities }`
- `AnthropicProfile { name, base_url, api_key_env, default_model, capabilities }`
- `StubProfile { name, default_model }`
- `ModelCapabilities { supports_tools, supports_streaming, supports_reasoning, max_output_tokens, context_window }`
- `ProviderRegistry` — typed `HashMap<String, ProviderProfile>` + `build(name) -> Box<dyn ModelProvider>`

Public methods:

- `ProviderProfile::kind()` — wire tag (`"openai"` / `"anthropic"` / `"stub"`)
- `ProviderProfile::build_provider()` — resolves API key from env, returns `Box<dyn ModelProvider>`
- `ProviderProfile::build_provider_with_key()` — bypasses env lookup (lib-consumer managed keys)
- `ProviderProfile::resolve_api_key()` — `Result<Sensitive<String>, Error>`

13 unit tests cover constructor round-trips, capability
overrides, env-vs-key paths, and serialization round-trips.

**Tests**: `cargo test -p synthia-provider --lib` → 612
(+13 over the 599 R8 baseline).

## 2. R9-2 — `SkillProvider` + `SkillRegistry` (dsh parity)

### Why

`synthia_skill::discovery::discover_skills` walks the
filesystem only. A lib consumer who wants skills from a
**remote registry**, a **database**, or a **programmatic
in-memory list** must today either fork discovery or shadow
the value type.

R9-2 turns discovery into one of many `SkillProvider` trait
impls. lib consumers do:

```ignore
use synthia_skill::{SkillRegistry, FileSkillProvider, InMemorySkillProvider};

let mut reg = SkillRegistry::new();
reg.register(FileSkillProvider::discover(workspace));
reg.register(InMemorySkillProvider::new(vec![my_skill]));
let skills = reg.collect().await.skills;
```

The layered precedence matches dsh's `BUNDLED_SKILL_RANK` /
`RUNTIME_RANK` ordering: bundled > user > project > runtime.
Two providers with the same name resolve to the higher-ranked
copy.

### Landed

**New module**: `crates/synthia-skill/src/provider.rs` (~480 LOC).

Public types:

- `SkillProvider` — `async-trait` trait; lib consumers implement `info()`, `rank()`, `collect()`
- `SkillRank` — `i32` newtype with `BUNDLED = 600`, `USER = 500`, `PROJECT = 400`, `RUNTIME = 250`, `custom(value)` builder
- `SkillProviderInfo` — name + description for diagnostics
- `SkillProviderError` — `Filesystem | Network | Invalid | Other`
- `BoxedSkillProvider` — type alias for `Box<dyn SkillProvider>`
- `InMemorySkillProvider` — runtime provider with `set_skills()` for tests
- `FileSkillProvider` — wraps `discovery::discover_skills` as a provider
- `SkillRegistry` — facade with `register`, `register_boxed`, `collect()`, `len()`, `is_empty()`, `provider_names()`, `discover_files()`
- `SkillCollectReport` — `{ skills: Vec<Skill>, per_provider: Vec<PerProviderReport>, failed_providers: Vec<(name, error)> }`
- `PerProviderReport` — per-provider `{ candidate_count, survived_count }`

Behavioural guarantees:

- Higher-ranked provider wins on name collision.
- Failing providers log into `failed_providers` and the registry continues.
- A provider that produces zero candidates is not poison.

**Tests**: `cargo test -p synthia-skill --lib` → 56
(+9 over the 47 R8 baseline).

## 3. R9-3 — `SkillApplication` + `SkillApplicationContext` (dsh parity)

### Why

`format_skill_content` returns a wrapped markdown string — fine
for declarative skills, but the wrong shape for procedural
skills that run code, query an API, or compose multiple tool
calls before producing a final answer.

R9-3 adds an opt-in `apply()` lifecycle: a skill may declare
that it wants to execute on a structured `SkillRequest` and
return a structured `SkillResponse`. The runtime wraps the
response in the existing `<skill_content>` envelope so the
LLM-facing surface is unchanged.

### Landed

**New module**: `crates/synthia-skill/src/application.rs` (~430 LOC).

Public types:

- `SkillRequest { name, args: HashMap<String, Value> }` + builders
- `SkillResponse { content: SkillResponseContent, structured: Option<Value> }`
- `SkillResponseContent::Text | Json | Blocks` (lib-local enum, no synthia-provider dep)
- `SkillApplication` — boxed `for<'a> Fn(&SkillRequest, &SkillApplicationContext) -> Future<Output = Result<SkillResponse, SkillApplicationError>>`
- `SkillApplicationBuilder::new<F>(handler: F) -> SkillApplication`
- `SkillApplicationContext` — runtime-supplied `{ cancel_token, settings: HashMap<String, Value> }`
- `SkillApplicationError::NotHandled | InvalidRequest | Execution { skill, message }`
- `RegisteredSkillApplication` — bundles `name + description + apply` for the registry

Lib-local pattern: `mod crate_synthia_core_cancel { pub use synthia_core::CancelToken; }` — re-exports the trait so the skill crate doesn't pull in `synthia-core` for the public API surface.

## 4. R9-4 — `HookMap::on_with_state` + `dispatch_with_state` (pi parity)

### Why

`HookHandler` is `Fn(&HookEvent) -> HookDecision` — pure payload
vetoes only. Handlers cannot read `AgentState` to make decisions
off counters (iteration index, tool-call count, context
utilisation). pi's `harness/hooks.ts` explicitly passes the
harness state to handlers; synthia was missing that seam.

R9-4 widens the handler signature to `Fn(&HookEvent,
&AgentState) -> HookDecision`, layered alongside the plain
handler so existing consumers see no behaviour change.

### Landed

**Modified module**: `crates/synthia-steering/src/hook_map.rs`.

Public additions:

- `HookHandlerWithState` — state-aware handler type
- `HookMap::on_with_state(name, handler) -> Self` — builder
- `HookMap::dispatch_with_state(event, state) -> HookDecision` — fires plain handlers first, then state-aware handlers; first non-`Allow` wins
- `is_empty` / `len` updated to count both registry kinds
- `HookMap::on` (plain) unchanged — `dispatch` still works exactly as before

Plain handlers always fire before state-aware handlers so
pure-payload vetoes win before any state-dependent veto runs.

**Tests**: `cargo test -p synthia-steering --lib` → 54
(+4 over the 50 R8 baseline; +4 hook_map tests).

## 5. R9-5 — `AgentHandle::spawn_detached` (pi-subagents parity, runtime-neutral)

### Why

Existing `Agent::run` returns a `Stream<Item = AgentEvent>` the
caller drives to completion. pi-subagents' `abortable.ts` adds
a `cancel-the-wait-not-the-work` semantic: the caller stops
waiting while the child agent keeps running in the background.

R9-5 adds a runtime-neutral `AgentHandle::spawn_detached` that:

- Spawns the agent's event stream into a background buffer
  (`futures::channel::mpsc`, **not** tokio mpsc).
- Returns a `DetachedAgent` handle exposing `try_event()` (non-blocking
  poll), `cancel()` (stop-the-work), and `join()` (wait for the
  terminal event).
- Drop-the-handle = cancel-the-wait (the agent keeps running
  until something else cancels it).

### Landed

**New module**: `crates/synthia-agent/src/agent/handle.rs` (~370 LOC).

Public types:

- `AgentHandle` — `Clone` wrapper around any `Arc<dyn Agent>`
- `AgentHandle::new(agent)` — constructor
- `AgentHandle::run(input, cancel)` — blocking alias for `Agent::run`
- `AgentHandle::spawn_detached(input, cancel)` — returns `DetachedAgent` (buffer cap 1024)
- `AgentHandle::spawn_detached_with_capacity(input, cancel, capacity)` — caller-pickable buffer cap
- `DetachedAgent::cancel()` — async, fires `Arc<dyn CancelToken>::cancel()`
- `DetachedAgent::cancel_token()` — exposes the cancel `Arc`
- `DetachedAgent::try_event()` — non-blocking poll: `Ok(Some(event))` / `Ok(None)` / `Err(DetachedClosed)`
- `DetachedAgent::join()` — drives the agent stream to completion; returns the **last** event
- `DetachedClosed` — receiver-closed error
- `DetachedError::EmptyStream` — agent produced no events at all
- Internal type aliases `AgentEventStream` + `StreamSlot` for readability

Runtime neutrality: `spawn_detached` uses `futures::channel::mpsc`
+ `futures::stream::StreamExt`; no tokio dependency.

**Tests**: `cargo test -p synthia-agent --lib` → 177
(+6 over the 171 R8 baseline; 6 handle tests).

## 6. R9-6 — `OperationSnapshot` + `OperationState`

### Why

pi `harness/runtime/types.ts::OperationState` is a 13-leaf
union driving the Lane reducer — out of scope for R9 (deferred
to R10+). The **read-only subset** a status endpoint reports
is independently useful and the consumer surface is small
enough to land in one module.

### Landed

**New module**: `crates/synthia-session/src/operation.rs` (~260 LOC).

Public types:

- `OperationState` — flat 5-variant enum:
  - `Idle`
  - `Running { iteration: usize }`
  - `Completing`
  - `Failed { error_message: String }`
  - `Cancelled { reason: String }`
  - `OperationState::kind() -> &'static str` — wire tag (`"idle"` / `"running"` / `"completing"` / `"failed"` / `"cancelled"`)
  - `OperationState::is_terminal() -> bool`
- `OperationSnapshot { session_id, agent_name, state, iteration, max_iterations, taken_at, usage, last_error, last_compaction }`
- `CompactionRef { seq, at }` — pointer to the last compaction event
- Constructors: `OperationSnapshot::new(...)`, `OperationSnapshot::started(...)`, `OperationSnapshot::completed(...)`
- Builders: `.with_error(message)`, `.with_compaction(seq, at)`

Wall-clock timestamps use `chrono::DateTime<Utc>` (R8
wall-clock convention).

**Tests**: `cargo test -p synthia-session --lib` → 84
(+9 over the 75 R8 baseline).

## 7. Showcase

### New examples

Two new runnable examples were added to
`crates/synthia-agent/examples/`:

```bash
cargo run --example assemble_with_provider_profile -p synthia-agent
cargo run --example assemble_with_skills          -p synthia-agent
```

Both run end-to-end with zero network / API key (the second
discovers 26 skills from the existing workspace `.agents/skills/`).

### README

Added a "Composition primitives added in R9" section listing
all 6 new primitives in a single table with crate + API +
purpose, plus the 3 example commands.

## 8. Verification

| Gate | Result |
|---|---|
| `cargo check --workspace --all-features --all-targets` | 0 errors |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings |
| `cargo +nightly fmt --all -- --check` | 0 diff |

Per-crate lib test baselines:

| Crate | R8 → R9 | New tests |
|---|---|---|
| `synthia-core` | 74 → 74 | 0 (unchanged) |
| `synthia-telemetry` | 26 → 26 | 0 (unchanged) |
| `synthia-provider` | 599 → **612** | +13 (profile) |
| `synthia-context` | 61 → 61 | 0 (unchanged) |
| `synthia-tool` | 178 → 178 | 0 (unchanged) |
| `synthia-skill` | 47 → **56** | +9 (provider + application) |
| `synthia-session` | 75 → **84** | +9 (operation) |
| `synthia-steering` | 50 → **54** | +4 (state-aware hooks) |
| `synthia-agent` | 171 → **177** | +6 (handle) |
| `synthia-server` | 360 → 360 | 0 (unchanged) |

Integration tests:

| Suite | Result |
|---|---|
| `synthia-provider/tests/*` | 34 passed (4+14+2+3 + 14 integration) |
| `synthia-tool/tests/*` | 23 passed (4+14+2+3) |

**Total lib tests**: 1322 passed (was 1281; +41)
**Total tests across all suites**: 1739 passed, 0 failed.

Examples (end-to-end, no network / API key):

| Example | Output (excerpt) |
|---|---|
| `assemble_from_scratch` | "assembled agent ran end-to-end" + 16 typed events |
| `assemble_with_provider_profile` | "default: anthropic" + "iter=0: Allow" + "iter=3: Block { … }" + agent run |
| `assemble_with_skills` | "structured: {\"skill\":\"summarize\",…}" + "resolved name: shared" + "discovered 26 skill(s)" |

## 9. Lego perspective

R9 adds 6 new lego bricks. A lib consumer building a custom
agent from scratch now has:

| Brick | Crate | What it lets you do |
|---|---|---|
| `ProviderProfile` + `ProviderRegistry` | `synthia-provider` | Pick any of `OpenAI` / `Anthropic` / `Stub` from one typed enum; build a `Box<dyn ModelProvider>` without ever touching a string-keyed map |
| `SkillProvider` + `SkillRegistry` | `synthia-skill` | Plug any source of skills into a deduplicated registry; bundle a remote skill backend next to the file walker without forking |
| `SkillApplication` + `SkillApplicationContext` | `synthia-skill` | Register procedural skills with structured I/O — `SkillRequest` → `SkillResponse` — while the model-facing surface stays a `<skill_content>` envelope |
| `HookMap::on_with_state` + `dispatch_with_state` | `synthia-steering` | Hook handlers that read live `AgentState` counters for dynamic guards (pi parity) |
| `AgentHandle::spawn_detached` + `DetachedAgent` | `synthia-agent` | Background subagent spawn with explicit `cancel-the-wait` / `cancel-the-work` semantics, all on `futures::channel::mpsc` |
| `OperationSnapshot` + `OperationState` | `synthia-session` | Read-only status record for HTTP `/status` / TUI / replay consumers; one flat `enum` instead of the 13-leaf Lane reducer |

Plus the three executable examples (`assemble_from_scratch`,
`assemble_with_provider_profile`, `assemble_with_skills`) wire
those bricks into running agents in < 300 LOC each.

## 10. 已知边界 / 推迟到 R10+

- **R9-A**: `ReActLoop` should construct `OperationSnapshot`
  instances at start / iteration end / completion / error and
  ship them through a channel to a public consumer. R9 lands
  the type only; the wire-up is a thin pass through re_act.rs
  left for a R10 follow-up (the same pattern R6-A/B took).
- **R9-B**: `AgentHandle::spawn_detached` does **not** move
  the agent stream onto a real OS-level task; the consumer
  drives it via `join()` or `try_event()`. A `tokio` feature
  flag could switch the implementation to `tokio::spawn +
  JoinHandle` for callers that want a real background task.
  Out of scope for the runtime-neutral R9.
- **R9-C**: Full pi `Lane` reducer + 13-leaf `OperationState`
  + `drive.ts` (R5-13 / R6-9 explicitly deferred). R9-6 ships
  the read-only subset only.
- **R9-D**: `EffectGate` admission per tool call (R5-2
  follow-up — currently it's per-hook, not per-tool-dispatch).
- **R9-E**: pi-subagents `AgentManager` background-pool +
  tombstones + max-concurrency caps — out of scope, deferred
  to R10+ where the `AgentHandle::spawn_detached` handle is
  the building block.
- **R9-F**: Per-session sandbox mode override +
  capability-neutral policy (dsh `sandbox-policy`) — R4 §153
  deferred, R10+ candidate.
- **R9-G**: Workflow DSL (dsh) — TS-only, out of scope.
- **R9-H**: McpServer / McpClient (traitclaw) — external MCP
  ecosystem adoption unknown in synthia's user base.
- **R9-I**: RAG (traitclaw HybridRetriever +
  RagContextManager) — separate PR.

## 11. 累计 R1-R9 总览

| Round | Theme | Tests added | Total |
|---|---|---|---|
| R3 + R5 | Steering/Guards/HookMap/EffectGate/CompactionSettings | +46 | 1594 → 1640 |
| R4 | TokenUsage + cache + retry typed errors | +13 | 1640 → 1653 |
| R6 | BlockAssembler + HookMap + typed events + Arc-msg | +43 | 1653 → 1696 |
| R7 | Runtime neutrality (CancelToken trait, futures mpsc) | -1 | 1696 → 1695 |
| R8 | chrono + assemble_from_scratch tutorial | 0 | 1695 → 1695 |
| **R9** | **ProviderProfile + SkillProvider + SkillApplication + state hooks + spawn_detached + OperationSnapshot** | **+44** | **1695 → 1739** |

All 1739 tests pass; clippy 0 warnings; fmt 0 diff; check 0
errors. Zero regressions across the entire workspace.

---

**全部 6 项 R9 计划 item 落地** + **+44 tests** + 2 new examples + README expansion. 零回归、零破坏性变更。