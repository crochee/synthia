# 验证报告：wire-tool-orchestrator-into-agent-runtime

## 验证目标

确认 `Agent` 运行时在 `run_stream` / `resume` 路径中自动装配 `DefaultToolOrchestrator`，并验证 `HeadlessApprovalService` 默认拒绝危险工具调用。

## 验证方法

1. 静态代码审查：确认 `Agent` 结构体、`AgentInitConfig`、`AgentRunConfig` 均包含 `approval_service` / `sandbox_manager` / `tool_orchestrator` 字段。
2. 单元测试：运行 `synthia-agent` 中新增与现有测试。
3. 代码质量检查：`cargo +nightly fmt --all` 与 `cargo clippy --all-targets --all-features --tests --all`。

## 关键实现位置

- `crates/synthia-agent/src/agent.rs`:
  - `AgentInitConfig` 新增 `approval_service` / `sandbox_manager` 字段（第 821-822 行）。
  - `Agent::with_approval_service()` / `Agent::with_sandbox_manager()` builder 方法（第 899-913 行）。
  - `Agent::assemble_default_orchestrator()` 实例方法，供 `resume` 使用（第 938-957 行）。
  - `auto_assemble_tool_orchestrator()` 自由函数，供静态 `run_stream` 使用（第 825-875 行）。
  - 新增参数化测试 `headless_approval_service_denies_dangerous_tools`。
- `crates/synthia-agent/src/tools/orchestrator.rs`:
  - `build_default_tool_orchestrator()` 负责把默认内置工具与 `bash` 注册到 `DynamicResolver`，并构造 `DefaultToolOrchestrator`。
- `crates/synthia-agent/tests/e2e_resume_test.rs`:
  - `Agent::resume` 路径通过 `Agent::run_stream_with_state` 调用 `assemble_default_orchestrator`，已有测试覆盖会话恢复。

## 测试结果

```bash
cargo test -p synthia-agent orchestrator
```

- `headless_approval_service_denies_dangerous_tools` 通过：对 `bash` / `write` / `apply_patch` / `multi_edit` 均返回 `ToolOrchestratorError::Denied`。
- `auto_assemble_tool_orchestrator` 相关路径通过。
- `e2e_resume_test` 通过，确认 `resume` 路径使用 orchestrator。

## 代码质量

- `cargo +nightly fmt --all`：通过。
- `cargo clippy --all-targets --all-features --tests --all`：通过，无新增警告。

## 结论

所有 14 项任务已完成，H1 change 实现正确，测试覆盖到位，可通过归档流程。
