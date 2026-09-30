# Tasks: simplify-agent-event-stream

## 1. Provider layer: ReasoningContent + signature handling

- [x] 1.1 Add `ReasoningContent { text, signature }` struct in `crates/synthia-provider/src/types/content.rs`
- [x] 1.2 Change `ContentPart::Reasoning(TextContent)` to `ContentPart::Reasoning(ReasoningContent)` and update all constructions
- [x] 1.3 Add `SamplingResult.reasoning_signature: Option<String>` field
- [x] 1.4 Anthropic v2 streaming: parse `signature_delta` and attach to most recent ReasoningContent chunk
- [x] 1.5 Anthropic v2 streaming: aggregate final `reasoning_signature` into SamplingResult on finalize
- [x] 1.6 Provider tests: multi-turn reasoning round-trip with signature preserved
- [x] 1.7 Provider tests: OpenAI streaming leaves signature None

## 2. AgentEvent enum restructuring

- [x] 2.1 Rewrite `crates/synthia-agent/src/events/event_enum.rs` with 5 top-level variants
- [x] 2.2 Add `SystemEvent`, `WarningKind`, `AgentMeta`, `HookEvent` sub-enums
- [x] 2.3 Add `Recovery` variant under `SystemEvent` with `level_number, tool_name, message, iteration`
- [x] 2.4 Add `Usage` variant under `SystemEvent` with token usage fields
- [x] 2.5 Update all construction sites in agent loop, tool execution, recovery, subagent, hook paths
- [x] 2.6 Delete dead variants: `Finish`, `Status`, `SelfReflection`, `IterationStarted`, `IterationCompleted`, `Checkpoint`, `StateChange`

## 3. StreamAccumulator migration

- [x] 3.1 Change `text_deltas: Vec<String>` to `deltas: Vec<ContentPart>` in `crates/synthia-agent/src/stream_builder/steps/sample/stream.rs` (note: file lives in `stream_builder/steps/sample/`, not `events/`)
- [x] 3.2 Push the ContentPart to `deltas` in every `handle_chunk` Content arm (Text, Reasoning, ToolUse, Image, Audio, ToolResult, Resource)
- [x] 3.3 Wire `reasoning_signature` collection alongside `reasoning` accumulation
- [x] 3.4 Update `finalize()` to return `SamplingResult` with `reasoning_signature`

## 4. is_durable() rewrite

- [x] 4.1 Rewrite `AgentEvent::is_durable()` in `event_enum.rs` as explicit match on the 5 variants
- [x] 4.2 Update `crates/synthia-event-v2` whitelist consumer to call the new match
- [x] 4.3 Remove legacy "unknown = durable" safe-default fallback (no longer needed)
- [x] 4.4 Update any references to removed variant names in `is_durable_event_type` callers

## 5. mapping.rs Part::data rewrite

- [x] 5.1 Rewrite `crates/synthia-a2a/src/mapping.rs` top-level match on 5 variants
- [x] 5.2 Translate `Model(ContentPart)` cases to `Part::data({kind: "<variant>", ...})`
- [x] 5.3 Translate `SystemEvent::*` cases: SessionStarted/Ended/Interrupted → StatusUpdate; others → Part::data
- [x] 5.4 Translate `Agent(meta, inner)` to multi-part message: Part::data(meta) + Part::data(inner data)
- [x] 5.5 Translate `Hook(HookEvent::*)` to Part::data with documented kind values
- [x] 5.6 Delete `Part::text("")` marker hack at former lines 102-106
- [x] 5.7 Fix doc comment at former line 30: `SessionInterrupted → Canceled` → `InputRequired`
- [x] 5.8 Map `SessionEndReason` variants to A2A TaskState per the spec
- [x] 5.9 Update internal mapping tests to assert Part::data shape

## 6. Server SSE variant enumeration

- [x] 6.1 Update `crates/synthia-server/src/sse.rs` variant name strings to match new 5-variant names
- [x] 6.2 Remove dead variant names from the match arm listing
- [x] 6.3 Update `crates/synthia-server/src/state/subagent_factory.rs:184` to use SessionEnded instead of Finish

## 7. Frontend dispatch migration

- [x] 7.1 Replace `metadata.segment_type` string dispatch in `synthia-web/src/` with `JSON.parse(part.data).kind`
- [x] 7.2 Update TypeScript type definitions for new wire schema
- [x] 7.3 Verify reasoning content is rendered (was previously lost)
- [x] 7.4 Verify tool_call/tool_result merge still works with new schema
- [x] 7.5 Verify StatusUpdate mapping (SessionInterrupted → InputRequired) reaches UI

## 8. Test fixture updates

- [x] 8.1 Update `crates/synthia-agent/tests/e2e_llm_test.rs` variant matches
- [x] 8.2 Update `crates/synthia-agent/tests/e2e_cli_test.rs` variant matches
- [x] 8.3 Update `crates/synthia-server/tests/e2e_registry_pipeline_test.rs` variant matches
- [x] 8.4 Update `crates/synthia-cli/src/repl_core/repl/format_event.rs` variant matches
- [x] 8.5 Update `crates/synthia-cli/src/repl_core/repl/state.rs` variant matches
- [x] 8.6 Add new tests: Anthropic multi-turn signature preservation
- [x] 8.7 Add new tests: Part::data wire schema stability
- [x] 8.8 Add new tests: is_durable() returns correct value for every path

## 9. Spec revisions

- [x] 9.1 Confirm `openspec/specs/event-durability-classification/spec.md` revisions match implementation
- [x] 9.2 Confirm `openspec/specs/subagent-event-bridge/spec.md` revisions match implementation
- [x] 9.3 Confirm `openspec/specs/subagent-background-mode/spec.md` revisions match implementation
- [x] 9.4 Confirm `openspec/specs/recovery-cascade-wiring/spec.md` revisions match implementation
- [x] 9.5 Confirm `openspec/specs/self-reflection-hotmemory/spec.md` revisions match implementation
- [x] 9.6 Confirm `openspec/specs/custom-event-renderer/spec.md` revisions match implementation
- [x] 9.7 Confirm `openspec/specs/agent-event-bus/spec.md` matches implementation
- [x] 9.8 Confirm `openspec/specs/provider-anthropic-signature/spec.md` matches implementation

## 10. Workspace validation

- [x] 10.1 `cargo build --workspace` passes
- [x] 10.2 `cargo test --workspace` passes (1 pre-existing failure: `sandboxed_bash_echo_hello` on master/HEAD~3)
- [x] 10.3 `cargo clippy --workspace --all-targets` passes (1 pre-existing `result_large_err` warning in `synthia-server/src/event_stream.rs:39`, present before Phase 1)
- [x] 10.4 Memory replay parity test: AgentEvent::is_durable() exhaustive match table matches the legacy whitelist — `test_agent_event_is_durable_exhaustive` covers every durable/ephemeral path with assertions; equivalent persistence-layer parity in `test_is_durable_event_type_unknown_defaults_to_durable`
- [x] 10.5 Frontend build (`synthia-web`) passes — `tsc --noEmit && vite build` clean
- [x] 10.6 Frontend dispatch (Phase 7): `wireKindToSegmentType(kind)` covers model_text → text, model_reasoning → thinking, tool_call → tool_call, tool_result → tool_result, progress → progress, response_complete → response_complete — Phase 5 mapping.rs tests confirm the wire shape, Phase 7 frontend extractors dispatch on the new `data` content value