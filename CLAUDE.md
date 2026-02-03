# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Synthia is a modular AI agent framework in Rust, implementing the ReAct (Reasoning + Acting) pattern. It provides language model integration with streaming, flexible tool execution, context management, model routing, job scheduling, and distributed tracing.

## Build & Development Commands

Rust 代码，如果没有使用的情况请删除，不能使用 `dead_code` 和`unused`标签忽略。

```bash
# Build
cargo build --release

# Run CLI (default member)
cargo run --package synthia-cli

# Run examples
cargo run --package synthia-examples --example agent_demo
cargo run --package synthia-examples --example model_chat
cargo run --package synthia-examples --example tool_call
cargo run --package synthia-examples --example job_scheduler

# Test
cargo test --lib                     # Unit tests only
cargo test -p synthia-agent --lib    # Test specific crate
cargo test --test '*'                # Integration tests
cargo test --test '*' -p synthia-agent  # Integration tests for synthia-agent

# Lint & Format
cargo clippy --workspace --all-targets
cargo +nifmt fmt --check --all

# Check
cargo check --workspace --all-targets
```

## Architecture

### Crates

- **`synthia-agent`** — Core AI agent with ReAct loop (`agent/react.rs`), tool system, context management, guardian safety system, model routing, memory management, and session handling.
- **`synthia-provider`** — LLM provider abstraction with OpenAI-compatible and Anthropic implementations.
- **`synthia-job`** — Time-wheel based job scheduler for periodic tasks.
- **`synthia-tracing`** — OpenTelemetry distributed tracing.
- **`synthia-cli`** — Interactive CLI with streaming chat.
- **`synthia-server`** — HTTP/WebSocket server with MCP support.

### Core Patterns

- **ReAct Loop**: `synthia-agent/src/agent/react.rs` — reasoning + acting iteration
- **Tool Trait**: `async_trait::async_trait` with `Tool` trait in `tools/mod.rs`
- **Model Provider**: `ModelProvider` trait with streaming support in synthia-provider
- **Dependency Injection**: `AgentDeps` struct holds all runtime dependencies
- **Context Management**: Token-aware context window with compression strategies

### synthia-agent Structure

```
src/
├── agent/           # ReAct loop, step processing, model calling
├── tools/           # Tool implementations (fs, web, cron, MCP, memory, todo, skill, subagent, thinking, exec, tom, worktree)
├── context/         # Context window management & compression
├── config/          # AgentConfig, SessionConfig
├── guardian/        # Safety & approval system
├── model_router/    # Intelligent model selection
├── memories/        # Memory management (phase1, phase2, session, cron)
├── prompt/          # System prompt building
├── session/         # Session management
├── hooks/           # Event-driven extensibility
├── shell/           # Shell execution abstraction
├── types/           # Core type definitions
└── event_handler/   # Event processing
```

## Configuration

- Configuration via `config.yaml` or `config.toml`
- `RUST_LOG` environment variable for logging level
- Provider config supports OpenAI-compatible and Anthropic APIs

## Toolchain

- **Edition**: 2024
- **Toolchain**: stable
- **Rust MSRV**: 1.70+
