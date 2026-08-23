# Synthia - AI Agent Framework

<p align="left">
  <img src="synthia-web/public/logo.svg" alt="Synthia" height="56">
</p>

Synthia is a modular, high-performance AI Agent framework written in Rust. It implements the ReAct (Reasoning + Acting) pattern with comprehensive tool execution, session management, and observability hooks.

## Architecture

The framework is organized as a Rust workspace with **9 crates** (8 library crates + `test-support`). The front-end sits next to the workspace as `synthia-web/` with its own `package.json`.

### Core Crates

| Crate | Description |
|-------|-------------|
| [synthia-core](crates/synthia-core/) | Common utilities (ID generation, time, paths, schemas) |
| [synthia-provider](crates/synthia-provider/) | LLM provider abstraction (OpenAI, Anthropic) |
| [synthia-tool](crates/synthia-tool/) | Tool registry, execution, and sub-trait composition |
| [synthia-session](crates/synthia-session/) | Session lifecycle and state management |
| [synthia-agent](crates/synthia-agent/README.md) | Core ReAct loop agent (reasoning + tool execution) |
| [synthia-telemetry](crates/synthia-telemetry/) | Observability with OpenTelemetry (optional `otel` feature) |

### Interface Crates

| Crate | Description |
|-------|-------------|
| [synthia-server](crates/synthia-server/) | HTTP/WebSocket server with axum, exposes the REST + SSE chat surface |

### Support Crates

| Crate | Description |
|-------|-------------|
| [test-support](test-support/) | Shared mock implementations for cross-crate testing |
| [synthia-web](synthia-web/) | React/Vite frontend speaking the REST + SSE chat surface |

### Protocol / Cache / Skill Crates

| Crate | Description |
|-------|-------------|
| [synthia-skill](crates/synthia-skill/) | Skill registry (slash-command, prompt, and tool bundles) |

### Workspace Members

The 8 crates listed above match the `[workspace.members]` array in the root
`Cargo.toml`. Every path below is a real on-disk directory:

| Path | One-line responsibility |
|------|-------------------------|
| `crates/synthia-core` | Cross-cutting utilities (IDs, time, paths, error schemas) |
| `crates/synthia-telemetry` | Tracing + optional OTel pipeline |
| `crates/synthia-provider` | LLM provider trait, OpenAI / Anthropic adapters |
| `crates/synthia-tool` | Tool registry, executor, built-in toolset |
| `crates/synthia-skill` | Skill registry + loader |
| `crates/synthia-session` | Session lifecycle + cleanup daemon |
| `crates/synthia-agent` | ReAct loop agent (the "AI" of Synthia) |
| `crates/synthia-server` | HTTP / WebSocket server (axum, REST + SSE chat surface) |
| `test-support` | Mock fixtures shared by integration tests |

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

### Server

```bash
make dev-server   # or: cargo run -p synthia-server
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
├── crates/                     # synthia Rust crates (8 libraries)
│   ├── synthia-core/
│   ├── synthia-telemetry/
│   ├── synthia-provider/
│   ├── synthia-tool/
│   ├── synthia-skill/
│   ├── synthia-session/
│   ├── synthia-agent/
│   └── synthia-server/
├── test-support/               # shared mock fixtures
└── synthia-web/                # React frontend
    ├── src/
    │   ├── api/                # REST + SSE client modules
    │   ├── components/         # UI components (ui/, layout/)
    │   ├── pages/              # Top-level pages
    │   ├── styles/             # Design tokens
    │   └── hooks/              # React hooks (useServerHealth, ...)
    └── tests/e2e/              # Playwright tests (3 layers)
```
