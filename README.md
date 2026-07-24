# Synthia - AI Agent Framework

Synthia is a modular, high-performance AI Agent framework written in Rust. It implements the ReAct (Reasoning + Acting) pattern with comprehensive tool execution, session management, memory systems, and security features.

## Architecture

The framework is organized as a Rust workspace with 17 crates:

### Core Crates

| Crate | Description |
|-------|-------------|
| [synthia-core](crates/synthia-core/) | Common utilities (ID generation, time, paths, schemas) |
| [synthia-provider](crates/synthia-provider/) | LLM provider abstraction (OpenAI, Anthropic) |
| [synthia-model-router](crates/synthia-model-router/) | Dynamic model selection and routing |
| [synthia-tool](crates/synthia-tool/) | Tool execution with permissions and sandbox |
| [synthia-session](crates/synthia-session/) | Session lifecycle and state management |
| [synthia-memory](crates/synthia-memory/) | Hot/cold/episodic/context memory systems |
| [synthia-agent](crates/synthia-agent/README.md) | Core ReAct loop agent |
| [synthia-context](crates/synthia-context/) | Context assembly and compaction |
| [synthia-guardian](crates/synthia-guardian/) | Security loop detection and circuit breakers |
| [synthia-hook](crates/synthia-hook/) | Extensible hook system |
| [synthia-telemetry](crates/synthia-telemetry/) | Observability with OpenTelemetry |
| [synthia-mcp](crates/synthia-mcp/) | MCP server/client integration |
| [synthia-command](crates/synthia-command/) | Slash command system |
| [synthia-task](crates/synthia-task/) | Task scheduling and dispatch |

### Interface Crates

| Crate | Description |
|-------|-------------|
| [synthia-cli](crates/synthia-cli/) | CLI interface with REPL |
| [synthia-server](crates/synthia-server/) | HTTP/WebSocket server with axum |

### Support Crates

| Crate | Description |
|-------|-------------|
| [test-support](test-support/) | Mock implementations for testing |

## Quick Start

### Prerequisites

- Rust 1.75+ (2021 edition)
- cargo

### Build

```bash
cargo build --release
```

### Test

```bash
cargo test --workspace
```

### Lint

```bash
cargo clippy --workspace -- -D warnings
```

## Running

### CLI Mode

```bash
cargo run --bin synthia
```

### Server Mode

```bash
cargo run --bin synthia-server
```

## Project Structure

```
synthia/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── synthia-core/        # Common utilities
│   ├── synthia-provider/   # LLM providers
│   ├── synthia-model-router/
│   ├── synthia-tool/       # Tool execution
│   ├── synthia-agent/      # Core agent
│   ├── synthia-session/    # Session management
│   ├── synthia-memory/     # Memory system
│   ├── synthia-context/    # Context management
│   ├── synthia-guardian/   # Security
│   ├── synthia-hook/       # Hook system
│   ├── synthia-telemetry/  # Observability
│   ├── synthia-mcp/        # MCP integration
│   ├── synthia-command/    # Slash commands
│   ├── synthia-task/       # Task system
│   ├── synthia-cli/        # CLI interface
│   └── synthia-server/     # HTTP server
└── test-support/           # Test utilities
```

## Testing

All crates follow TDD methodology with comprehensive test coverage:

```bash
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test -p synthia-agent

# Run integration tests
cargo test -p synthia-agent --test react_loop_test

# Run end-to-end tests
cargo test -p synthia-agent --test e2e_llm_test
cargo test -p synthia-agent --test e2e_event_sequence_test
cargo test -p synthia-agent --test e2e_memory_correctness_test
```

## Development

- **TDD**: Write tests first, then implementation
- **Surgical Changes**: Touch only what's required
- **Simplicity**: Minimum code that solves the problem
- **Security**: Permission-based tool execution, sandbox isolation

## License

MIT
