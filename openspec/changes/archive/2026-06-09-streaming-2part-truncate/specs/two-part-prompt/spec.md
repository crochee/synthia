# two-part-prompt Specification

## Purpose

Define the contract for `TwoPartPrompt`, a prompt-assembly container that splits the system prompt into a stable `header` (2-3K tokens, byte-level immutable per session) and a variable `body`, so that upstream LLM providers can cache the header across turns for prefix-cache cost reduction. Header stability is tracked via `header_hash` and reported as `prefix_stability_ratio`.

## ADDED Requirements

### Requirement: TwoPartPrompt SHALL separate stable header from variable body

`TwoPartPrompt` SHALL hold a `header: String` (the stable portion, byte-level immutable per session) and a `body: String` (the variable portion: task, memory, working state). The `header` SHALL be byte-identical across all LLM calls within a single session.

#### Scenario: Header immutable across turns
- **WHEN** a `TwoPartPrompt` is built at the start of a session
- **THEN** the `header` field SHALL be the same bytes on turn 1 and turn 12 of the same session
- **THEN** `header` SHALL NOT be mutated by `finalize`, `to_request`, or any other method

#### Scenario: Body varies per turn
- **WHEN** a `TwoPartPrompt` is finalized for turn 1 vs turn 12
- **THEN** the `body` field MAY differ (it carries the current user prompt, summarized history, and working state)
- **THEN** the resulting `CompletionRequest` SHALL carry the latest `body`

### Requirement: TwoPartPrompt SHALL track header stability via blake3 hash

`TwoPartPrompt::header_hash: [u8; 32]` SHALL be the blake3 hash of the `header` field, computed at `build()` time and stored in the struct.

#### Scenario: Hash computed at build
- **WHEN** `TwoPartPrompt::build(header, body, family)` is called
- **THEN** `header_hash` SHALL equal `blake3::hash(header.as_bytes()).as_bytes()`
- **THEN** the hash SHALL be `[u8; 32]`

#### Scenario: finalize compares to prev
- **WHEN** `TwoPartPrompt::finalize(self, prev_header_hash: Option<[u8; 32]>, form: SystemMessageForm)` is called
- **THEN** if `prev_header_hash == Some(self.header_hash)`, `TwoPartDecision.cache_hit_expected` SHALL be `true`
- **THEN** if `prev_header_hash == Some(other) where other != self.header_hash`, `cache_hit_expected` SHALL be `false` and `header_unstable_reason` SHALL be `Some(diff summary)`
- **THEN** if `prev_header_hash == None` (first call), `cache_hit_expected` SHALL be `false` and `header_unstable_reason` SHALL be `None`

### Requirement: SystemMessageForm SHALL control single-vs-two-part assembly

`SystemMessageForm` SHALL have two variants: `Single` (one system message combining header+body) and `TwoPart` (two system messages: header first, then body).

#### Scenario: Single form
- **WHEN** `finalize(..., SystemMessageForm::Single)` is called
- **THEN** the resulting `final_messages` SHALL contain exactly one `ChatMessage` with `role: System` and `content = header + "\n\n" + body`
- **THEN** this form is the legacy path (backward compatible with non-cache-friendly models)

#### Scenario: TwoPart form
- **WHEN** `finalize(..., SystemMessageForm::TwoPart)` is called
- **THEN** the resulting `final_messages` SHALL contain two consecutive `ChatMessage` entries: first with `role: System` and `content = header`, second with `role: System` (or `User` if provider requires) and `content = body`
- **THEN** Anthropic and OpenAI providers SHALL both accept this layout natively
- **THEN** the upstream provider's prompt-cache SHALL key on the first `System` message, yielding cache hits when `header` is stable

### Requirement: ModelFamily SHALL distinguish provider-specific quirks

`ModelFamily` SHALL have three variants: `Anthropic`, `OpenAI`, `Generic`.

#### Scenario: Family passed at build
- **WHEN** `TwoPartPrompt::build(header, body, family)` is called
- **THEN** the `model_family` field SHALL be stored verbatim
- **THEN** `finalize` MAY use `model_family` to choose provider-specific message shaping (e.g., Anthropic requires assistant prefill for some prompts; OpenAI supports multiple `developer` system messages)

### Requirement: Header length SHALL be estimated via char/3.5 heuristic

The `header` field SHALL be sized such that `header.chars().count() / 3.5` (rounded up) is in the range [600, 3500] tokens. The estimator SHALL use the `chars / 3.5` heuristic; it SHALL NOT call tiktoken or any tokenizer library at runtime.

#### Scenario: Header within budget
- **WHEN** a `TwoPartPrompt` is built with `header.chars().count() == 7000` (≈ 2000 tokens via /3.5)
- **THEN** the build SHALL succeed (within the 600-3500 token target range)
- **THEN** no error SHALL be raised

#### Scenario: Header over budget (allowed but warned)
- **WHEN** `header.chars().count() > 12250` (≈ 3500 tokens via /3.5)
- **THEN** the build SHALL still succeed
- **THEN** a `tracing::warn!` event SHALL be emitted with the actual estimated token count

### Requirement: TwoPartPrompt header violation SHALL emit event, not panic

If the `header` field mutates between turns within a session (i.e., `header_hash` differs from `prev_header_hash`), the system SHALL emit a `header_unstable` telemetry event with a diff summary. The system SHALL NOT panic in production.

#### Scenario: Header drift detected
- **WHEN** `finalize` is called and `self.header_hash != prev_header_hash.unwrap()`
- **THEN** `header_unstable_total` counter SHALL be incremented
- **THEN** a structured log entry SHALL be emitted with `old_hash`, `new_hash`, and `diff_summary`
- **THEN** the call SHALL NOT panic; the request SHALL proceed normally with cache_hit_expected = false

#### Scenario: Dev mode assertion
- **WHEN** the binary is compiled with `--features dev-assertions` (or equivalent)
- **THEN** a header-drift detection SHALL additionally trigger `debug_assert_eq!` on `header_hash`
- **THEN** without this feature, the assertion SHALL be a no-op
