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
| [synthia-web](synthia-web/) | React/Vite frontend using the A2A protocol |

## Quick Start

### Prerequisites

- Rust 1.95+
- cargo
- Node.js 20+ (for `synthia-web`)
- Docker (optional, for containerized runs)

### Develop

```bash
make dev   # boots synthia-server (:8080) + synthia-web (:5173)
```

Open `http://localhost:5173`.

### Build

```bash
make build           # both server and web (debug)
make build-release   # release binaries
```

### Test

```bash
make test            # all Rust tests + frontend type-check
make test-rust       # cargo test --workspace
make test-e2e        # Playwright end-to-end
```

### Lint & Format

```bash
make lint            # clippy + tsc
make fmt             # cargo +nightly fmt + prettier
```

## Running

### CLI Mode

```bash
make dev-server   # or: cargo run --bin synthia
```

### Server + Web

```bash
make dev   # boots both with hot reload
```

See [DEPLOYMENT.md](./DEPLOYMENT.md) for production deployment,
Docker Compose, Nginx configuration, and environment variables.

## Makefile

The root `Makefile` is the single entry point for development,
build, test, format, lint, deploy, and Docker operations.
Run `make help` for the full list of targets.

## Project Structure

```
.
├── Cargo.toml                  # Rust workspace root
├── Makefile                    # unified dev/build/test/deploy entry point
├── Dockerfile.server           # synthia-server production image
├── Dockerfile.web              # synthia-web production image
├── docker-compose.yml          # development compose
├── docker-compose.prod.yml     # production compose (split deploy)
├── nginx.conf                  # reverse-proxy config (used by web image)
├── DEPLOYMENT.md               # deployment guide
├── crates/                     # synthia Rust crates
└── synthia-web/                # React frontend
    ├── src/
    │   ├── api/                # A2A client modules
    │   ├── components/         # UI components (ui/, layout/)
    │   ├── pages/              # Top-level pages
    │   ├── styles/             # Design tokens
    │   └── hooks/              # React hooks (useServerHealth, ...)
    └── tests/e2e/              # Playwright tests (3 layers)
```
