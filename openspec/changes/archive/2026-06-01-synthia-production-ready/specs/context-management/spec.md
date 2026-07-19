## ADDED Requirements

### Requirement: Context SHALL apply progressive degradation in three stages
When context utilization exceeds thresholds, the system SHALL apply pruning in order: Stage 1 (Soft Trim at 30%-50%) → Stage 2 (Hard Clear at 50%-70%) → Stage 3 (分级压缩 at 70%-95%). Each stage SHALL only be applied when the previous stage is insufficient.

#### Scenario: Soft Trim reduces large tool results
- **WHEN** context utilization reaches 40% and there are large tool results
- **THEN** the system SHALL apply Soft Trim, keeping head and tail of tool results

#### Scenario: Hard Clear replaces old tool results with placeholders
- **WHEN** context utilization reaches 60% after Soft Trim
- **THEN** old tool results SHALL be replaced with `[cleared]` placeholders

### Requirement: Soft Trim SHALL preserve head and tail of tool results
When Soft Trim is applied, the first 500 tokens and last 500 tokens of the tool result SHALL be preserved. The middle portion SHALL be replaced with a trim summary indicating omitted size.

#### Scenario: 10KB tool result is soft-trimmed
- **WHEN** Soft Trim is applied to a 10KB tool result
- **THEN** the first 500 tokens and last 500 tokens SHALL be kept, middle replaced with `[trimmed: omitted X bytes]`

### Requirement: Context SHALL enforce safety thresholds
When available context falls below HARD_MIN (16K tokens), the system SHALL refuse to execute and end the session. When available context falls below WARN_BELOW (32K tokens), the system SHALL emit a warning and trigger Stage 3 pruning.

#### Scenario: Context below HARD_MIN
- **WHEN** available context drops below 16K tokens
- **THEN** the system SHALL refuse further execution and emit an error status

#### Scenario: Context below WARN_BELOW triggers warning
- **WHEN** available context drops below 32K tokens
- **THEN** the system SHALL emit a warning and trigger Stage 3 pruning

### Requirement: System SHALL track KV Cache prefix stability
The system SHALL compute prefix_hash (SHA-256 of system_prompt + skill_snapshot) before each API call. The system SHALL track prefix_stability_ratio (proportion of consecutive calls with unchanged prefix_hash) and expose it as a metric.

#### Scenario: Prefix hash remains stable
- **WHEN** the system prompt and skill snapshot have not changed between calls
- **THEN** the prefix_hash SHALL be identical and cache_hit SHALL be true

#### Scenario: Prefix hash changes and is tracked
- **WHEN** skill snapshot changes mid-session (first skill usage)
- **THEN** the prefix_hash SHALL change once and remain stable thereafter

### Requirement: Pruning SHALL apply differential treatment by event type in Stage 3
In Stage 3 pruning, events SHALL be handled by level: Level 1 (Decision, Error) preserved with full details; Level 2 (FileModified) compressed to one-line summary; Level 3 (FileRead, Output) deleted (retrievable from event log).

#### Scenario: Decision events survive Stage 3 pruning
- **WHEN** Stage 3 pruning is applied
- **THEN** Decision events SHALL retain their full details

#### Scenario: FileRead events are removed in Stage 3
- **WHEN** Stage 3 pruning is applied
- **THEN** FileRead events SHALL be removed from context but remain in the event log
