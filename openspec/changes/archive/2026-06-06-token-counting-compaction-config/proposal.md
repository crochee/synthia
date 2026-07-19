## Why

The agent uses character-length/4 estimation for token counting, which is too inaccurate for production token budget management. Compaction threshold decisions require precise counts to avoid premature or late compaction. Additionally, the 100K threshold is hardcoded and not configurable for different models or deployment contexts.

## What Changes

**Precise Token Counting (D4)**
- From: Token counts estimated via `chars / 4` or provider response `TokenUsage`
- To: `tiktoken-rs` provides accurate BPE-based token counts for all context messages before each LLM call
- Reason: Compaction threshold accuracy is critical for production deployments. Estimation error accumulates.
- Impact: Non-breaking; improves accuracy of existing behavior

**Configurable Compaction Threshold (D5)**
- From:100K token compaction threshold hardcoded in `StepCompact`
- To: `AgentConfig.compaction_threshold: Option<usize>` — `None` defaults to100K
- Reason: Different models have different context windows; production deployments may need custom thresholds
- Impact: Non-breaking; adds new optional config field

## Capabilities

### New Capabilities
- `precise-token-counting`: Agent counts tokens using tiktoken BPE encoding before each LLM call for accurate context size measurement
- `configurable-compaction-threshold`: Compaction trigger threshold configurable via `AgentConfig.compaction_threshold`

### Modified Capabilities
- `context-compaction`: Compaction trigger now uses precise token counts and configurable threshold

## Impact

- `crates/synthia-agent/Cargo.toml` — adds `tiktoken-rs` dependency
- `crates/synthia-agent/src/config/agent_config.rs` — adds `compaction_threshold` field
- `crates/synthia-agent/src/stream_builder/builder.rs` — tiktoken token counting before LLM calls
- `crates/synthia-agent/src/stream_builder/steps/compact.rs` — reads from config; removes hardcoded 100K