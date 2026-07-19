# token-counter-unification Specification

## Purpose
TBD - created by archiving change synthia-gap-analysis-2026-06-07. Update Purpose after archive.
## Requirements
### Requirement: synthia-provider SHALL expose TokenCounter trait

`crates/synthia-provider/src/token_counter.rs` SHALL define a `TokenCounter` trait with two methods: `count_messages(&[Message]) -> u32` and `count_text(&str) -> u32`. The trait SHALL be `Send + Sync`.

#### Scenario: Trait definition exists
- **WHEN** `synthia-provider/src/token_counter.rs` is read
- **THEN** a `pub trait TokenCounter: Send + Sync` SHALL be defined
- **THEN** both methods SHALL be declared with the signatures above

#### Scenario: Anthropic and OpenAI implementations exist
- **WHEN** implementations of `TokenCounter` are searched
- **THEN** `AnthropicCounter` (using Anthropic's tokenizer) and `OpenAITokenCounter` (using tiktoken) SHALL exist
- **THEN** each SHALL be exported as a `pub struct` with a `new()` constructor

#### Scenario: count_messages is batch
- **WHEN** `count_messages` is called with a `&[Message]` of N messages
- **THEN** it SHALL return a single `u32` representing total token count across all messages
- **THEN** the result SHALL equal the sum of per-message token counts

### Requirement: synthia-context SHALL consume TokenCounter via Arc<dyn>

`synthia-context` modules that perform token estimation SHALL hold a `Arc<dyn TokenCounter>` reference (injected at construction) rather than implementing their own estimation logic. The `Message` type used by `TokenCounter::count_messages` SHALL be the existing `synthia_provider::Message` type (or a re-export thereof from `synthia-message` if the canonical type lives there).

#### Scenario: ContextAssembler holds Arc<dyn TokenCounter>
- **WHEN** `ContextAssembler::new()` is called
- **THEN** it SHALL accept a `Arc<dyn TokenCounter>` parameter (or use a builder method `with_counter`)
- **THEN** it SHALL NOT instantiate any provider-specific counter internally

#### Scenario: Arc<dyn> is Send + Sync
- **WHEN** the `Arc<dyn TokenCounter>` is shared across threads
- **THEN** it SHALL be safe to send between threads (the trait's `Send + Sync` bound ensures this)
- **THEN** no `unsafe impl Send` SHALL be required

### Requirement: Duplicate estimation functions SHALL be removed

The following functions in `synthia-context` SHALL be deleted, with their callers migrated to use the injected `TokenCounter`:
- `synthia_context::estimator::estimate_message_tokens` (the `pub(crate)` one)
- Any local `count_tokens` / `estimate_tokens` function in `synthia-context` modules

#### Scenario: estimator module removed
- **WHEN** `synthia-context/src/estimator.rs` is searched
- **THEN** either the file SHALL be deleted, or it SHALL contain only thin re-exports
- **THEN** the `pub(crate) fn estimate_message_tokens` function SHALL NOT be defined

#### Scenario: No other estimator implementations
- **WHEN** `synthia-context` source is searched for `fn count_tokens` or `fn estimate_tokens`
- **THEN** the only such functions SHALL be methods on `ContextAssembler` that delegate to the injected `TokenCounter`
- **THEN** no module-level estimator SHALL exist

### Requirement: Compaction logic SHALL use TokenCounter for threshold checks

The compaction decision logic (`CompactAction::MustCompact` / `Warning` / `None`) SHALL use the injected `TokenCounter` to compute the actual token count of the current message list, rather than relying on per-module local estimates.

#### Scenario: MustCompact uses real count
- **WHEN** the compaction decision is made
- **THEN** it SHALL call `counter.count_messages(&current_messages)` to get the actual count
- **THEN** it SHALL compare this against `compaction_threshold` to decide action
- **THEN** the decision SHALL NOT use any local `s.len() / 4` style heuristic

#### Scenario: Different providers give different counts
- **WHEN** `ContextAssembler` is configured with `AnthropicCounter` vs `OpenAITokenCounter`
- **THEN** the actual token count returned for the same `&[Message]` MAY differ
- **THEN** the compaction decision SHALL follow the configured counter, not a global default

### Requirement: Public API for token counting SHALL remain available

`synthia-context` SHALL continue to expose a public function `count_tokens_for_assembly(messages: &[Message], counter: &dyn TokenCounter) -> u32` for callers that need a one-shot token count without holding a `ContextAssembler`.

#### Scenario: Public function exists
- **WHEN** external code calls `synthia_context::count_tokens_for_assembly`
- **THEN** the function SHALL exist with the signature `pub fn count_tokens_for_assembly(messages: &[Message], counter: &dyn TokenCounter) -> u32`
- **THEN** it SHALL return the result of `counter.count_messages(messages)`

#### Scenario: Existing callers migrate cleanly
- **WHEN** existing call sites of `estimate_message_tokens` are migrated
- **THEN** each call site SHALL pass the injected `Arc<dyn TokenCounter>` (or `&dyn TokenCounter`) to the new function
- **THEN** no call site SHALL silently fall back to a local estimate

### Requirement: synthia-context depends on synthia-provider

`crates/synthia-context/Cargo.toml` SHALL declare `synthia-provider` as a workspace dependency. The dependency direction `synthia-context → synthia-provider` SHALL NOT create a cycle.

#### Scenario: No circular dependency
- **WHEN** `cargo metadata --format-version 1` is run
- **THEN** the dependency graph SHALL NOT contain a cycle involving `synthia-context` and `synthia-provider`
- **THEN** `cargo build --workspace` SHALL succeed

#### Scenario: Minimal dependency surface
- **WHEN** `synthia-context` declares its dependency on `synthia-provider`
- **THEN** it SHALL depend only on the `TokenCounter` trait and `Message` type
- **THEN** it SHALL NOT depend on the HTTP client or other heavy provider internals

