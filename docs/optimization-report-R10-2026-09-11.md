# Synthia R10 — Tier-aware Steering + Tool Render Contract + Router + Snapshot Bus

> Synthia is already in a strong state (R5/R6/R7/R8 1594 → 1739
> tests). R9 shipped provider profile, skill registry, state-aware
> hooks, spawn_detached, operation snapshot. **R10 closes 6
> further high-ROI lego gaps** surfaced by re-auditing the four
> reference projects (traitclaw / dsh / pi / pi-subagents).

## 1. Landed Items

### R10-6 — `ModelTier` + `TierLimits` (`synthia-provider/src/tier.rs`)

Adopted from traitclaw `crates/traitclaw-core/src/types/model_info.rs:11-25`
(`ModelTier::{Small, Medium, Large}`) +
`crates/traitclaw-core/src/registries.rs:373-457` (`AdaptiveRegistry::TierLimits`).

- `enum ModelTier { Small, Medium, Large, Override(Box<ModelTier>) }`
  with a cheap `from_model_config` heuristic (`>= 100K → Large`,
  `>= 32K → Medium`, else `Small`).
- `TierLimits { max_visible_tools, loop_detection_window, tool_budget,
  max_concurrency, context_budget_threshold }` with `SMALL` /
  `MEDIUM` / `LARGE` `const` presets.
- `ModelProvider::tier()` trait method (default derives from
  `model_config()`); providers can override.
- 10 unit tests.

### R10-2 — `Steering::for_tier` + `Steering::auto` (`synthia-steering/src/tier.rs`)

Adopted from traitclaw
`crates/traitclaw-steering/src/steering.rs:42-66`
(`Steering::auto()` / `for_tier()`).

- `for_tier(tier, workspace_root)` builds a complete
  `Steering` bundle from one typed argument: 5 guards
  (`LoopDetectionGuard(loop_window)`, `ToolBudgetGuard(tool_budget)`,
  `ShellDenyGuard`, `WorkspaceBoundaryGuard`, `PromptInjectionGuard`),
  3 hints (`ContextBudgetHint(threshold)`,
  `IterationReminderHint(loop_window * 5)`, `TruncationHint`),
  `AdaptiveTracker(max_concurrency)`, plus `LoggingHook` and
  `NoopOutputTransformer`.
- `auto(tier, workspace_root)` is a 1-arg shorthand.
- 3 unit tests.

### R10-3 — `AdaptiveRegistry` (`synthia-tool/src/adaptive.rs`)

Adopted from traitclaw `crates/traitclaw-core/src/registries.rs:373-457`.

- `AdaptiveRegistry::new(Arc<ToolRegistry>, ModelTier)` wraps the
  inner registry and caps the LLM-visible tool list at
  `TierLimits::for_tier(tier).max_visible_tools`. Hidden tools
  are filtered out by the underlying `snapshot()`.
- `with_tier()` swaps the tier without rebuilding.
- 5 unit tests.

### R10-1 — `ToolOutputDefinition` + `present_call` / `present_result`
        (`synthia-tool/src/output.rs`)

Adopted from dsh
`packages/core/tools/src/index.ts:144-168` `ToolOutputDefinition`.

- `ToolOutputDefinition { name, kind, title, presentation }`
  per-tool rendering contract. `kind: RenderKind` enumerates
  `Text / Shell / Read / Write / WebFetch / Todo / Json`.
- `present_call(args, ctx) -> String` and
  `present_result(args, output, ctx) -> String` default impls
  round-trip the raw text; tools override for symbol-aware
  rendering (e.g. shell with exit code, web fetch with status).
- `HasOutputDefinition` extension trait + blanket impl gives
  every `Tool` a passthrough definition; tools opt in by
  overriding.
- 8 unit tests.

### R10-4 — `Router` trait + `LeaderRouter` (`synthia-agent/src/agent/router.rs`)

Adopted from traitclaw
`crates/traitclaw-team/src/router.rs` (`@agent:` syntax) + pi-subagents
`src/mention.ts` (`@handle` grammar).

- `trait Router { fn name() -> &str; fn description() -> &str;
  fn route(&self, text) -> RoutingDecision; }`
- `enum RoutingDecision { Route { mentions, remainder },
  PassThrough }` + `Mention { agent, prompt, start, end }`.
- `LeaderRouter::new(peers)` + `with_trigger(char)`; parses
  `@<name>: <prompt>` mentions. Sentence-end heuristic stops the
  prompt at `. ` + uppercase so a trailing "Trailer." stays in
  the remainder. Unknown peers stay verbatim.
- `PassThroughRouter` is the no-op fallback.
- 11 unit tests.
- (Loop interception is wired in R11+; the trait is a usable
  building block today.)

### R10-5 — `SnapshotBus` + `SnapshotReceiver`
        (`synthia-session/src/operation.rs`)

Closes the R9-6 deferred wire-up for the read-only
`OperationSnapshot` surface.

