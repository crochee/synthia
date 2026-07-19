## ADDED Requirements

### Requirement: Compaction threshold SHALL be configurable via AgentConfig

`AgentConfig` SHALL expose a `compaction_threshold: Option<usize>` field. When `None`, the system SHALL use the hardcoded default of 100,000 tokens. When `Some(n)`, the threshold SHALL be `n` tokens.

#### Scenario: Default threshold when not configured
- **WHEN** `AgentConfig.compaction_threshold = None`
- **THEN** compaction SHALL trigger at 100,000 tokens

#### Scenario: Custom threshold when configured
- **WHEN** `AgentConfig.compaction_threshold = Some(50_000)`
- **THEN** compaction SHALL trigger at 50,000 tokens

### Requirement: Compaction threshold SHALL be validated

If `compaction_threshold` is set, the value SHALL be validated: greater than 0 and less than or equal to the model's context limit (if known).

#### Scenario: Zero threshold rejected
- **WHEN** `AgentConfig.compaction_threshold = Some(0)` is validated
- **THEN** validation SHALL fail with an error

#### Scenario: Excessive threshold warned
- **WHEN** `AgentConfig.compaction_threshold = Some(1_000_000)` for a model with 128K context
- **THEN** a warning SHALL be logged but the value SHALL be accepted