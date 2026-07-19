## ADDED Requirements

### Requirement: System SHALL apply five-layer progressive error recovery
The system SHALL recover from errors through five layers in order: L1 Truncate (output > 16KB) → L2 Retry (timeout/temporary error, max 2 attempts with exponential backoff) → L3 Fallback (same tool fails 2 consecutive times, use degraded path + reload skill guide) → L4 Auto-Compact (context > 80% and pruning insufficient, compress and generate summary) → L5 Reset (30 consecutive failures, rebuild session + fail-fast).

#### Scenario: Truncate handles oversized output
- **WHEN** a tool returns 50KB of output
- **THEN** L1 SHALL truncate to head + tail with truncation marker

#### Scenario: Retry recovers from temporary network error
- **WHEN** a web_fetch tool fails due to connection timeout
- **THEN** L2 SHALL retry up to 2 times with exponential backoff

### Requirement: Error recovery SHALL prevent L4-L5 deadlock loop
The system SHALL maintain a global error counter per session. If recovery cycles exceed 3 times within the same session, the system SHALL fail-fast immediately. After L5 Reset, there SHALL be a 30-second cooldown period during which L4 SHALL NOT trigger. L5 Reset SHALL reset the consecutive_failures counter.

#### Scenario: Deadlock loop is broken by fail-fast
- **WHEN** Auto-Compact fails, triggering Reset, and the new session immediately triggers Auto-Compact again
- **THEN** after 3 such cycles, the system SHALL fail-fast and exit

### Requirement: React loop errors SHALL trigger proper cleanup
When the ReAct loop exits due to an error, the system SHALL call loop_detector.reset_circuit_breaker(), drain the steering channel, and clean up all resources including background processes through CommandManager.

#### Scenario: Error exit cleans up circuit breaker
- **WHEN** the ReAct loop exits due to an unrecoverable error
- **THEN** the circuit breaker counter SHALL be reset to zero

### Requirement: Fallback layer SHALL provide degraded paths for common tools
The Fallback layer SHALL provide degraded alternatives: web_fetch → cached version or "network unavailable"; complex bash → simplified command; subagent → direct answer without subagent; memory_search → grep local files; read_file (large) → read first 100 lines only.

#### Scenario: Web fetch falls back to cached version
- **WHEN** web_fetch fails 2 consecutive times
- **THEN** L3 SHALL return a cached version or "network unavailable" message
