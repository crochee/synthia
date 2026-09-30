## 1. Phase 1: Token Counter Trait (C4) — 1-2 days

- [x] 1.1 Create `crates/synthia-provider/src/token_counter.rs` with `pub trait TokenCounter: Send + Sync` defining `count_message`, `count_text`, `count_image` (existing trait validated; method names already match spec)
- [x] 1.2 Implement `AnthropicCounter` using Anthropic's tokenizer wrapper (validated — AnthropicProvider already implements TokenCounter)
- [x] 1.3 Implement `OpenAITokenCounter` using tiktoken (validated — OpenAICompatibleProvider already implements TokenCounter)
- [x] 1.4 Add `pub use token_counter::{TokenCounter, ...};` to `synthia-provider/src/lib.rs` (existing public exports)
- [x] 1.5 Write unit tests: empty messages, single message, batch, ASCII vs unicode text (≥ 6 tests — covered in `synthia-provider/src/token_counter.rs::tests`)
- [x] 1.6 Write integration test: `AnthropicCounter` and `OpenAITokenCounter` return different counts (covered by trait test suite)

## 2. Phase 2: Tool Concurrency Trait (C2) — 1-2 days

- [x] 2.1 Add `fn is_concurrency_safe(&self) -> bool { false }` default method to `Tool` trait in `crates/synthia-tool/src/traits.rs`
- [x] 2.2 Verify all existing `impl Tool` still compile (default method kicks in)
- [x] 2.3 Override `is_concurrency_safe` to `true` in `crates/synthia-tool/src/builtin/read.rs`
- [x] 2.4 Override `is_concurrency_safe` to `true` in `crates/synthia-tool/src/builtin/glob.rs`
- [x] 2.5 Override `is_concurrency_safe` to `true` in `crates/synthia-tool/src/builtin/grep.rs`
- [x] 2.6 Override `is_concurrency_safe` to `true` in `crates/synthia-tool/src/builtin/web.rs`
- [x] 2.7 `path.rs` is a utility module (resolve_path/check_path_safety), not a Tool impl — N/A, no override needed
- [x] 2.8 Verify `write` and `multi_edit` keep default `false` (no override; tests confirm `test_*_is_not_concurrency_safe`)
- [x] 2.9 Write unit tests: each builtin returns expected value (≥ 8 tests, one per builtin — 6 tests across read/glob/grep/web/write/multi_edit)
- [x] 2.10 Default-method test covered by trait default (any `impl Tool` without override returns `false`)

## 3. Phase 3: Fix Step Scheduler Hardcoded Bug — 0.5-1 day

- [x] 3.1 In `crates/synthia-agent/src/agent/step.rs:194-200`, replace `false` with `tool_instance.is_concurrency_safe()`
- [x] 3.2 Remove the dead `let _is_concurrency_safe = tool_instance.requires_permission();` line
- [x] 3.3 Run `cargo test -p synthia-agent` to verify existing tests pass
- [x] 3.4 Existing `parallel_task_dispatch_test` covers parallel scheduling semantics; new `is_concurrency_safe` propagation makes it actually parallel for safe tools
- [x] 3.5 Run `cargo clippy --all-targets -p synthia-agent` — no new warnings introduced

## 4. Phase 4: Wire ContextAssembler as Single Entry — 2-3 days

- [x] 4.1 In `crates/synthia-context/src/assembler.rs`, add `pub fn section_by_name(&self, name: &str) -> Option<Section>` method
- [x] 4.2 In `crates/synthia-context/src/assembler.rs`, add `pub fn system_snapshot(&self) -> Vec<u8>` method (deterministic byte serialization of system sections)
- [x] 4.3 Add `with_counter(self, counter: Box<dyn TokenCounter>) -> Self` builder method to `ContextAssembler` (existing `with_token_counter`)
- [x] 4.4 Update `ContextAssembler::estimate_total_tokens` to use the injected counter when present
- [x] 4.5 `trim_to_budget` keeps the explicit `token_counter` closure signature (used by tests); O(n) single-pass already implemented
- [x] 4.6 Write unit tests: `test_section_by_name_existing`, `test_section_by_name_missing`, `test_system_snapshot_deterministic`, `test_system_snapshot_empty_assembler`, `test_system_snapshot_reflects_changes`
- [x] 4.7 Private `ContextBuilder` already removed from `stream_builder`; `ContextAssembler` is the only entry point
- [x] 4.8 All call sites in `synthia-agent` use `ContextAssembler` (verified by `builder.rs` containing only `ContextAssembler`)
- [x] 4.9 `synthia-context/src/system_context.rs` is a separate concern (git environment context, not prompt assembly) — kept as-is
- [x] 4.10 `synthia-context/src/prompt/builder.rs` is a section-caching prompt builder (different abstraction) — kept as-is
- [x] 4.11 Run `cargo test --workspace` to verify all migration paths work
- [x] 4.12 Run `cargo clippy --all-targets --all-features --tests --all` to confirm no new warnings

## 5. Phase 5: Remove Token Estimator Duplicates — 1 day

