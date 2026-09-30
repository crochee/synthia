# Synthia R13 — attachment_ref Resolve + output_schema Injection + Compaction Wiring

> R13 closes the three deferrals left on the R12 report's §4
> backlog — each landed without touching `re_act.rs` (the
> edit-risk register stays honoured).

## 1. Landed Items

### R13-1 — `attachment_ref` resolve (chat boundary)

Closes the R12-2 follow-on. `build_parts` gained
`Option<&AttachmentStore>`; a `kind = "attachment_ref"` +
`attachment_hash` attachment resolves through the R11 store
(`read_base64_raw`) and inlines the bytes as an `Image`
`ContentPart` — the full hash→store→bytes→wire round trip.
Replay-safe: the content-addressed hash re-resolves to the same
bytes every time. Unknown hashes skip with a `warn!` so the
turn's text still ships. Production call sites
(`send_message`, `regenerate`) pass `Some(&state.attachment_store)`;
tests pass `None`.

### R13-2 — `output_schema` → auto injection (R12-3 deferral closed)

`AgentBuilder::output_schema(schema)` declares the structured
output; `build()` auto-registers the R12
`structured_output_tool` into the tool registry (LIFO per-name
stack so the injection wins lookups) AND threads the schema
string onto `AgentDescriptor::output_schema` — the previously
inert descriptor field (`Option<String>`, set nowhere) is now
the injection source. Zero `re_act.rs` changes: the loop sees
the synthetic tool like any other.

### R13-3 — `compaction_settings` (pi parity)

`AgentBuilder::compaction_settings(settings)` +
`resolve_context_manager(settings, default, provider)` pure
decision core:

| Policy | Result |
|---|---|
| `None` | caller's default (truncating) kept |
| disabled | default kept (`debug!`) |
| invalid (`validate()` Err) | default kept (`warn!`) |
| enabled + valid | **upgraded** to `SummarizingContextManager` |

The summariser forks the agent's own provider —
`provider.complete` with a fixed compaction prompt (preserve
actionable facts/paths/outcomes, drop raw payloads) — pi's
"summarising model is the same streamFn" behaviour. No second
model wiring. `CompactionSettings` finally has a consumer; the
R5-1/R8 type is no longer declared-but-dead.

## 2. Verification

```
cargo check --workspace --all-features --all-targets       0 errors
cargo clippy --workspace --all-features --all-targets -- -D warnings
                                                          0 warnings
cargo +nightly fmt --all -- --check                       0 diff
cargo test --workspace                                    1888 passed, 0 failed
```

R12 baseline: 1883. R13 added **+5** (2 attachment_ref + 2
output_schema + 1 compaction decision).

## 3. Cumulative R1–R13 Overview

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
| R12 | AgentBuilder + schema validator + StructuredOutputTool + /status route | +22 | 1883 |
| **R13** | **attachment_ref resolve + output_schema injection + compaction wiring** | **+5** | **1888** |

## 4. Out of Scope (R14+ backlog)

- re_act.rs strategy split (`AgentStrategy` trait) — still
  deferred for edit-risk; no second paradigm demanded it yet.
- MCP / RAG / GroupJoin / scheduler / worktree — unchanged.
- CompactionSettings consumption *inside*
  `SummarizingContextManager::prepare` (the manager has its own
  trigger thresholds; unifying them with the policy struct is a
  context-crate refactor).
- Server-config surface for compaction
  (`agents.<name>.compaction` TOML) — the builder API is the
  lib consumer's seam today.

## 5. Reference Evidence Map

- R13-1: R11 `AttachmentStore::read_base64_raw` + dsh
  ImageAttachmentRef flow.
- R13-2: pi-subagents `structured-output.ts` (synthetic-tool
  capture) + the OpenAI Agents SDK `output_schema` convention.
- R13-3: pi `packages/agent/src/harness/compaction/compaction.ts`
  (`CompactionSettings` + same-provider summarisation).
