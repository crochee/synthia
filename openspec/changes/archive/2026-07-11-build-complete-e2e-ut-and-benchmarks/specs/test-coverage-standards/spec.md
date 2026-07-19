## ADDED Requirements

### Requirement: Test file naming conventions

All test files SHALL follow these naming patterns:
- Unit tests: `<module_name>_test.rs` placed adjacent to the module under test.
- Integration tests: `<feature>_integration_test.rs` placed in the crate's `tests/` directory.
- End-to-end tests: `e2e_<scenario>_test.rs` placed in the crate's `tests/` directory.

#### Scenario: E2E test naming
- **WHEN** a developer creates a new e2e test for the "session pause" scenario
- **THEN** the file SHALL be named `e2e_session_pause_test.rs` in `crates/synthia-agent/tests/`

#### Scenario: Unit test naming
- **WHEN** a developer creates a unit test for the `turn.rs` module
- **THEN** the test SHALL be placed in `crates/synthia-agent/src/turn_test.rs` or within `turn.rs` using `#[cfg(test)]`

---

### Requirement: Test categorization

Each test SHALL be categorized into exactly one of:
- **Unit**: tests a single module in isolation with mocked dependencies.
- **Integration**: tests interactions between two or more modules within a crate.
- **E2E**: tests a complete user-visible behavior end-to-end, possibly spanning multiple crates.

#### Scenario: Classifying an agent loop test
- **WHEN** a test runs the full ReAct loop with a mock LLM and checks the final state
- **THEN** it SHALL be classified as an E2E test and named `e2e_<scenario>_test.rs`

#### Scenario: Classifying a module unit test
- **WHEN** a test exercises only the `TurnTask::transition_to` method with mock session state
- **THEN** it SHALL be classified as a Unit test

---

### Requirement: Minimum test coverage per crate

The following crates SHALL maintain at least one test file:
- `synthia-agent`: at least 5 e2e tests and at least 3 unit test files.
- `synthia-session`: at least 2 integration test files.
- `synthia-tool`: at least 2 unit test files.
- `synthia-memory`: at least 1 unit test file.
- `synthia-context`: at least 1 unit test file.

#### Scenario: New crate adds first test
- **WHEN** a new crate `synthia-foo` is created
- **THEN** it SHALL have at least one test file before the crate is considered "tested"

---

### Requirement: Test documentation

Each crate that contains tests SHALL have a `tests/README.md` or a module-level doc comment explaining the test organization and run commands.

#### Scenario: Reading test documentation
- **WHEN** a developer joins the project and wants to know how to run tests
- **THEN** they SHALL find a README in `crates/<crate>/tests/README.md` or `tests/README.md` at the workspace root
