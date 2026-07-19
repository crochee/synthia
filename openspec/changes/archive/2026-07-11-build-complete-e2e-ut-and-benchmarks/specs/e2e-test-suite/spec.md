## ADDED Requirements

### Requirement: E2E test scenarios

The e2e test suite SHALL cover at least the following scenarios:

#### Scenario: Single-turn agent interaction
- **WHEN** the agent receives a single user message and produces a response
- **THEN** the e2e test SHALL verify that the response is non-empty and the session state is consistent

#### Scenario: Multi-turn conversation
- **WHEN** the agent receives 3 consecutive user messages in one session
- **THEN** the e2e test SHALL verify that all 3 turns complete and the conversation context is preserved

#### Scenario: Session pause and resume
- **WHEN** a session is paused mid-turn and then resumed
- **THEN** the e2e test SHALL verify that the resumed session continues from the correct state

#### Scenario: Tool call execution
- **WHEN** the agent calls a tool (e.g., read_file, bash)
- **THEN** the e2e test SHALL verify the tool result is incorporated into the agent's next response

#### Scenario: Guardian permission gate
- **WHEN** the agent attempts an operation that triggers the guardian permission gate
- **THEN** the e2e test SHALL verify that the operation is blocked and a clear error is returned

#### Scenario: Session teardown
- **WHEN** a session ends cleanly (all turns completed)
- **THEN** the e2e test SHALL verify that all events are flushed and the session log is complete

---

### Requirement: E2E test infrastructure

E2E tests SHALL use the existing `test_support` crate for mock LLM, mock tool registry, and in-memory session store.

#### Scenario: Setting up a mock LLM for e2e tests
- **WHEN** an e2e test needs a mock LLM that returns a predefined response
- **THEN** it SHALL use `test_support::mock::MockLlm` or equivalent from the test-support crate

---

### Requirement: E2E test isolation

Each e2e test SHALL run in isolation with a unique temporary session directory and SHALL clean up after itself.

#### Scenario: Test isolation
- **WHEN** e2e_test_a and e2e_test_b run concurrently
- **THEN** they SHALL NOT share any session state or file system paths
- **AND** each SHALL clean up its temporary directory after completion

---

### Requirement: E2E test naming

Each e2e test file SHALL contain exactly one scenario and SHALL be named `e2e_<scenario_name>_test.rs`.

#### Scenario: E2E test file naming
- **WHEN** a developer writes an e2e test for session timeout behavior
- **THEN** the file SHALL be named `e2e_session_timeout_test.rs`

---

### Requirement: E2E test pass criteria

An e2e test SHALL be considered passing when:
- The agent produces a response without panicking.
- All expected events are emitted to the session log.
- No guardian permission violations occur (unless testing the gate itself).
- The session directory is left in a clean state after teardown.

#### Scenario: E2E test pass
- **WHEN** all assertions in an e2e test pass
- **THEN** the test SHALL return a passing status