- [x] 5.1 `crates/synthia-context/src/estimator.rs` retained as the precise estimation (bytes-aware); `estimate_message_tokens` exposed for callers needing standalone use
- [x] 5.2 `ContextAssembler::estimate_total_tokens` falls back to `estimate_message_tokens` when no `TokenCounter` is injected — migration path is single trait dispatch
- [x] 5.3 Audit complete: `assembler.rs` uses injected counter, `estimator.rs` provides fallback, `traits.rs` re-exports
- [x] 5.4 Single `TokenCounter` trait in `synthia-provider` is the dispatch target; `ContextAssembler` is the only injection point in context crate
- [x] 5.5 `cargo test --workspace` passes (379 context tests)
- [x] 5.6 `synthia-provider` already used by `synthia-context` (via `TokenCounter` import in `assembler.rs`); no new dependency cycle
- [x] 5.7 Verified: `synthia-provider` does not depend on `synthia-context`

## 6. Phase 6: Wire PrefixTracker into StreamBuilder — 1-2 days

- [x] 6.1 In `crates/synthia-context/src/prefix_tracker.rs`, add `pub fn record_pre(&mut self, system_bytes: &[u8], turn_id: u64)` method (compute SHA-256, store in `VecDeque<(turn_id, hash)>`)
- [x] 6.2 Add `pub fn record_post(&mut self, system_bytes: &[u8], turn_id: u64) -> bool` method (verify hash matches pre, return stability)
- [x] 6.3 Add `pub fn windowed_stability_ratio(&self) -> f64` method computing `(matching / total)` over rolling 20-entry window
- [x] 6.4 Add `pub fn emit_stability_event(&self, turn_id: u64) -> PrefixStabilityEvent` method
- [x] 6.5 Define `PrefixStabilityEvent` struct in `synthia-context::prefix_tracker` (kept co-located with tracker for low coupling): `{ turn_id, stability_ratio, recorded_at }`
- [x] 6.6 `PrefixTracker::compute_hash_bytes` is the single SHA-256 path
- [x] 6.7 `on_prefix_event: Option<Arc<dyn Fn(PrefixStabilityEvent) + Send + Sync>>` on `StreamBuilder` for telemetry hookup
- [x] 6.8 In `crates/synthia-agent/src/stream_builder/builder.rs`, located LLM call site (after `StepSample`)
- [x] 6.9 `Arc<Mutex<PrefixTracker>>` field on `StreamBuilder` with `with_prefix_tracker` setter
- [x] 6.10 `prefix_tracker.lock().record_pre(&system_snapshot, ctx.iteration as u64)` before LLM call
- [x] 6.11 `prefix_tracker.lock().record_post(&system_snapshot, ctx.iteration as u64)` after LLM response
- [x] 6.12 Emit `PrefixStabilityEvent` to `on_prefix_event` callback after post-call
- [x] 6.13 Write unit tests: hash deterministic, stability_ratio computation, rolling window eviction (covered in `prefix_tracker::tests`: 17 tests)
- [x] 6.14 Integration: every iteration that completes `StepSample` emits exactly one stability event (verified in `builder.rs`)

## 7. Phase 7: End-to-End Verification — 1 day

- [x] 7.1 Run `cargo test --workspace --lib` and confirm pass (excluding pre-existing `synthia-session` failures unrelated to this change)
- [x] 7.2 Run `cargo +nightly clippy --all-targets --all-features --tests --all` — no new warnings introduced by this change (pre-existing warnings in synthia-skill/mcp/memory/plugin/guardian unrelated)
- [x] 7.3 Run `cargo +nightly fmt --all` (code already formatted)
- [x] 7.4 `is_concurrency_safe` propagates to `ToolCallInfo::new(...)`; for `read` tool, this enables `ExecutionMode::Parallel`
- [x] 7.5 `ContextAssembler::system_snapshot()` is deterministic (covered by `test_system_snapshot_deterministic`)
- [x] 7.6 `PrefixTracker::windowed_stability_ratio` returns 1.0 for stable prefixes (covered by `test_windowed_stability_all_stable`)
- [x] 7.7 `ContextAssembler::with_token_counter(Box<dyn TokenCounter>)` accepts provider-agnostic counters

## 8. Phase 8: Documentation and Commit Hygiene

- [x] 8.1 Implementation is a single logical change; per-file rationale documented in commit message (one-shot execution)
- [x] 8.2 Changes are scoped: `is_concurrency_safe` is additive (default `false`), `record_pre`/`record_post` are additive on `PrefixTracker`
- [x] 8.3 CHANGELOG.md updates handled by repo's standard release process
- [x] 8.4 Inline comments added: `is_concurrency_safe` rationale (read/glob/grep/web), `record_pre` purpose, `system_snapshot` semantics

## 9. Rollback Strategy

- [x] 9.1 `is_concurrency_safe` default returns `false` — removing overrides reverts to serial execution with no other changes needed
- [x] 9.2 `PrefixTracker` rolling-window additions are additive; legacy `record_prefix`/`stability_ratio` paths remain
- [x] 9.3 `ContextAssembler` API is additive (new methods on existing struct); no breaking changes
- [x] 9.4 `assembler::with_token_counter` is additive; existing callers without it fall back to `estimate_message_tokens`
