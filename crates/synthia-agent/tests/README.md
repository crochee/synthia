# Synthia Agent Test Suite

## Test Organization

The `synthia-agent/tests/` directory contains end-to-end (E2E) and integration tests that exercise the full ReAct loop with mocked or real LLM providers.

### Directory Structure

```
tests/
├── test_support.rs           # Shared test infrastructure (FakeProvider, mock tools)
├── react_loop_test.rs        # Core ReAct loop tests
├── e2e_llm_test.rs          # E2E tests with real LLM (some #[ignore]d)
├── e2e_event_sequence_test.rs # Event ordering E2E tests
└── e2e_*.rs                 # Scenario-specific E2E tests
```

## Naming Conventions

| Pattern | Type | Example |
|---------|------|---------|
| `e2e_<scenario>_test.rs` | End-to-end | `e2e_multi_turn_conversation_test.rs` |
| `<feature>_test.rs` | Unit/Component | `turn_id_test.rs`, `span_hierarchy_test.rs` |
| `<feature>_integration_test.rs` | Integration | (when added) |

## Test Categories

### Unit Tests
Single module tests using mocks. Fast, deterministic, no external dependencies.
- Located in `src/<module>_test.rs` within the crate

### Integration Tests
Tests across multiple modules within the crate using `FakeProvider`.
- Located in `tests/` directory

### E2E Tests
Full ReAct loop tests exercising real LLM integration or complex scenarios.
- Named `e2e_<scenario>_test.rs`
- May be `#[ignore]`d requiring real credentials

## Run Commands

```bash
# Run all synthia-agent tests
cargo test -p synthia-agent

# Run all tests including ignored E2E tests
cargo test -p synthia-agent -- --ignored

# Run only E2E tests
cargo test -p synthia-agent e2e_

# Run specific test file
cargo test -p synthia-agent --test e2e_llm_test

# Run tests matching pattern
cargo test -p synthia-agent -- <pattern>
```

## Adding a New Test

1. Choose the appropriate naming pattern for your test type
2. Create the file in `tests/` for integration/E2E tests
3. Use `test_support::FakeProvider` for fast/deterministic tests
4. Use `#[tokio::test]` for async tests
5. Each test should clean up its temporary session directory

## Test Support Infrastructure

The `test_support.rs` module provides:
- `FakeProvider` - Mock LLM that returns scripted responses
- Mock tool registry
- In-memory session store for isolation