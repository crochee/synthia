# Spec: agent-executor-trait

## ADDED Requirements

### Requirement: AgentExecutor trait
The system SHALL define an `AgentExecutor` trait:

```rust
trait AgentExecutor {
    async fn run(&self, prompt: &str, config: RunConfig) -> Result<AgentOutput>;
    async fn resume(&self, session: &mut AgentSession, prompt: &str) -> Result<AgentOutput>;
}
```

`run()` SHALL create a new session for execution; `resume()` SHALL continue execution on an existing session.

#### Scenario: run creates new session
- **WHEN** `executor.run(prompt, config)` is called
- **THEN** a new `AgentSession` is created and the prompt is executed within it, returning `AgentOutput`

### Requirement: AgentStreamExecutor trait
The system SHALL define an `AgentStreamExecutor` trait extending `AgentExecutor`:

```rust
trait AgentStreamExecutor: AgentExecutor {
    async fn run_stream(&self, prompt: &str, config: RunConfig) -> Result<AgentOutputStream>;
    async fn resume_stream(&self, session: &mut AgentSession, prompt: &str) -> Result<AgentOutputStream>;
}
```

#### Scenario: stream execution returns output stream
- **WHEN** `executor.run_stream(prompt, config)` is called
- **THEN** a new session is created and an `AgentOutputStream` is returned for streaming the response

### Requirement: AgentHandle impl traits
`AgentHandle` SHALL implement both `AgentExecutor` and `AgentStreamExecutor`. `run_stream_with_state` SHALL be removed and unified as `resume_stream`.

#### Scenario: handle implements executor traits
- **WHEN** an `AgentHandle` is used where an `AgentExecutor` or `AgentStreamExecutor` is expected
- **THEN** it compiles and executes correctly, and `run_stream_with_state` no longer exists

### Requirement: RunConfig simplification
`RunConfig` SHALL contain only runtime parameters: `session_id`, `user_id`, `cancel_token`, `max_iterations`, etc. It SHALL NOT include `tool_registry`, `hook_registry`, or `session_store`, which SHALL be obtained from `AgentHandle`.

#### Scenario: run config excludes handle fields
- **WHEN** a `RunConfig` is constructed
- **THEN** it does not contain `tool_registry`, `hook_registry`, or `session_store`; those are accessed via `AgentHandle`
