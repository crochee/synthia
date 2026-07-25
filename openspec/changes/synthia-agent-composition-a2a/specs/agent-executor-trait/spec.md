## ADDED Requirements

### Requirement: AgentExecutor trait
```rust
trait AgentExecutor {
    async fn run(&self, prompt: &str, config: RunConfig) -> Result<AgentOutput>;
    async fn resume(&self, session: &mut AgentSession, prompt: &str) -> Result<AgentOutput>;
}
```
run() 创建新 Session 执行，resume() 基于已有 Session 继续执行。

### Requirement: AgentStreamExecutor trait
```rust
trait AgentStreamExecutor: AgentExecutor {
    async fn run_stream(&self, prompt: &str, config: RunConfig) -> Result<AgentOutputStream>;
    async fn resume_stream(&self, session: &mut AgentSession, prompt: &str) -> Result<AgentOutputStream>;
}
```

### Requirement: AgentHandle impl traits
AgentHandle 同时实现 AgentExecutor 和 AgentStreamExecutor。
run_stream_with_state 删除，统一为 resume_stream。

### Requirement: RunConfig simplification
RunConfig 只包含运行时参数：session_id, user_id, cancel_token, max_iterations 等。
不包含 tool_registry / hook_registry / session_store（从 AgentHandle 获取）。
