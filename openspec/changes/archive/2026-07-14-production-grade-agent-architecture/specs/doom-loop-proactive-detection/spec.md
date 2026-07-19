## ADDED Requirements

### Requirement: DoomLoopDetector SHALL track sliding window of tool call signatures

`DoomLoopDetector` SHALL maintain a sliding window of the most recent N tool call signatures (tool name + args hash). When the window fills with identical signatures, detection triggers.

#### Scenario: Detection after 3 identical calls
- **WHEN** tool "read" is called with identical arguments three consecutive times
- **THEN** after the third call, `DoomLoopDetector::check()` SHALL return `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`

#### Scenario: Different arguments reset the window
- **WHEN** tool "read" is called with `{"path": "/foo"}`
- **AND** then called with `{"path": "/bar"}`
- **AND** then called with `{"path": "/foo"}` again
- **THEN** the window SHALL contain three different entries
- **AND** no doom loop SHALL be detected

#### Scenario: Different tool name resets the window
- **WHEN** tool "read" is called twice with identical arguments
- **AND** then tool "write" is called
- **THEN** the window SHALL contain only the write call
- **AND** doom loop detection SHALL restart from that point

---

### Requirement: DoomLoopDetector SHALL use hash-based signature comparison

Tool call signatures SHALL be compared using a hash of `(tool_name, JSON.stringify(args))` rather than full string comparison for performance. Hash collisions SHALL fall back to full comparison.

#### Scenario: Hash-based comparison detects match
- **WHEN** two tool calls have identical (tool_name, args_json)
- **THEN** their hashes SHALL be identical
- **AND** `DoomLoopDetector` SHALL detect the match

#### Scenario: Hash collision falls back to full comparison
- **WHEN** two different signatures produce the same hash (collision)
- **THEN** the detector SHALL fall back to full JSON string comparison
- **AND** only identical full signatures SHALL be considered matches

---

### Requirement: DoomLoopDetector SHALL emit RequirePermission at threshold

When the sliding window contains `DOOM_LOOP_THRESHOLD` (default: 3) consecutive identical signatures, `check()` SHALL return `RequirePermission` action.

#### Scenario: RequirePermission action emitted at threshold
- **WHEN** 3 consecutive identical tool calls are detected
- **THEN** `check()` SHALL return `(LoopStatus::Detected { severity: Critical }, Some(LoopAction::RequirePermission))`
- **AND** the status SHALL include the `tool_name` and `input_hash`

#### Scenario: Caller handles RequirePermission by prompting
- **WHEN** the agent receives `(LoopStatus::Detected, Some(RequirePermission))`
- **THEN** the agent SHALL call `permission.ask(doom_loop, { tool, input_hash })`
- **AND** the agent SHALL NOT execute the tool until permission is granted

#### Scenario: Caller ignoring RequirePermission falls back to block
- **WHEN** a caller does NOT handle `RequirePermission` explicitly
- **THEN** the caller SHALL still be able to block based on `LoopStatus::Detected`
- **AND** worst-case behavior SHALL match pre-change behavior (block without asking)

---

### Requirement: DOOM_LOOP_THRESHOLD SHALL be configurable

`DOOM_LOOP_THRESHOLD` (default: 3) SHALL be configurable via `AgentConfig.doom_loop_threshold`.

#### Scenario: Custom threshold takes effect
- **WHEN** `AgentConfig.doom_loop_threshold` is set to 5
- **THEN** `DoomLoopDetector` SHALL trigger after 5 consecutive identical calls
- **AND** threshold of 3 SHALL trigger after 3 calls (default)

---

### Requirement: DoomLoopDetector detection SHALL be independent of iteration counter

`DoomLoopDetector::check()` SHALL NOT depend on the iteration number passed to it. Detection SHALL be based solely on the tool call signature history.

#### Scenario: Iteration number does not affect detection
- **WHEN** `check()` is called with the same signature at iteration 1 and iteration 100
- **THEN** the behavior SHALL be identical
- **AND** the iteration parameter SHALL have no effect on doom loop detection
