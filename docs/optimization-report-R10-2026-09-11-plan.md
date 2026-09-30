# Synthia R10 — Tool Render Contract + Tier-aware Steering + OperationSnapshot Wire-up

> Synthia lands 6 R5-R9 lego rounds (1640 → 1739 tests). R10 closes the
> remaining real-world lego gaps surfaced by re-auditing the four
> reference projects (traitclaw / dsh / pi / pi-subagents):
>
> 1. **`ToolOutputDefinition` + `present_call` / `present_result`** —
>    dsh `tools/presentCall` parity. Tool-owned canonical model-facing
>    and UI-facing projection; the LLM and the UI see the same shape
>    by construction. Replay-safe rendering for SSE / TUI / Web.
> 2. **`Steering::for_tier(ModelTier)` facade** — traitclaw
>    `Steering::auto()/for_tier()` parity. One-line safety configuration
>    that picks the bundle per model tier (Small/Medium/Large). Each
>    tier tunes guard strictness, tool budget, hint thresholds, tracker
>    concurrency in lock-step.
> 3. **`AdaptiveRegistry` + `TierLimits`** — traitclaw
>    `AdaptiveRegistry::with_tier_limits` parity. Small models see ≤N
>    tools; large models see the full set. Built on top of the existing
>    `ToolRegistry` so lib consumers can wrap any registry.
> 4. **`@agent:` text-routed delegation** — traitclaw `LeaderRouter`
>    parity. The leader agent emits `@agent_name: prompt` in its text;
>    the loop intercepts and delegates. Encodes multi-agent routing
>    in agent text rather than a separate state machine.
> 5. **`OperationSnapshot` wire-up to `SessionController`** — R9-6
>    deferred the wire-up; R10 closes the seam by adding a public
>    `subscribe()` channel on the controller and emitting a snapshot
>    at start / iteration / completion / error. Consumers (HTTP
>    `/status`, TUI, replay) read the live snapshot without touching
>    ReActLoop internals.
> 6. **`ToolCallMeta` + `ProviderInfo::tier`** — traitclaw
>    `Provider::model_info() -> ModelTier` parity, but through a
>    runtime-resolved path (`ModelTier` cheap to copy, no network).
>    Provider implements a `tier()` adapter that resolves once at
>    `initialize` time and caches the tier in `ModelConfig`.

Plus the standard `cargo run --example …` showcase expansions and
CHANGELOG entry for the R10 round.

## 1. Backlog Synthesis

| ID | Title | Source | ROI | Effort | Crates |
|---|---|---|---|---|---|
| **R10-1** | `ToolOutputDefinition` + `present_call` / `present_result` | dsh `tools/src/index.ts` ToolOutputDefinition | **High** | M (~350 LOC) | `synthia-tool/src/output.rs` (new) + `synthia-tool/src/registry.rs` (consume) + `synthia-tool/src/builtin/*.rs` (default impls) |
| **R10-2** | `Steering::for_tier(ModelTier)` + `Steering::auto` | traitclaw `crates/traitclaw-steering/src/steering.rs` `Steering::auto()/for_tier()` | **High** | M (~250 LOC) | `synthia-steering/src/tier.rs` (new) + `synthia-steering/src/steering.rs` (extend) |
| **R10-3** | `AdaptiveRegistry` + `TierLimits` | traitclaw `crates/traitclaw-core/src/registries.rs::AdaptiveRegistry` | **High** | M (~300 LOC) | `synthia-tool/src/registry/adaptive.rs` (new) + `synthia-tool/src/lib.rs` (re-export) |
| **R10-4** | `@agent:` text-routed delegation | traitclaw `crates/traitclaw-team/src/router.rs::LeaderRouter` | M-High | M (~280 LOC) | `synthia-agent/src/agent/router.rs` (new) + `synthia-agent/src/agent/re_act.rs` (intercept) |
| **R10-5** | `OperationSnapshot` wire-up to controller | R9-6 deferred + pi `harness/runtime/types.ts` `OperationState` | M | S (~200 LOC) | `synthia-session/src/operation.rs` (subscribe) + `synthia-server/src/session/controller.rs` (emit) |
| **R10-6** | `ToolCallMeta` + `ModelTier` cheap resolution | traitclaw `types/model_info.rs::ModelTier` | M | S (~150 LOC) | `synthia-provider/src/tier.rs` (new) + `synthia-provider/src/types/models.rs` (extend) |

## 2. Why These 6

R6-R9 closed:
- Trait surface (Guard / Hint / Tracker / Hook / HookMap / AgentHook / OutputTransformer / EffectGate).
- Runtime neutrality (CancelToken trait, futures mpsc, chrono).
- ProviderProfile (typed provider config).
- SkillProvider + SkillApplication (layered registry + structured I/O).
- OperationSnapshot type (deferred wire-up).

