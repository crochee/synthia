# token-budget-observability Specification

## Purpose
TBD - created by archiving change fix-agent-critical-bugs-and-production-gaps. Update Purpose after archive.
## Requirements
### Requirement: TokenBudgetWarning SHALL emit actual cumulative token count

When a token budget warning is triggered, the emitted `TokenBudgetWarning` event SHALL include the actual `current_tokens` value representing the cumulative token count since session start, not a hardcoded zero.

#### Scenario: Token count is not hardcoded to zero
- **WHEN** `TokenBudgetWarning` event is emitted
- **THEN** `current_tokens` SHALL be the actual cumulative token count

### Requirement: TokenBudgetWarning SHALL emit configured threshold value

When a token budget warning is triggered, the emitted `TokenBudgetWarning` event SHALL include the `threshold_tokens` value from the agent's `context_token_budget` configuration, specifically the `hard_limit` value.

#### Scenario: Threshold reflects actual config
- **WHEN** `TokenBudgetWarning` event is emitted
- **THEN** `threshold_tokens` SHALL be the configured `hard_limit` value

### Requirement: Agent SHALL accumulate token usage across iterations

The agent SHALL update `ctx.cumulative_tokens` after each LLM sampling step completes, using the `usage.total()` from the sampling result.

#### Scenario: Warning at 80% of budget
- **WHEN** cumulative tokens reach 80% of the configured hard_limit
- **THEN** `TokenBudgetWarning` event SHALL be emitted with accurate `current_tokens` and `threshold_tokens` values

#### Scenario: Compaction trigger at budget
- **WHEN** cumulative tokens exceed the configured hard_limit
- **THEN** `MustCompact` status SHALL be emitted with accurate token values
- **AND** the agent SHALL attempt compaction before the next sampling step

---

