## Context

Two related issues in the agent's token budget management:

**D4 — Token counting uses estimation.**
Token counts in `StreamBuilder` use `chars / 4` estimation. commit `0a7d0d6` wired `TokenUsage` from the provider (response-side usage reporting), but that's not input-side counting. Compaction threshold decisions require accurate context token counts.

**D5 — Compaction threshold hardcoded.**
The100K token compaction threshold is hardcoded in `StepCompact`. Not configurable via `AgentConfig`. Production deployments may need different thresholds per model or use case.

## Goals / Non-Goals

**Goals:**
- Precise token counting via `tiktoken-rs` as the default (no optional feature, no fallback)
- Compaction threshold exposed via `AgentConfig.compaction_threshold: Option<usize>`
- Remove character-length/4 estimation entirely

**Non-Goals:**
- No tiktoken optional feature — precise counting is the only mode
- No token budget enforcement beyond compaction threshold check
- No changes to `synthia-session` compaction config (only agent-level config)

## Decisions

### D1: Token counting strategy — tiktoken only, no dual-mode

- **選擇**: `tiktoken-rs` as a regular dependency; single counting strategy; no `enable_precise_token_count` field
- **理由**: Compaction threshold accuracy requires precise counts. Optional feature with fallback defeats the purpose. Production agent must have accurate token counts.
- **已考慮 alternative**: Optional tiktoken via feature flag — rejected: dual-mode adds complexity with minimal benefit. Provider `TokenUsage` — rejected: reports response usage, not input context size.

### D2: tiktoken encoding model selection

- **選擇**: Use `AgentConfig.model` for encoding selection; fall back to `cl100k_base` for unknown models
- **理由**: Most provider APIs use cl100k_base encoding. Using the model config avoids extra config surface.
- **已考慮 alternative**: Separate encoding config field — rejected: adds config surface for a rare need. Mandatory exact match — rejected: models like `gpt-4o` work with cl100k_base.

### D3: Compaction threshold type and validation

- **選擇**: `AgentConfig.compaction_threshold: Option<usize>`; validate `> 0` and reasonable (e.g., not exceeding model context limit)
- **理由**: `usize` is natural for token counts. `Option` allows defaulting to hardcoded 100K. `f64` in session config is a different concern.
- **已考慮 alternative**: Use `f64` like session config — rejected: token counts are integers. Reuse session config directly — rejected: agent config and session config serve different scopes.

## Risks / Trade-offs

[Risk] `tiktoken-rs` encoding mismatch for non-OpenAI models → Mitigation: Fall back to `cl100k_base`; log warning for unknown model.

[Risk] Type mismatch between `agent_config.compaction_threshold: usize` and `session.rs` field `f64` → Mitigation: Check if session config can be updated or if a conversion layer is needed; do not force consistency across crate boundaries.

## Migration Plan

N/A — dependency addition and config addition, no deployment changes. Rollback via `git revert`.

## Open Questions

None.

## Files to Modify

- `crates/synthia-agent/Cargo.toml`
- `crates/synthia-agent/src/config/agent_config.rs`
- `crates/synthia-agent/src/stream_builder/builder.rs`
- `crates/synthia-agent/src/stream_builder/steps/compact.rs`