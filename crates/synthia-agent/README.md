# synthia-agent

The core ReAct loop agent for the Synthia AI Agent framework.

## Features

- **ReAct Loop**: Thought → Action → Observation cycle
- **Task-Centric Execution**: Structured task decomposition with `Task`, dependency tracking, and status transitions
- **Tool Dispatch**: Execute tools with permission and sandbox checks
- **Streaming**: Real-time event streaming to clients
- **Cancellation**: CancellationToken support for graceful shutdown
- **Hooks**: Before/After hooks for LLM calls, tool execution, and more

## Architecture

Synthia Agent decomposes work into structured `Task` objects
(`synthia_task::types::Task`) that are dispatched to sub-agents or
executed directly. Each task carries `description`, `owner`,
`status` (Pending / Running / Done / Failed / Blocked), and
`ProgressState`. Dependencies between tasks are tracked by
`TaskRegistry::add_dependency`.

### Sub-Task Context & Results

When an agent dispatches a sub-task, it builds a `TaskContext`
that the sub-agent reads to scope its work:

```rust,ignore
use synthia_agent::task::types::{CodeSnippet, TaskContext, TaskPriority};

let context = TaskContext::new(
    "Implement the user authentication module".to_string(),
)
.with_files(vec!["src/auth.rs".to_string()])
.with_snippets(vec![CodeSnippet::new(
    "existing_user",
    "pub struct User { /* ... */ }",
)])
.with_constraints(vec!["Must use JWT tokens with 1-hour expiry".to_string()]);

// The agent then runs the sub-task with the configured priority:
let priority = TaskPriority::High;
```

The sub-agent returns a structured `TaskResult`:

```rust,ignore
use synthia_agent::task::types::{TaskResult, TaskStatus};

let result = TaskResult::success("auth module implemented".to_string());
assert_eq!(result.status, TaskStatus::Success);
assert!(result.exit_code.is_none() || result.exit_code == Some(0));
```

## Usage

See `examples/` for end-to-end usage. The entry point is
`Agent::run_stream(AgentRunConfig)`.
