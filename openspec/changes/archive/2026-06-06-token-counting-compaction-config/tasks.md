## 1. tiktoken Token Counting (D4)

- [ ] 1.1 Add `tiktoken-rs = "0.5"` to `crates/synthia-agent/Cargo.toml` (regular dependency, not optional)
- [ ] 1.2 In `stream_builder/builder.rs`, before each LLM sampling call, count tokens in `ctx.messages` using tiktoken with model-based encoding (use `AgentConfig.model`, fall back to `cl100k_base` for unknown models)
- [ ] 1.3 Remove all `chars / 4` estimation logic from `StreamBuilder` and related modules
- [ ] 1.4 Wire the precise token count into `StepCompact::check` for threshold decisions
- [ ] 1.5 Wire the precise token count into `TokenBudgetWarning` event emission

## 2. Compaction Threshold Config (D5)

- [ ] 2.1 Add `compaction_threshold: Option<usize>` field to `AgentConfig` in `config/agent_config.rs`
- [ ] 2.2 Add validation: if `Some(v)`, v must be > 0; log warning if v > model context limit
- [ ] 2.3 In `StepCompact::check`, read `config.compaction_threshold.unwrap_or(100_000)` as the threshold
- [ ] 2.4 Remove hardcoded `100_000` from `compact.rs` — it should come from config only

## 3. Integration and Testing

- [ ] 3.1 Run `cargo test -p synthia-agent` — all tests pass
- [ ] 3.2 Add integration test: set `compaction_threshold = 50_000`, verify compaction triggers at 50K not 100K
- [ ] 3.3 Add test vector: verify tiktoken encodes known strings correctly (e.g., "hello" = 1 token for cl100k_base)
- [ ] 3.4 Run `cargo clippy` — clean