## ADDED Requirements

### Requirement: compact_context SHALL be exposed as an LLM-callable tool

The context compaction capability MUST be exposed as a tool registered with the tool orchestrator. The tool MUST be named `compact_context` and accept an optional `reason` parameter (string) for the LLM to explain why compaction is requested.

#### Scenario: LLM calls compact_context with reason

- **WHEN** the LLM emits a `tool_use` for `compact_context` with `reason: "context getting long"`
- **THEN** the tool orchestrator dispatches the call to the compaction pipeline
- **AND** compaction runs (Stage 1 → Stage 2 → Stage 3 as needed)
- **AND** a `CompactionAnalyticsAttempt` is recorded with `trigger = "tool-call"`
- **AND** the `tool_result` summarizes what was compacted (e.g., "Compacted 12 messages, freed 4500 tokens")

#### Scenario: LLM calls compact_context without reason

- **WHEN** the LLM emits a `tool_use` for `compact_context` with no `reason` parameter
- **THEN** compaction runs normally
- **AND** the `CompactionAnalyticsAttempt.reason` is set to `"llm-requested"`
- **AND** the tool_result is returned as in the with-reason scenario

---

### Requirement: compact_context tool description SHALL include token hints

The tool description MUST include a `<context_tokens>` XML tag with the current context token count, updated each time the tool registry is queried. This hint allows the LLM to make informed decisions about when to call `compact_context`.

#### Scenario: Token hint reflects current context size

- **WHEN** the tool registry is queried and the current context is 75000 tokens
- **THEN** the `compact_context` tool description contains `<context_tokens>75000</context_tokens>`
- **AND** the hint is accurate within ±100 tokens of the actual context size

#### Scenario: Token hint updates between queries

- **WHEN** the tool registry is queried at turn 1 (60000 tokens) and again at turn 5 (80000 tokens)
- **THEN** the turn 1 description shows `<context_tokens>60000</context_tokens>`
- **AND** the turn 5 description shows `<context_tokens>80000</context_tokens>`
- **AND** the LLM sees the updated hint and can decide whether to call the tool

---

### Requirement: compact_context tool SHALL retain auto-trigger fallback

The runtime MUST automatically trigger context compaction when the context token count exceeds the configured threshold (default 80% of `context_window`), regardless of whether the LLM has called the `compact_context` tool. This fallback ensures compaction occurs even if the LLM neglects to call the tool.

#### Scenario: Auto-trigger at 80% threshold

- **WHEN** the context reaches 80% of `context_window` and the LLM has not called `compact_context`
- **THEN** the runtime triggers compaction automatically
- **AND** a `CompactionAnalyticsAttempt` is recorded with `trigger = "auto-threshold"`
- **AND** the LLM is informed via a system message that compaction occurred

#### Scenario: LLM call does not disable auto-trigger

- **WHEN** the LLM calls `compact_context` at 70% threshold
- **AND** the context subsequently grows to 80% threshold
- **THEN** the auto-trigger still fires
- **AND** a second compaction runs (the LLM's earlier call did not "satisfy" the threshold)

#### Scenario: Auto-trigger and LLM call at same iteration

- **WHEN** the LLM calls `compact_context` at iteration 10
- **AND** the auto-trigger is also scheduled for iteration 10 (because threshold was crossed)
- **THEN** only the LLM-initiated compaction runs
- **AND** no duplicate compaction occurs
- **AND** the `CompactionAnalyticsAttempt.trigger` is recorded as `"tool-call"` (LLM call wins)