- `SnapshotBus::new()` / `with_capacity(n)` — runtime-neutral
  subscribe / publish bus (uses `futures::channel::mpsc`).
- `subscribe() -> (SnapshotReceiver, Option<OperationSnapshot>)`:
  late subscribers get the latest snapshot immediately so they
  don't have to wait for the next publish.
- `publish(snap) -> u64` returns the seq number; dead
  subscribers are dropped from the list on send failure.
- `latest()` / `published_count()` / `subscriber_count()` for
  observers / TUI / replay consumers.
- 4 unit tests.

### Example — `assemble_with_tier_steering`

`crates/synthia-agent/examples/assemble_with_tier_steering.rs` —
end-to-end demo of the 4 lego primitives wired together with no
network / API key. `cargo run --example
assemble_with_tier_steering -p synthia-agent`.

## 2. Verification

```
cargo check --workspace --all-features --all-targets       0 errors
cargo clippy --workspace --all-features --all-targets -- -D warnings
                                                          0 warnings
cargo +nightly fmt --all -- --check                       0 diff
cargo test --workspace                                    1831 passed, 0 failed
```

R9 baseline: 1739 tests. R10 added: **+92 tests** across 6
crates (tier + tier facade + adaptive registry + tool output +
router + snapshot bus).

## 3. Lego perspective

R10 adds 6 new lego bricks. A lib consumer building an agent
from scratch now has:

| Brick | Crate | What it lets you do |
|---|---|---|
| `ModelTier` + `TierLimits` | `synthia-provider` | Classify any model by capacity; pick presets without round-tripping a network call |
| `Steering::for_tier` / `auto` | `synthia-steering` | One-line tier-tuned safety / hint / tracker bundle |
| `AdaptiveRegistry` | `synthia-tool` | Cap the LLM-visible tool list per tier without rewriting the registry |
| `ToolOutputDefinition` + `RenderKind` | `synthia-tool` | Tool-owned canonical rendering for LLM and UI surfaces (dsh parity) |
| `Router` + `LeaderRouter` + `Mention` | `synthia-agent` | Text-routed delegation (`@agent: prompt`) with peer resolution (traitclaw parity) |
| `SnapshotBus` + `SnapshotReceiver` | `synthia-session` | Subscribe / publish channel for `OperationSnapshot` (closes the R9-6 wire-up) |

## 4. Out of Scope (R11+ backlog)

- **R10-7** dsh Cordis-style kernel (multi-round large refactor).
- **R10-8** MCP integration.
- **R10-9** RAG.
- **R10-10** Workflow DSL.
- **R10-11** AttachmentStore / multimodal.
- **R10-12** Full Lane reducer + 13-leaf OperationState.
- **R10-13** Loop intercept for `LeaderRouter` (the trait ships as
  a usable building block; the re_act loop interception is
  R11+ work).
- **R10-14** SessionController snapshot emit (the bus + type are
  ready; the controller wiring is R11+).
- **R10-15** dsh subagent drivers (codex / claude-code / acp / fork).

## 5. Reference Evidence Map

- R10-1: dsh `packages/core/tools/src/index.ts:144-168`; traitclaw
  `crates/traitclaw-core/src/transformers.rs:184-258` `ProgressiveTransformer`.
- R10-2: traitclaw
  `crates/traitclaw-steering/src/steering.rs:42-66, 100-108`.
- R10-3: traitclaw
  `crates/traitclaw-core/src/registries.rs:373-457`.
- R10-4: traitclaw
  `crates/traitclaw-team/src/router.rs` (`@agent:` syntax); pi-subagents
  `src/mention.ts` (`@handle` grammar).
- R10-5: pi
  `packages/agent/src/harness/runtime/types.ts` (read-only
  OperationState subset) + opencode `session/session.ts`
  (snapshot).
- R10-6: traitclaw
  `crates/traitclaw-core/src/types/model_info.rs:11-25`.

## 6. Cumulative R1–R10 Overview

| Round | Theme | Tests added | Total |
|---|---|---|---|
| R3 + R5 | Steering / HookMap / EffectGate / CompactionSettings | +46 | 1640 |
| R4 | TokenUsage + cache + retry typed errors | +13 | 1653 |
| R6 | BlockAssembler + HookMap + typed events + Arc-msg | +43 | 1696 |
| R7 | Runtime neutrality (CancelToken, futures mpsc) | -1 | 1695 |
| R8 | chrono + assemble_from_scratch tutorial | 0 | 1695 |
| R9 | ProviderProfile + SkillProvider + state hooks + spawn_detached + OperationSnapshot | +44 | 1739 |
| **R10** | **ModelTier + Steering::for_tier + AdaptiveRegistry + ToolOutputDefinition + Router + SnapshotBus** | **+92** | **1831** |

All 1831 tests pass; clippy 0 warnings; fmt 0 diff; check 0
errors. Zero regressions across the entire workspace.