R10 closes the **rendering** axis (tool output contract), the **tier
auto-config** axis (Steering::for_tier + AdaptiveRegistry), the
**multi-agent routing** axis (`@agent:` text syntax), and finishes the
**read-only observability surface** (OperationSnapshot wire-up). These
are the 6 highest-ROI gaps from the cross-reference survey that R9
deliberately deferred.

Items kept out of scope for R10:
- **R10-7** Full dsh `Cordis`-style kernel rewrite (large; multi-round).
- **R10-8** McpServer / McpClient (traitclaw) — R9-H still deferred.
- **R10-9** RAG (traitclaw HybridRetriever) — R9-I still deferred.
- **R10-10** Workflow DSL (dsh / pi-subagents) — TS-only.
- **R10-11** `attachment/AttachmentStore` (dsh) — large surface; needs
  redesign of `ImageContent` + persistence; deferred to R11.
- **R10-12** Full pi `Lane` reducer + 13-leaf `OperationState`
  (R5-13 / R6-9 / R9-C). OperationSnapshot is the read-only subset.

## 3. Sequencing

| Phase | Items | Risk |
|---|---|---|
| R10.A | R10-6 (ModelTier) | Low (additive type) |
| R10.B | R10-2 (Steering::for_tier) | Low (additive facade) |
| R10.C | R10-3 (AdaptiveRegistry) | Low (additive wrapper) |
| R10.D | R10-1 (ToolOutputDefinition) | Medium (touches Tool trait shape) |
| R10.E | R10-4 (`@agent:` delegation) | Medium (new intercept seam) |
| R10.F | R10-5 (OperationSnapshot wire-up) | Low (additive channel) |
| Showcase | 2 new examples + README expansion | Low |

## 4. Acceptance Gates

- `cargo check --workspace --all-features --all-targets` 0 errors
- `cargo clippy --all-targets --all-features --tests --all -- -D warnings` 0 warnings
- `cargo +nightly fmt --all -- --check` 0 diff
- Per-crate test baselines hold + new tests pass:
  - `cargo test -p synthia-tool --lib` (+12+ present_/registry tests)
  - `cargo test -p synthia-steering --lib` (+6+ tier tests)
  - `cargo test -p synthia-agent --lib` (+10+ router tests)
  - `cargo test -p synthia-session --lib` (+4+ snapshot tests)
  - `cargo test -p synthia-provider --lib` (+5+ tier tests)
- `cargo run --example assemble_with_tier_steering -p synthia-agent` end-to-end
- `cargo run --example assemble_with_adaptive_registry -p synthia-agent` end-to-end

## 5. Risk Register

| ID | Risk | Mitigation |
|---|---|---|
| R10-1 | Default `present_call`/`present_result` impls must be sound for every existing tool | Provide a `ToolOutputDefinition::default()` that round-trips JSON values; only tools with a custom `present_*` opt in. `Tool` trait grows an `output_definition()` method with a default impl that returns `ToolOutputDefinition::passthrough()`. |
| R10-3 | AdaptiveRegistry must not deadlock the registry's existing invariants | AdaptiveRegistry wraps an `Arc<dyn ToolRegistry>` and exposes only the read-side; writes flow through the inner registry. `tier()` is resolved at the wrap site so the inner registry stays tier-agnostic. |
| R10-4 | `@agent:` parsing must not collide with legitimate user text | Use a configurable trigger (default `@agent:` like traitclaw) and require the prefix at start of an assistant turn OR a line-prefixed mention with explicit `\n@agent:` boundary; ambiguous matches fall through. |
| R10-5 | Snapshot channel could leak if a consumer drops before subscribing | `subscribe()` returns a bounded mpsc receiver; controller emits with `try_send` and drops on Lagged (tested). |

## 6. Out of Scope (R11+ backlog)

- R10-7 dsh Cordis-style kernel (multi-round large refactor).
- R10-8 MCP integration.
- R10-9 RAG.
- R10-10 Workflow DSL.
- R10-11 AttachmentStore / multimodal.
- R10-12 Full Lane reducer + 13-leaf OperationState.
- R10-13 dsh subagent drivers (codex / claude-code / acp / fork).
- R10-14 pi `effects.ts` tool runtime.

## 7. Reference Evidence Map

- R10-1: dsh `packages/core/tools/src/index.ts:144-168` ToolOutputDefinition; traitclaw
  `crates/traitclaw-core/src/transformers.rs:184-258` `ProgressiveTransformer`.
- R10-2: traitclaw `crates/traitclaw-steering/src/steering.rs:42-66, 100-108`.
- R10-3: traitclaw `crates/traitclaw-core/src/registries.rs:373-457`.
- R10-4: traitclaw `crates/traitclaw-team/src/router.rs` `@agent:` syntax; pi-subagents
  `src/mention.ts` `@handle` grammar.
- R10-5: pi `packages/agent/src/harness/runtime/types.ts` (read-only OperationState
  subset) + opencode `session/session.ts` snapshot pattern.
- R10-6: traitclaw `crates/traitclaw-core/src/types/model_info.rs:11-25`
  `ModelTier::Small/Medium/Large`.
