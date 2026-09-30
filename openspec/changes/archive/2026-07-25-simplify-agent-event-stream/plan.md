# simplify-agent-event-stream Implementation Plan

> **For agentic workers:** Use subagent-driven-development to implement
> this plan task-by-task. Each `## Task N` here corresponds to a checkbox
> in `tasks.md`. The plan expands each task into TDD-style micro-steps.

**Goal:** Restructure `AgentEvent` from 32 variants to 5 top-level variants, fix the silent reasoning-delta bug, fix Anthropic signature handling, and switch wire mapping to A2A `Part::data` typed JSON.

**Architecture:** Provider layer first (signature handling), then Agent enum restructuring, then StreamAccumulator, then is_durable rewrite, then mapping rewrite, then SSE/server fixture updates, then frontend, then spec/validation.

**Tech Stack:** Rust 1.85, Tokio, serde, a2a-types, existing crate layout (`synthia-provider`, `synthia-agent`, `synthia-a2a`, `synthia-server`, `synthia-event-v2`, `synthia-web`).

---

## Task 1.1: Add `ReasoningContent { text, signature }` struct

**Files:**
- `crates/synthia-provider/src/types/content.rs`

**Steps:**

1. Open `crates/synthia-provider/src/types/content.rs` and locate the `TextContent` struct definition.
2. Add new struct below it:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
   pub struct ReasoningContent {
       pub text: String,
       #[serde(skip_serializing_if = "Option::is_none", default)]
       pub signature: Option<String>,
   }
   ```
3. Verify `cargo build -p synthia-provider` passes.
4. Commit: `feat(provider): add ReasoningContent { text, signature }`.

## Task 1.2: Switch `ContentPart::Reasoning` to wrap `ReasoningContent`

**Files:**
- `crates/synthia-provider/src/types/content.rs`
- All call sites that construct `ContentPart::Reasoning`

**Steps:**

1. In `content.rs`, change enum variant from `Reasoning(TextContent)` to `Reasoning(ReasoningContent)`.
2. Run `cargo build -p synthia-provider` and collect compile errors.
3. For each error site, update the construction:
   - `ContentPart::Reasoning(TextContent { text: t })` → `ContentPart::Reasoning(ReasoningContent { text: t, signature: None })`
4. Verify `cargo build --workspace` passes.
5. Commit: `feat(provider): switch ContentPart::Reasoning to ReasoningContent`.

## Task 1.3: Add `SamplingResult.reasoning_signature`

**Files:**
- `crates/synthia-agent/src/events/stream.rs` (or wherever `SamplingResult` is defined)
- All construction sites

**Steps:**

1. Locate `SamplingResult` definition.
2. Add field `pub reasoning_signature: Option<String>`.
3. Build, collect errors, update each construction site to set the field (default `None`).
4. Commit: `feat(agent): SamplingResult carries reasoning_signature`.

## Task 1.4: Anthropic v2 streaming parses `signature_delta`

**Files:**
- `crates/synthia-provider/src/streaming/anthropic/v2.rs`

**Steps:**

1. Locate the delta-type dispatch (`"thinking_delta"`, `"text_delta"`, etc.).
2. Add a `"signature_delta"` arm:
   ```rust
   "signature_delta" => {
       self.last_reasoning_signature = delta.signature.clone();
       // Do not emit a stream chunk; signature is folded in on finalize.
   }
   ```
3. On the streaming accumulator, ensure the last reasoning chunk retains the signature:
   - When emitting `ContentPart::Reasoning`, propagate `self.last_reasoning_signature.clone()` if set.
4. Write a test in `v2.rs` (or its test module) that:
   - Streams a synthetic Anthropic sequence with thinking + signature_delta
   - Asserts the final aggregated `SamplingResult.reasoning_signature == Some(sig)`
5. Run `cargo test -p synthia-provider`.
6. Commit: `fix(provider): Anthropic signature_delta attaches to ReasoningContent`.

## Task 1.5: Anthropic v2 finalize aggregates signature into SamplingResult

**Files:**
- Same as 1.4

**Steps:**

1. In the streaming finalize path, ensure the final `SamplingResult.reasoning_signature` carries the latest non-None signature seen.
2. Extend the test from 1.4 to assert this.
3. Commit: included in 1.4.

## Task 1.6: Provider multi-turn signature round-trip test

**Files:**
- `crates/synthia-provider/tests/` (new file if needed)

**Steps:**

1. Write a test that:
   - Calls complete() once with a thinking+signature response
   - Calls complete() again with the prior messages including the signed reasoning block
   - Asserts the second call's message history preserves the signature
2. Run `cargo test -p synthia-provider`.
3. Commit: `test(provider): multi-turn reasoning signature preservation`.

## Task 1.7: Provider non-Anthropic leaves signature None

**Files:**
- `crates/synthia-provider/src/openai_streaming/processor.rs`

**Steps:**

1. Verify OpenAI streaming does not touch the signature field.
2. Add a regression test that OpenAI reasoning has `signature: None`.
3. Commit: `test(provider): OpenAI reasoning signature defaults to None`.

---

## Task 2.1: Rewrite `event_enum.rs` with 5 top-level variants

**Files:**
- `crates/synthia-agent/src/events/event_enum.rs`

**Steps:**

1. Replace the file contents with:
   ```rust
   pub enum AgentEvent {
       Model(ContentPart),
       ModelDone(SamplingResult),
       System(SystemEvent),
       Agent(AgentMeta, Box<AgentEvent>),
       Hook(HookEvent),
   }
   ```
2. Define sub-enums in the same file (or split into separate module).
3. Run `cargo build -p synthia-agent` and collect errors. Every legacy variant becomes a compile error.
4. Commit: `refactor(agent): AgentEvent 32→5 top-level variants (WIP)`.

## Task 2.2: Add SystemEvent, WarningKind, AgentMeta, HookEvent

**Files:**
- `crates/synthia-agent/src/events/event_enum.rs`

**Steps:**

1. Define `SystemEvent` with all 7 variants per `specs/agent-event-bus/spec.md`.
2. Define `WarningKind` with 6 variants.
3. Define `SessionEndReason` (preserve existing 8 variants).
4. Define `AgentMeta { parent_session_id, child_session_id, parent_depth }`.
5. Define `HookEvent` with 4 variants (`Message`, `ConfirmRequest`, `ConfirmResponse`, `Custom`).
6. ConfirmRequest adds `tool_use_id: String` field.
7. Commit: included in 2.1.

## Task 2.3: SystemEvent::Recovery variant

**Files:**
- `crates/synthia-agent/src/events/event_enum.rs`

**Steps:**

1. Add `Recovery { level_number: u32, tool_name: Option<String>, message: String, iteration: Option<usize> }`.
2. Commit: included in 2.1.

## Task 2.4: SystemEvent::Usage variant

**Files:**
- `crates/synthia-agent/src/events/event_enum.rs`

**Steps:**

1. Add `Usage { input_tokens: usize, output_tokens: usize, cache_read_tokens: Option<usize>, cache_creation_tokens: Option<usize> }`.
2. Commit: included in 2.1.

## Task 2.5: Update all construction sites in agent code

**Files:** all of `crates/synthia-agent/src/`

**Steps:**

1. Run `cargo build -p synthia-agent` to enumerate error sites.
2. For each error, map the legacy variant to the new shape per the proposal's mapping table.
3. Pay special attention to:
   - `tool_execution.rs` — `ToolCallStarted/Completed/Error/Skipped`
   - `error_recovery.rs` — `RecoveryApplied` → `System(SystemEvent::Recovery)`
   - `subagent.rs` / `factory.rs` — `Subagent*` → `Agent(meta, Box<new(inner))`
   - `context_compact.rs` — `ContextCompacted` → `System(Warning { kind: ContextCompaction })`
   - `reflection.rs` — `SelfReflection` → `Model(ContentPart::ToolUse { name: "self_reflection", ... })`
   - `hooks.rs` — `HookError` → `System(Warning { kind: Hook })`
   - `guardian.rs` — `GuardianWarning`, `GuardianConfirmationRequest` → `System(Warning { kind: Guardian })` and `Hook(ConfirmRequest)`
   - `loop_detect.rs` — `LoopWarning` → `System(Warning { kind: Loop })`
   - `token_budget.rs` — `TokenBudgetWarning/Notice` → `System(Warning { kind: TokenBudget })`
   - `stream.rs` — drop `Finish`; replace with `System(SessionEnded)`
   - `iteration.rs` — drop `IterationStarted/Completed`
   - `recovery_cascade.rs` — drop `Checkpoint`, `StateChange`
   - `status.rs` — drop `Status(AgentStatus)` (already not constructed)
4. After every error site is resolved, `cargo build -p synthia-agent` passes.
5. Commit: `refactor(agent): migrate construction sites to new AgentEvent`.

## Task 2.6: Delete dead variants

**Files:** `event_enum.rs`

**Steps:**

1. After 2.5, ensure no `Finish`, `Status`, `SelfReflection`, `IterationStarted`, `IterationCompleted`, `Checkpoint`, `StateChange` names remain anywhere in the agent crate.
2. `rg -n "Finish|Status|SelfReflection|IterationStarted|IterationCompleted|Checkpoint|StateChange" crates/synthia-agent/src` returns no matches.
3. Commit: included in 2.5.

---

## Task 3.1: StreamAccumulator deltas: Vec<ContentPart>

**Files:**
- `crates/synthia-agent/src/events/stream.rs`

**Steps:**

1. Replace `text_deltas: Vec<String>` with `deltas: Vec<ContentPart>`.
2. Update `ingest()`: in every arm (Text, Reasoning, ToolUse, Image, Audio, Resource), push the cloned `ContentPart` to `self.deltas`.
3. Verify `cargo build -p synthia-agent` passes.
4. Commit: `refactor(agent): StreamAccumulator deltas is Vec<ContentPart>`.

## Task 3.2: StreamAccumulator push every variant

**Files:** same as 3.1

**Steps:**

1. Walk every `ingest` arm and confirm push to `self.deltas`.
2. Commit: included in 3.1.

## Task 3.3: Wire reasoning_signature collection

**Files:** same as 3.1

**Steps:**

1. In `ingest()`'s `ContentPart::Reasoning(rc)` arm, set `self.reasoning_signature = rc.signature.or(self.reasoning_signature.clone());`.
2. Commit: included in 3.1.

## Task 3.4: StreamAccumulator::finalize returns SamplingResult with reasoning_signature

**Files:** same as 3.1

**Steps:**

1. In `finalize()`, populate `SamplingResult.reasoning_signature` from `self.reasoning_signature.take()`.
2. Commit: included in 3.1.

---

## Task 4.1: Rewrite `AgentEvent::is_durable()` as explicit match

**Files:**
- `crates/synthia-agent/src/events/event_enum.rs`

**Steps:**

1. Replace the body with explicit match:
   ```rust
   pub fn is_durable(&self) -> bool {
       match self {
           Self::Model(ContentPart::Text(_))
           | Self::Model(ContentPart::ToolUse(_))
           | Self::Model(ContentPart::ToolResult(_))
           | Self::Model(ContentPart::Resource(_)) => true,
           _ => false,
       }
   }
   ```
2. Run `cargo build --workspace` and resolve errors in callers that imported removed variant names.
3. Commit: `refactor(agent): is_durable explicit match on 5 variants`.

## Task 4.2: Update synthia-event-v2 whitelist consumer

**Files:**
- `crates/synthia-event-v2/src/...`

**Steps:**

1. Find the function that consumed the legacy whitelist string.
2. Replace with call to `AgentEvent::is_durable(&event)`.
3. Remove the whitelist string literal entirely.
4. Commit: `refactor(event-v2): consume is_durable() instead of whitelist`.

## Task 4.3: Remove unknown = durable safe default

**Files:** same as 4.2

**Steps:**

1. After 4.2, ensure no fallback returns true for unknown variant strings.
2. The new match is exhaustive — there are no unknowns.
3. Commit: included in 4.2.

## Task 4.4: Update is_durable_event_type callers

**Files:** all of `crates/synthia-event-v2/src/`

**Steps:**

1. `rg "is_durable_event_type"` and update each call site to use `event.is_durable()`.
2. Commit: included in 4.2.

---

## Task 5.1-5.8: Rewrite mapping.rs

**Files:**
- `crates/synthia-a2a/src/mapping.rs`

**Steps:**

1. Replace the top-level `match` with 5-arm dispatch on `Model | ModelDone | System | Agent | Hook`.
2. Each arm delegates to a sub-helper:
   - `map_model(parts) -> Vec<StreamResponse>` (Text/Reasoning/ToolUse/ToolResult/Image/Audio/Resource)
   - `map_model_done(sampling) -> Vec<StreamResponse>`
   - `map_system(sys) -> Vec<StreamResponse>` (returns StatusUpdate for Session*, Part::data for others)
   - `map_agent(meta, inner) -> Vec<StreamResponse>` (multi-part)
   - `map_hook(h) -> Vec<StreamResponse>`
3. Replace every `Part::text("") + metadata.segment_type` with `Part::data(json!({ kind: <name>, ... }))`.
4. Fix the `mapping.rs:30` doc comment: `SessionInterrupted → Canceled` → `InputRequired`.
5. Map `SessionEndReason` to A2A `TaskState` per spec.
6. Delete the legacy mapping test that referenced old variant names; write new tests that assert Part::data shape and kind discriminator.
7. Run `cargo test -p synthia-a2a`.
8. Commit: `refactor(a2a): Part::data wire mapping for 5-variant AgentEvent`.

---

## Task 6.1-6.3: Update server SSE variant enumeration

**Files:**
- `crates/synthia-server/src/sse.rs`
- `crates/synthia-server/src/state/subagent_factory.rs`

**Steps:**

1. In `sse.rs`, update each match arm to use new variant names (5 + sub).
2. Delete any arm that referenced removed variant names.
3. In `subagent_factory.rs:184`, change `Ok(AgentEvent::Finish { output })` to `Ok(AgentEvent::System(SystemEvent::SessionEnded(reason)))` and read the output from the SessionEnded payload.
4. `cargo build -p synthia-server`.
5. Commit: `refactor(server): SSE variant enumeration matches new schema`.

---

## Task 7.1-7.5: Frontend dispatch migration

**Files:**
- `synthia-web/src/**` (dispatch logic files identified during grep)

**Steps:**

1. `rg "segment_type" synthia-web/src` to find all current dispatch sites.
2. For each site, replace string comparison with `JSON.parse(part.data).kind` against the new kind values from the spec.
3. Update TypeScript types to reflect new wire schema.
4. Manual smoke test: reasoning visible in UI, tool merge works, status updates correct.
5. Commit: `refactor(web): dispatch on Part::data kind instead of metadata.segment_type`.

---

## Task 8.1-8.8: Test fixture updates

**Files:**
- `crates/synthia-agent/tests/e2e_llm_test.rs`
- `crates/synthia-agent/tests/e2e_cli_test.rs`
- `crates/synthia-server/tests/e2e_registry_pipeline_test.rs`
- `crates/synthia-cli/src/repl_core/repl/format_event.rs`
- `crates/synthia-cli/src/repl_core/repl/state.rs`

**Steps:**

1. For each file, replace legacy variant names in `matches!` patterns.
2. Replace `Finish` references with `System(SessionEnded(...))`.
3. Replace `Status(_)` references with `System(SessionEnded(...))` (or omit, since it no longer exists).
4. `cargo test --workspace`.
5. Commit per file: `test: update <file> to new AgentEvent`.

## Task 8.6: New test — Anthropic multi-turn signature

**Files:**
- `crates/synthia-provider/tests/anthropic_signature.rs` (new)

**Steps:**

1. Write a synthetic streaming sequence with thinking + signature_delta, then a follow-up complete call.
2. Assert signature is preserved.
3. `cargo test -p synthia-provider`.
4. Commit: `test(provider): Anthropic signature multi-turn preservation`.

## Task 8.7: New test — Part::data stability

**Files:**
- `crates/synthia-a2a/src/mapping.rs` (test module)

**Steps:**

1. For each of the 5 top-level variants, write a test that asserts the resulting A2A Message contains exactly `Part::data({ kind, ...payload })` with the documented kind value.
2. `cargo test -p synthia-a2a`.
3. Commit: `test(a2a): Part::data kind discriminator per variant`.

## Task 8.8: New test — is_durable per path

**Files:**
- `crates/synthia-agent/src/events/event_enum.rs` (test module)

**Steps:**

1. For every documented path, assert `event.is_durable()` returns the expected bool per the spec table.
2. `cargo test -p synthia-agent`.
3. Commit: `test(agent): is_durable returns correct value for every path`.

---

## Task 9.1-9.8: Confirm spec revisions match implementation

**Files:** all 6 modified + 2 new specs

**Steps:**

1. Read each spec file in `openspec/changes/simplify-agent-event-stream/specs/` and the corresponding existing spec in `openspec/specs/`.
2. Confirm delta sections use exact normalized requirement headers.
3. Confirm every requirement has at least one scenario with exactly 4 hashtags.
4. Confirm SHALL/MUST language on every requirement.
5. Commit: included as part of `openspec archive` later.

---

## Task 10.1-10.6: Workspace validation

**Steps:**

1. `cargo build --workspace`.
2. `cargo test --workspace`.
3. `cargo clippy --workspace --all-targets -- -D warnings`.
4. Memory replay parity test:
   - Pick a saved session fixture.
   - Run replay under old is_durable logic and new is_durable match.
   - Assert the produced durable event sets differ only in the documented ways (no Reasoning/Image/Audio; no ModelDone; no System; no Hook; no Agent). Text/ToolUse/ToolResult/Resource must be identical.
5. `pnpm --filter synthia-web build` (or whatever the web build command is — discover during execution).
6. Manual smoke test: chat with reasoning visible, tool merge works, session interruption transitions to InputRequired.

---

## Verification & Commit Strategy

- Commit per task group (1, 2, 3, 4, 5, 6, 7, 8, 10).
- Spec updates (9) are committed alongside the change archive, not as separate commits.
- After all tasks pass, run `openspec archive simplify-agent-event-stream` to fold the specs into `openspec/specs/`.