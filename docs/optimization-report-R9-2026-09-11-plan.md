# Synthia R9 — Provider Profile + Skill Provider Registry + Runtime-Neutral Spawn

> Synthia is already in a strong state (R6/R7/R8 落地, 1594 tests passing).
> R9 is a **focused 6-item round** that closes the remaining real-world
> lego-composition gaps surfaced by re-auditing the four reference projects
> (traitclaw / dsh / pi / pi-subagents):
>
> 1. **`ProviderProfile` typed union** — replace the string-keyed provider
>    map (`WorkspaceConfig.providers: HashMap<String, ProviderEntry>`)
>    with a **single typed `ProviderProfile` enum** so lib consumers can
>    wire Anthropic / OpenAI / OpenAI-compat / Stub / custom profiles
>    with **zero string lookups** and full schema-level type safety.
> 2. **`SkillProvider` registry (dsh parity)** — `SkillRegistry` becomes
>    a layered registry of `SkillProvider` trait impls; project / user /
>    runtime skills are pluggable providers; lib consumers can register
>    their own providers.
> 3. **`SkillApplicationContext` for runtime skills** — model-invocable
>    skills gain an `apply(ctx, args)` lifecycle (dsh parity) so a skill
>    is no longer "read a markdown file"; it's a structured request →
>    structured response.
> 4. **`HookMap` gains `&AgentState` access** — `HookHandler` signature
>    becomes `(event: HookEvent, state: &AgentState) -> HookDecision`
>    so handlers can make decisions off the live runtime state, not just
>    the event payload.
> 5. **`spawn_detached` for background subagents** — `AgentHandle::run`
>    stays blocking (cancel-the-wait / pi-subagents parity *with a
>    semantic twist*); `spawn_detached` returns immediately with a
>    handle for background work. Cancellation owns the spawn token
>    (cancel-the-work semantics) — same as pi-subagents `abortable`
>    semantic, but **only on `spawn_detached`**.
> 6. **`OperationSnapshot` flat status record** — pi's flat
>    `OperationState` 13-leaf was *deferred* (R5-13 / R6-9), but the
>    **read-only snapshot** a session reports to consumers is a clear
>    win: a single `OperationSnapshot { state, error, usage, ... }` that
>    describes a run for HTTP `/status` / TUI / replay consumers
>    without exposing the ReActLoop internals.

Plus the standard `cargo run --example …` showcase expansions for the
two new lego pieces.

## 1. Backlog Synthesis

| ID | Title | Source | ROI | Effort | Crates |
|---|---|---|---|---|---|
| **R9-1** | `ProviderProfile` typed union | dsh `packages/llm/` profile/preset pattern | **High** | M (~250 LOC) | `synthia-provider/src/profile.rs` (new) + `synthia-provider/src/config.rs` (refactor) + `synthia-server/src/server/provider_factory.rs` (consume) |
| **R9-2** | `SkillProvider` trait + `SkillRegistry::register_provider` | dsh `packages/skill/skill/src/index.ts` | **High** | M (~400 LOC) | `synthia-skill/src/registry.rs` (new) + `synthia-skill/src/skill.rs` (extend) |
| **R9-3** | `SkillApplicationContext` + `apply()` lifecycle | dsh `SkillDefinition` + Skill provider pattern | M | M (~250 LOC) | `synthia-skill/src/application.rs` (new) |
| **R9-4** | `HookMap` handlers receive `&AgentState` | pi `harness/hooks.ts` + synthia R6-4 | **High** | S (~80 LOC) | `synthia-steering/src/hook_map.rs` + `synthia-steering/src/lib.rs` re-export |
| **R9-5** | `AgentHandle::spawn_detached` + detached-subagent tool | pi-subagents `abortable.ts` + `SubagentExit.status = "detached"` | M-High | M (~300 LOC) | `synthia-agent/src/agent/handle.rs` (new) + `synthia-agent/src/agent/delegation.rs` (extend) |
| **R9-6** | `OperationSnapshot` flat status record | pi `harness/runtime/types.ts` (OperationState 13-leaf, read-only subset) + opencode session read pattern | M | S (~200 LOC) | `synthia-session/src/operation.rs` (new) |

## 2. Why These 6

R6 / R7 / R8 closed the **trait-surface** and **runtime-neutrality**
axes — ReActLoop now has typed event producers, BlockAssembler, HookMap,
Arc-shared contexts, atomic cancel tokens, chrono time, and a runnable
assemble-from-scratch tutorial. R9 closes the **configuration** axis
(`ProviderProfile`), the **extension** axis (`SkillProvider` registry
+ `apply()` lifecycle), and adds **two new runtime-neutral patterns**
that downstream consumers asked for in spirit:

- `ProviderProfile` makes `synthia-provider` lego-composable by
  configuration — a lib consumer chooses `OpenAIProfile` /
  `AnthropicProfile` / `OpenAICompatProfile::custom(name, base)` /
  `StubProfile::text_only()` from one typed enum, not from a string-keyed
  map. R9 is the first round that touches `synthia-provider`'s config
  surface.
- `SkillProvider` registry adopts dsh's **layered registry** pattern
  (project / user / runtime / custom providers) — current synthia
  discovery is a hard-coded `discovery::discover_skills` function;
  R9 turns discovery into one of multiple pluggable providers.
- `HookMap` handlers can't currently read `AgentState`; pi's hook
  pattern explicitly passes the harness state. R9-4 makes the handler
  signature state-aware, which is the prerequisite for **real** dynamic
  guards (e.g. "if `state.iteration_index > 5`, block tool calls").
