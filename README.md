# Synthia

[![Rust](https://img.shields.io/badge/Rust-1.70+-dea584.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Synthia is a modular AI agent framework built with Rust.

## Overview

Synthia provides a comprehensive set of tools for building sophisticated AI applications. It features advanced language model integration with streaming support, a flexible tool execution system, intelligent task planning with ReAct pattern, and built-in distributed tracing.

## Key Features

- **ReAct Agent**: Implementation of the ReAct (Reasoning + Acting) pattern for intelligent task execution
- **Multi-Provider Support**: Unified interface for OpenAI-compatible and Anthropic providers
- **Tool System**: Extensible tool registry with built-in tools (filesystem, web, cron, MCP, memory, etc.)
- **Context Management**: Intelligent conversation context compression and retention
- **Model Routing**: Multi-model support with configurable fallback strategies
- **Job Scheduling**: Time-wheel based job scheduler for periodic task execution
- **Distributed Tracing**: OpenTelemetry-based tracing and structured logging

## Crates

Synthia is organized into the following core modules:

| Crate | Description |
|-------|-------------|
| [synthia-agent](crates/synthia-agent) | Core AI agent with ReAct execution, tool management, context handling, and built-in tools (fs, web, cron, MCP, memory, todo, skill, subagent) |
| [synthia-cli](crates/synthia-cli) | Interactive CLI tool with streaming chat support |
| [synthia-job](crates/synthia-job) | Time-wheel based job scheduler for periodic task execution |
| [synthia-provider](crates/synthia-provider) | LLM provider abstraction with OpenAI-compatible and Anthropic implementations |
| [synthia-tracing](crates/synthia-tracing) | Distributed tracing and structured logging using OpenTelemetry |

## Quick Start

### Prerequisites

- Rust 1.70+ (edition 2024)
- Cargo (Rust package manager)

### Installation

```bash
# Clone the repository
git clone https://github.com/crochee/synthia.git
cd synthia

# Build the project
cargo build --release

# Run the CLI
cargo run --package synthia-cli
```

### Usage Examples

Run the included examples:

```bash
# Agent demo with ReAct execution
cargo run --package synthia-examples --example agent_demo

# Direct model chat with streaming
cargo run --package synthia-examples --example model_chat

# Tool calling demonstration
cargo run --package synthia-examples --example tool_call

# Job scheduler example
cargo run --package synthia-examples --example job_scheduler
```

#### Agent with ReAct Pattern

```rust
use synthia_agent::Agent;
use synthia_agent::config::{AgentConfig, SessionConfig};
use synthia_agent::tools::ToolRegistry;
use synthia_agent::AgentEvent;
use synthia_provider::{ProviderConfig, Provider, Message};

#[tokio::main]
async fn main() -> Result<()> {
    let provider_config = ProviderConfig::from_env_auto()?;
    let provider = Provider::new(provider_config);

    let config = AgentConfig::builder()
        .name("assistant")
        .description("A helpful AI assistant")
        .build();

    let tool_registry = ToolRegistry::new();

    let agent = Agent::builder()
        .config(config)
        .provider(provider)
        .tool_registry(tool_registry)
        .build()?;

    let session_config = SessionConfig::default();
    let message = Message::user("Your message here");

    let mut stream = agent.reply(message, &session_config, None).await?;

    while let Some(event) = stream.next().await {
        match event? {
            AgentEvent::Message(msg) => print!("{}", msg.content),
            AgentEvent::ModelChange { model, .. } => println!("[Model: {}]", model),
            AgentEvent::LoopExited { reason } => println!("[Done: {:?}]", reason),
            _ => {}
        }
    }

    Ok(())
}
```

#### Multi-Provider Setup

```rust
use synthia_provider::{ProviderConfig, Provider};

let config = ProviderConfig::builder()
    .provider(ProviderType::OpenAI)
    .model("gpt-4o")
    .api_key(std::env::var("OPENAI_API_KEY")?)
    .build();

let provider = Provider::new(config);

// Or use Anthropic
let anthropic_config = ProviderConfig::builder()
    .provider(ProviderType::Anthropic)
    .model("claude-sonnet-4-20250514")
    .api_key(std::env::var("ANTHROPIC_API_KEY")?)
    .build();
```

#### Job Scheduler

```rust
use synthia_job::{TimeWheel, Job, every};
use std::time::Duration;
use std::sync::Arc;

struct HelloJob {
    name: String,
    counter: std::sync::atomic::AtomicU32,
}

#[async_trait]
impl Job for HelloJob {
    fn description(&self) -> &str {
        "A simple hello world job"
    }

    fn key(&self) -> &str {
        &self.name
    }

    async fn execute(&self) {
        let count = self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        println!("[{}] Hello! (execution #{})", self.name, count + 1);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let wheel = Arc::new(TimeWheel::new());
    let job = Arc::new(HelloJob {
        name: "hello_job".to_string(),
        counter: std::sync::atomic::AtomicU32::new(0),
    });
    let trigger = every(Duration::from_secs(2));

    wheel.schedule_async(job, Arc::new(trigger)).await?;

    tokio::time::sleep(Duration::from_secs(10)).await;

    Ok(())
}
```

## Built-in Tools

The agent comes with comprehensive built-in tools:

| Category | Tools |
|----------|-------|
| **Filesystem** | `readFile`, `writeFile`, `editFile`, `deleteFile`, `glob`, `grep`, `listDirectory`, `createDirectory`, `moveFile`, `directoryTree` |
| **Web** | `webFetch` |
| **Cron** | `cron_add`, `cron_remove`, `cron_update`, `cron_list`, `cron_get`, `cron_run`, `cron_runs` |
| **MCP** | Dynamic MCP server integration via adapter |
| **Memory** | `memory_store`, `memory_recall` |
| **Todo** | `todoWrite` |
| **Skill** | `loadSkill` |
| **Subagent** | `spawn_agent` |
| **Thinking** | `sequentialthinking` |
| **Execution** | `exec` |
| **User Interaction** | `askUserQuestion` |
| **TOM** | `tom` |

## Configuration

### Environment Variables

- `RUST_LOG`: Logging level (e.g., `info`, `debug`)

### YAML Configuration

Reference the example configuration file:

```bash
cp example/config.sample.yaml config.yaml
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

Licensed under the [MIT License](LICENSE).

## Contact

- GitHub: [https://github.com/crochee/synthia](https://github.com/crochee/synthia)

---

*Built with Rust for performance and reliability.*
