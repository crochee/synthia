# Synthia Test Suite

## Test Categories

Tests are categorized into three levels:

| Category | Scope | Location | Naming Pattern |
|----------|-------|----------|---------------|
| **Unit** | Single module in isolation | Adjacent to source (`src/`) | `<module>_test.rs` or `#[cfg(test)]` |
| **Integration** | Multiple modules within a crate | Crate's `tests/` directory | `<feature>_integration_test.rs` |
| **E2E** | Complete user-visible behavior, may span crates | Crate's `tests/` directory | `e2e_<scenario>_test.rs` |

## Naming Conventions

### E2E Tests
- File: `e2e_<scenario>_test.rs`
- Test function: `test_<specific_behavior>`
- Example: `e2e_session_pause_resume_test.rs`

### Unit Tests
- File: `<module>_test.rs` adjacent to the module under test
- Inline: `#[cfg(test)]` modules within source files

### Integration Tests
- File: `<feature>_integration_test.rs`
- Example: `session_lifecycle_integration_test.rs`

## Run Commands

### Full Test Suite
```bash
cargo test --workspace
```

### Per Crate
```bash
cargo test -p synthia-agent
cargo test -p synthia-session
cargo test -p synthia-tool
cargo test -p synthia-context
```

### Specific Test Categories
```bash
# Run all e2e tests for synthia-agent
cargo test -p synthia-agent e2e_

# Run unit tests only
cargo test -p synthia-agent --lib

# Run integration tests
cargo test -p synthia-session --test '*_integration*'
```

## Coverage Expectations

### Crate Coverage Requirements

| Crate | E2E Tests | Unit/Integration Tests |
|-------|-----------|----------------------|
| `synthia-agent` | 5+ e2e tests | 3+ unit test files |
| `synthia-session` | - | 2+ integration test files |
| `synthia-tool` | - | 2+ unit test files |
| `synthia-context` | - | 1+ unit test files |

## CI Expectations

- All tests run on every PR
- Benchmarks are informational (not blocking)
- Clippy and fmt checks must pass
