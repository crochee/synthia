## ADDED Requirements

### Requirement: Compaction trigger SHALL use precise token count from tiktoken

The compaction decision SHALL be based on the precise token count from tiktoken, not on estimation. The trigger condition is: `tiktoken_token_count >= compaction_threshold`.

#### Scenario: Compaction triggers at exact threshold
- **WHEN** `compaction_threshold = 100_000` and tiktoken counts exactly 100,000 tokens
- **THEN** compaction SHALL be triggered

#### Scenario: Compaction does not trigger below threshold
- **WHEN** `compaction_threshold = 100_000` and tiktoken counts 99,999 tokens
- **THEN** compaction SHALL NOT be triggered

### Requirement: TokenBudgetWarning SHALL use precise token count

The `TokenBudgetWarning` event SHALL include the precise tiktoken count as `current_tokens`, not an estimated value.

#### Scenario: Warning at 80% of configured threshold
- **WHEN** `compaction_threshold = 100_000` and tiktoken counts 80,000 tokens
- **THEN** `TokenBudgetWarning` SHALL be emitted with `current_tokens = 80,000`

### Requirement: MustCompact SHALL use precise token count

The `MustCompact` status SHALL be emitted with the precise tiktoken count when the threshold is reached or exceeded.

#### Scenario: MustCompact emitted at threshold
- **WHEN** tiktoken count reaches 100,000 tokens with `compaction_threshold = 100_000`
- **THEN** `MustCompact` status SHALL be emitted with `current_tokens = 100,000`