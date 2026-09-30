# Brainstorming: Track D — Token Counting + Compaction Config (D4 + D5)

## Background

Two related issues:
- D4: Token counting uses character-length/4 estimation, not precise counting
- D5: Compaction threshold (100K) hardcoded, not configurable

## Decision Chain

### D4: Token Counting Precision

**Context:** commit `0a7d0d6` wired `TokenUsage` from provider, but that's response-side usage reporting, not input-side counting for context budget decisions.

**Options:**
- A) tiktoken precise counting (recommended)
- B) Provider `TokenUsage` sufficient
- C) estimation first, tiktoken later

**Decision: A** — tiktoken as default, no fallback. Compaction threshold decisions require accurate context token counts. Estimation (chars/4) is too inaccurate for production.

**Refinement during design:** No optional feature, no dual-mode. Single strategy: tiktoken as regular dependency. `enable_precise_token_count` field removed — precise counting IS the default behavior.

### D5: Compaction Threshold Configuration

**Options:**
- Expose to `AgentConfig.compaction_threshold`
- Environment variable
- Keep hardcoded

**Decision: `AgentConfig.compaction_threshold: Option<usize>`** — aligns with existing Rust config patterns. Validate `> 0` and reasonable.

**Note:** `synthia-session/src/session.rs:59` has `compaction_threshold: Option<f64>`. Need type alignment check.

## Design Trade-offs

### tiktoken Model Selection

`tiktoken_rs::BpeEncoder::new()` requires model name. Different models use different encodings.

| Approach | Pros | Cons |
|----------|------|------|
| Use `AgentConfig.model` field | Automatic, no extra config | Model string may not match tiktoken model names |
| Separate encoding config | Exact control | More config surface |
| Fall back to `cl100k_base` | Safe default | May be slightly off for non-OpenAI models |

**Chosen: Use `AgentConfig.model` for encoding selection, fall back to `cl100k_base` for unknown models.** Most provider APIs use cl100k_base encoding.

### Compaction Threshold Type

`session.rs` uses `f64`, agent config uses `usize`. Need to decide:
- Convert `f64` to `usize` (loses precision but matches existing session field)
- Use `usize` everywhere (breaking change in session config)

**Decision:** Use `usize` in `AgentConfig` (token counts are integers). Check if session config can be updated or if a conversion layer is needed.

## Output

Design doc committed to `docs/superpowers/specs/2026-06-06-track-d-token-compaction-design.md`.

## Dependencies

- D4 (tiktoken) is a prerequisite for Track C (memory semantic search)
- No other cross-change dependencies

## Verification

- `cargo test -p synthia-agent` passes
- Integration test: set threshold to 50K, verify compaction at 50K not 100K
- tiktoken encodes known strings correctly (test vectors)