- `spawn_detached` is the **only** pi-subagents pattern that's actually
  missing from synthia — R7 concluded pi-subagents is otherwise aligned.
- `OperationSnapshot` is the **observability subset** of pi's larger
  Lane model — R9 lands the **flat, read-only** surface that consumers
  want without dragging the reducer in.

R5-11 / R6-9 (full Lane model + flat OperationState 13-leaf + ReActLoop
reducer rewrite) remain explicitly out of scope — R9 doesn't try to
resurrect the Lane reducer, just give consumers a clean read-side view.

## 3. Sequencing

| Phase | Items | Risk |
|---|---|---|
| R9.A | R9-1 (ProviderProfile) | Low (additive config surface) |
| R9.B | R9-2 (SkillProvider registry) | Low (additive) |
| R9.C | R9-3 (SkillApplicationContext) | Medium (touches Skill public API) |
| R9.D | R9-4 (HookMap state) | Low (signature widening) |
| R9.E | R9-5 (spawn_detached) | Medium (new tool seam) |
| R9.F | R9-6 (OperationSnapshot) | Low (additive type) |
| Showcase | 2 new examples + README section | Low |

## 4. Acceptance Gates

- `cargo check --workspace --all-features --all-targets` 0 errors
- `cargo clippy --all-targets --all-features --tests --all -- -D warnings` 0 warnings
- `cargo +nightly fmt --all -- --check` 0 diff
- Per-crate test baselines hold + new tests pass:
  - `cargo test -p synthia-provider --lib` (+15+ ProviderProfile tests)
  - `cargo test -p synthia-skill --lib` (+20+ SkillProvider / apply tests)
  - `cargo test -p synthia-steering --lib` (+5+ state-aware HookMap tests)
  - `cargo test -p synthia-agent --lib` (+10+ spawn_detached tests)
  - `cargo test -p synthia-session --lib` (+8+ OperationSnapshot tests)
- `cargo run --example assemble_with_provider_profile -p synthia-agent` — runs end-to-end
- `cargo run --example assemble_with_skills -p synthia-agent` — runs end-to-end

## 5. Risk Register

| ID | Risk | Mitigation |
|---|---|---|
| R9-1 | Existing `WorkspaceConfig.providers` map is consumed by `synthia-server`'s `provider_factory.rs`; refactoring the config breaks server startup | Keep `ProviderEntry::from_profile(&profile)` + `ProviderEntry::to_profile()` round-trip; server migration uses the new API surface but old `ProviderEntry` config files continue to load (deserialize → re-wrap to profile). |
| R9-3 | Adding `apply()` to `Skill` changes its public shape | Make `apply` an opt-in `Skill::with_application(ApplicationContext)` builder; `Skill` stays `Clone + Deserialize` for backward compat. |
| R9-5 | `spawn_detached` could leak child sessions if dropped | `AgentHandle::detach()` returns `DetachedAgent { join: JoinHandle, cancel: Arc<dyn CancelToken> }`; Drop on the handle explicitly fires cancel and **does NOT** abort work; lib consumers must call `cancel().await` or `join().await` to clean up. |
| R9-6 | `OperationSnapshot` might leak ReActLoop internals | `OperationSnapshot` is a **read-only** struct constructed only by `ReActLoop`; consumers receive a snapshot via `&OperationSnapshot` on a public channel; no internal locks exposed. |

## 6. Out of Scope (deferred to R10+)

- **R9-7** Full pi `Lane` reducer + 13-leaf `OperationState` (R5-13 / R6-9
  intentionally deferred; R9-6 is the read-only snapshot, R10+ lands the
  reducer).
- **R9-8** Per-session sandbox mode override + capability-neutral policy
  (dsh `sandbox-policy`). R4 §153 deferred; R10+.
- **R9-9** Workflow DSL (dsh). TS-only.
- **R9-10** McpServer / McpClient (traitclaw). External MCP ecosystem
  adoption unknown in synthia's user base.
- **R9-11** RAG (traitclaw HybridRetriever + RagContextManager). High ROI
  but separate PR.
- **R9-12** SessionPreparation / preview-fork (dsh). Use case unclear.
- **R9-13** `agent-loop.ts` style multi-iteration reducer (pi).
- **R9-14** dsh `codex` / `claude-code` / `acp` external subagent drivers.
- **R9-15** pi `effects.ts` effects-based tool runtime. Synthia uses the
  simpler Stream-based runtime; effects model would require a
  full rewrite of `synthia-tool`.

## 7. Reference Evidence Map

- R9-1: dsh `packages/llm/llm/src/profile.ts` + `preset` pattern
- R9-2: dsh `packages/skill/skill/src/index.ts:248-268, 357-661`
  (`SkillProvider` interface + `SkillLayer` + `SkillRegistry` +
  `BUNDLED_SKILL_RANK` / `RUNTIME_RANK` precedence)
- R9-3: dsh `SkillDefinition` + `SkillRegistration` + `renderSkillContent`
- R9-4: pi `packages/agent/src/harness/hooks.ts` (handler receives
  harness state) + synthia R6-4 `HookHandler` signature
- R9-5: pi-subagents `src/abortable.ts` (cancel-the-wait) +
  `agent-runner.ts` (`spawnOptions.isBackground`)
- R9-6: pi `packages/agent/src/harness/runtime/types.ts` (read-only
  `OperationState` 13-leaf subset) + opencode
  `session/session.ts` (snapshot)