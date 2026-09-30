# Synthia R12 — AgentBuilder + Schema Validation + Structured Output + /status Route

> R12 closes the remaining top-3 gaps from the four-reference
> survey (the same methodology as R10/R11): the lego-assembly
> facade, the JSON-Schema validation layer for tool arguments,
> the structured-output capture tool, and the deferred
> `/sessions/{id}/status` HTTP route.

## 1. Landed Items

### R12-1 — `AgentBuilder` (`synthia-agent/src/agent/builder.rs`)

Traitclaw `examples/24-agent-factory` parity. The fluent factory
hides the orchestrator wiring: one `AgentBuilder::new(provider)`
plus chained setters produces a fully wired `ReActAgent` —
closing the goal's 乐高式从零组装 (lego-assembly) axis. The
builder is `Clone` so callers can branch configurations cheaply.
Re-exported from `synthia_agent::AgentBuilder`.

### R12-3a — `synthia_core::schema` (`crates/synthia-core/src/schema.rs`)

Minimal JSON-Schema-subset validator (`type` / `required` /
`properties` / `items`; unknown keywords permissive per the
JSON-Schema spec's annotation rule). `validate_against_schema`
returns **all** violations at once with dotted paths
(`"opts.depth"`, `"[2]"`) so the model gets complete feedback and
can self-correct in a single retry — the pi-subagents
`structured-output.ts` behaviour.

### R12-3b — `StructuredOutputTool`
        (`crates/synthia-tool/src/structured_output.rs`)

pi-subagents `src/structured-output.ts` parity. The synthetic
tool's `parameters()` IS the caller's schema — the provider
enumerates the fields from it and fills them as the tool-call
arguments. `call()` re-validates through
`validate_against_schema` and either captures the canonical JSON
payload as the tool result or returns an `is_error` result
enumerating every violation. No response_format machinery.

### R12-2 — `WireAttachment.attachment_hash` (chat wire)

The chat request body gains an optional `attachment_hash` field:
`kind = "attachment_ref"` + the hash references a
previously-uploaded attachment (R11's `/api/v1/attachments`) by
content hash. `WireAttachment` now derives `Default` +
`Serialize`; the hash round-trips through serde and rides
verbatim into the session input queue. The agent-runtime
resolve-through-store step (inline the bytes via
`attachment_to_content_part`) is the R13 follow-on.

### R12-4 — `GET /api/v1/sessions/{id}/status` (closes R10-14)

Returns the latest `OperationSnapshot` from the controller's
snapshot bus — the typed `state` / `iteration` / `usage` /
`last_error` surface — or 404 when the session is unknown / has
never run. Complements the string-label `infer_status` on
`GET /sessions/{id}`. A bus-latest test pins the
late-subscriber-sees-latest contract the route depends on.

## 2. Verification

```
cargo check --workspace --all-features --all-targets       0 errors
cargo clippy --workspace --all-features --all-targets -- -D warnings
                                                          0 warnings
cargo +nightly fmt --all -- --check                       0 diff
cargo test --workspace                                    1883 passed, 0 failed
```

R11 baseline: 1861 tests. R12 added **+22** (4 builder +
10 schema validator + 5 structured output + 1 wire round-trip +
1 status bus + 1 net from fixture churn).

## 3. Lego perspective

| Brick | Crate | What it lets you do |
|---|---|---|
| `AgentBuilder` | `synthia-agent` | Assemble a complete agent from zero in one chained expression |
| `validate_against_schema` + `SchemaViolation` | `synthia-core` | Validate any JSON value against the schemars subset with field-level feedback |
| `StructuredOutputTool` + `structured_output_tool()` | `synthia-tool` | Capture schema-validated structured output from the model via a synthetic tool |
| `WireAttachment.attachment_hash` | `synthia-server` | Reference uploaded attachments by content hash in chat requests |
| `GET /sessions/{id}/status` | `synthia-server` | Read the typed operation snapshot over HTTP |

## 4. Out of Scope (R13+ backlog)

- Agent-runtime attachment_ref resolve (inline the bytes from
  the store at the `PromptMulti` boundary).
- `AgentDescriptor::output_schema` → automatic
  `StructuredOutputTool` injection (the type + tool both ship;
  the auto-wiring when a descriptor declares a schema is R13).
- CompactionSettings wired into the run factory.
- re_act.rs strategy split (AgentStrategy trait) — still
  deferred per the edit-risk register.
- MCP / RAG / GroupJoin / scheduler / worktree — unchanged from
  the R11 §4 backlog.

## 5. Reference Evidence Map

- R12-1: traitclaw `examples/24-agent-factory/src/main.rs`.
- R12-3a/b: pi-subagents `src/structured-output.ts` (synthetic
  tool + schema validation + isError retry).
- R12-2: R11 `attachment_to_content_part` (unused adapter) +
  dsh ImageAttachmentRef flow.
- R12-4: opencode `specs/v2/session.md` status model; R10 plan
  §R10-5/§R10-14 deferrals.

## 6. Cumulative R1–R12 Overview

| Round | Theme | Tests added | Total |
|---|---|---|---|
| R3 + R5 | Steering / HookMap / EffectGate / CompactionSettings | +46 | 1640 |
| R4 | TokenUsage + cache + retry typed errors | +13 | 1653 |
| R6 | BlockAssembler + HookMap + typed events + Arc-msg | +43 | 1696 |
| R7 | Runtime neutrality (CancelToken, futures mpsc) | -1 | 1695 |
| R8 | chrono + assemble_from_scratch tutorial | 0 | 1695 |
| R9 | ProviderProfile + SkillProvider + state hooks + spawn_detached + OperationSnapshot | +44 | 1739 |
| R10 | ModelTier + Steering::for_tier + AdaptiveRegistry + ToolOutputDefinition + Router + SnapshotBus | +92 | 1831 |
| R11 | AttachmentStore + /attachments routes + mentions bridge + snapshot emit | +30 | 1861 |
| **R12** | **AgentBuilder + schema validator + StructuredOutputTool + /status route** | **+22** | **1883** |

All 1883 tests pass; clippy 0 warnings; fmt 0 diff; check 0
errors. Zero regressions across the workspace.
