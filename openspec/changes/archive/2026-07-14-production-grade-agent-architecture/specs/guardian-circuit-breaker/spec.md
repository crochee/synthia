## ADDED Requirements

### Requirement: DoomLoopDetector SHALL complement GuardianCircuitBreaker

The `DoomLoopDetector` SHALL operate alongside `GuardianCircuitBreaker`, not replace it. Each detects different failure modes:

- `DoomLoopDetector`: Detects actual repeated tool calls with identical (tool_name, args) via sliding window. Proactive detection triggers permission prompt at 3 consecutive identical calls.
- `GuardianCircuitBreaker`: Detects permission denial patterns (consecutive/total denials). Reactive detection triggers session interrupt.

Both can trigger protective actions; they are complementary.

#### Scenario: DoomLoopDetector detects repeated calls
- **WHEN** the same tool is called with identical arguments 3 times consecutively
- **THEN** `DoomLoopDetector` SHALL return `LoopStatus::Detected`
- **AND** `GuardianCircuitBreaker` SHALL NOT be affected
- **AND** the agent SHALL receive `RequirePermission` action

#### Scenario: GuardianCircuitBreaker detects denial pattern
- **WHEN** the agent makes 3 consecutive requests that Guardian denies
- **THEN** `GuardianCircuitBreaker` SHALL set `session_interrupt = true`
- **AND** `DoomLoopDetector` SHALL NOT be affected

#### Scenario: Both detectors operate independently
- **WHEN** a doom loop occurs AND denial pattern also emerges
- **THEN** both detectors SHALL operate independently
- **AND** both protective actions MAY be triggered
- **AND** each detector SHALL maintain its own state
