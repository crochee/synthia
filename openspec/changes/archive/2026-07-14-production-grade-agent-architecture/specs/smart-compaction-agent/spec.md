## ADDED Requirements

### Requirement: SmartCompactionAgent SHALL use backward token selection

`SmartCompactionAgent::select_tokens()` SHALL walk backward from the most recent message, keeping the newest messages up to `keep_tokens` (default: 8,000). Messages older than the budget are fed to the summarization LLM.

#### Scenario: Backward walk preserves recent messages
- **WHEN** context has 15,000 tokens of messages
- **AND** `keep_tokens` is 8,000
- **THEN** the algorithm SHALL walk backward from the most recent message
- **AND** keep the newest messages that fit within 8,000 tokens

#### Scenario: Overflowing message is split
- **WHEN** a message at the split boundary would exceed `keep_tokens`
- **THEN** the message SHALL be split
- **AND** the prefix SHALL be discarded (fed to summarization)
- **AND** the suffix SHALL be preserved in `recent`

#### Scenario: Prior compaction messages are excluded
- **WHEN** a `compaction` message exists in the conversation history
- **THEN** it SHALL be excluded from the token count
- **AND** only non-compaction messages SHALL be considered for preservation

---

### Requirement: SmartCompactionAgent SHALL use LLM for summarization

After token selection, `SmartCompactionAgent::summarize()` SHALL make a separate LLM call to generate a summary. The call SHALL use the same model as the main agent, with no tools, and capped output (default: 4,096 tokens).

#### Scenario: Summarization call uses template prompt
- **WHEN** `summarize()` is called with previous summary and selected head
- **THEN** a LLM call SHALL be made with the OpenCode-style template prompt
- **AND** the prompt SHALL include: previous summary, goal, constraints, progress, decisions, next steps, critical context, relevant files
- **AND** `generation.maxTokens` SHALL be 4,096

#### Scenario: Summarization failure falls back to truncation
- **WHEN** the LLM summarization call fails
- **THEN** `SmartCompactionAgent` SHALL fall back to simple truncation
- **AND** an error SHALL be logged but not propagated

#### Scenario: Empty summary abandons compaction
- **WHEN** the LLM returns empty or whitespace-only summary
- **THEN** the compaction SHALL be abandoned
- **AND** no compaction message SHALL be inserted
- **AND** the context SHALL remain unchanged

---

### Requirement: Compaction SHALL emit a structured compaction message

After successful compaction, a `compaction` message SHALL be inserted into the conversation history containing the LLM-generated summary and the preserved `recent` tail.

#### Scenario: Compaction message structure
- **WHEN** compaction succeeds
- **THEN** a message SHALL be inserted with:
  - `type: "compaction"`
  - `text`: the LLM-generated summary
  - `recent`: the preserved tail (newest messages)
  - `timestamp`: current time

#### Scenario: Compaction message excluded from future compaction
- **WHEN** future compaction runs
- **THEN** the previous compaction message SHALL be filtered out
- **AND** only non-compaction messages SHALL be considered

---

### Requirement: SmartCompactionAgent SHALL support incremental summary chaining

When multiple compactions occur, each subsequent compaction SHALL include the previous summary in the prompt so the summary grows incrementally.

#### Scenario: Incremental summary builds over compactions
- **WHEN** compaction #1 generates summary S1
- **AND** later compaction #2 runs with head H2
- **THEN** the prompt for compaction #2 SHALL include `<previous-summary>S1</previous-summary>`
- **AND** the resulting summary SHALL incorporate S1 and H2

---

### Requirement: Compaction trigger threshold SHALL be configurable

The compaction trigger threshold SHALL be configurable via `ContextConfig.compaction_buffer` (default: 20,000 tokens).

#### Scenario: Custom buffer affects trigger point
- **WHEN** `ContextConfig.compaction_buffer` is set to 30,000
- **THEN** compaction SHALL trigger when `context - max(output, buffer) < estimated_request_size`
- **AND** the agent SHALL have more buffer room for the final turn

---

### Requirement: Compaction SHALL run at most once per turn

If overflow occurs after compaction, the system SHALL fail with a hard error rather than running compaction again.

#### Scenario: Post-compaction overflow fails hard
- **WHEN** compaction runs successfully
- **AND** the next LLM call returns `context_overflow`
- **THEN** no further compaction SHALL be attempted
- **AND** the session SHALL return an error indicating context cannot be reduced
