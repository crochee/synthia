## ADDED Requirements

### Requirement: LoopDetector Canonical Implementation
The system SHALL have exactly one canonical `LoopDetector` implementation at `synthia-agent/src/agent/loop_detector.rs`. All other implementations SHALL delegate to it.

#### Scenario: LoopDetector delegates to canonical implementation
- **WHEN** `stream_builder` or `guardian` needs loop detection
- **THEN** it SHALL delegate to `agent/loop_detector.rs`

#### Scenario: Loop detection behavior unchanged
- **WHEN** loop detection is performed
- **THEN** the behavior SHALL be identical to before consolidation