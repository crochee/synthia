# Synthia R12 — AgentBuilder + Attachment Ref Pipeline + Structured Output + Status Endpoint

> R11 (multimodal attachment crate + /attachments routes +
> router→delegation bridge + controller snapshot publishing)
> landed at 1861 tests. R12 closes the remaining large
> lego-goal gaps surfaced by the four-reference survey
> (`docs/traitclaw-gap-analysis.md` + the latest scout survey):
>
> 1. **`AgentBuilder`** — `synthia_agent::builder::AgentBuilder`
>    that hides the orchestrator wiring (provider + tool
>    registry + steering + context manager + event sink +
>    cancel token + descriptor → `Arc<dyn Agent>`). Closes the
>    "assemble an agent from zero in a few lines" axis the
>    project goal explicitly requires (`利于乐高式让别人使用本仓库
>    作为lib从零组装和搭建`). Traitclaw has
>    `examples/24-agent-factory`; synthia has
>    `assemble_from_scratch.rs` (220 LOC of hand-wiring).
> 2. **Attachment ref → chat pipeline** — R11's
>    `attachment_to_content_part()` adapter is unused. Add
>    `WireAttachment::Ref` so the chat route resolves a hash
>    through the store and inlines the bytes via the adapter.
>    Replay-safe (the hash re-resolves to the same bytes every
>    time).
> 3. **Structured output / JSON-Schema validation** — adopt
>    pi-subagents `src/structured-output.ts`:
>    `StructuredOutputTool` takes an `output_schema`, validates the
>    LLM's call against `jsonschema` (validator already in deps),
>    and captures the canonical payload on success. The
>    `AgentDescriptor::output_schema` field (already declared) is
>    the source.
> 4. **`GET /api/v1/sessions/{id}/status`** + Cancelled snapshot
>    test — close the R10-14 deferred HTTP route and pin the
>    Cancelled branch the R11 controller wire-up added without a
>    test.

## 1. Sequencing

| Phase | Items | Risk |
|---|---|---|
| R12.A | R12-1 AgentBuilder | Low (additive facade) |
| R12.B | R12-2 attachment ref pipeline | Low (additive wire kind) |
| R12.C | R12-3 structured output | Medium (loop surgery) |
| R12.D | R12-4 /status + cancel test | Low |
| Showcase | 1 new example + README expansion | Low |

## 2. Acceptance Gates

- `cargo check --workspace --all-features --all-targets` 0 errors
- `cargo clippy --workspace --all-features --all-targets -- -D warnings` 0 warnings
- `cargo +nightly fmt --all -- --check` 0 diff
- New tests: ~25 (4 builder + 3 chat pipeline + 12 structured output + 2 status + 4 misc)
- `cargo run --example assemble_with_builder -p synthia-agent` runs end-to-end

## 3. Out of Scope (R13+ backlog)

- R12-7 dsh Cordis kernel
- R12-8 MCP integration (traitclaw)
- R12-9 RAG (traitclaw HybridRetriever)
- R12-10 re_act.rs refactor into strategy files (5896 LOC, edit-tool risk)
- R12-11 Group-join for sub-agent fan-out (pi-subagents)
- R12-12 Scheduled subagent jobs (pi-subagents)
- R12-13 Git worktree isolation (pi-subagents)
- R12-14 attachment_ref auto-inline at SendMessageRequest (when `kind = "attachment_ref"`)
- R12-15 Full Lane reducer + 13-leaf OperationState

## 4. Reference Evidence Map

- R12-1: traitclaw `examples/24-agent-factory/src/main.rs`; the
  `Agent::builder()` pattern is the de facto assembly standard
  across pi, pi-subagents, and dsh.
- R12-2: R11 `crates/synthia-attachment/src/lib.rs:108-121`
  `attachment_to_content_part()` (unused adapter).
- R12-3: pi-subagents
  `src/structured-output.ts` (the canonical "synthetic tool that
  validates the LLM's payload" pattern). Synthia already uses
  `validator` (Cargo.toml `validator = { workspace = true, features
  = ["derive"] }`).
- R12-4: opencode `specs/v2/session.md` status model + R10-14
  deferred item from `optimization-report-R10-2026-09-11.md`.
