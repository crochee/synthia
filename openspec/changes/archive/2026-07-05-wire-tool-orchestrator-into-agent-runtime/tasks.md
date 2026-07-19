## 1. Agent 结构体与构造方法

- [x] 1.1 在 `AgentInitConfig` 中增加 `approval_service: Option<Arc<dyn ApprovalService>>` 和 `sandbox_manager: Option<Arc<dyn SandboxManager>>`。
- [x] 1.2 在 `Agent` 结构体中增加对应字段，并在 `Agent::new` 中默认设为 `None`。
- [x] 1.3 实现 `Agent::with_approval_service()` 和 `Agent::with_sandbox_manager()` builder 方法。

## 2. 默认装配逻辑

- [x] 2.1 在 `Agent::run_stream` 中，若 `AgentRunConfig.tool_orchestrator` 为 `None`，则使用已注入或默认的 approval/sandbox 服务调用 `build_default_tool_orchestrator()` 生成 orchestrator，并写入 `AgentRunConfig`。
- [x] 2.2 在 `Agent::resume` 中复用同一装配逻辑（抽取为 `assemble_default_orchestrator()` 私有辅助函数）。
- [x] 2.3 默认服务选择：未注入 `ApprovalService` 时使用 `HeadlessApprovalService`；未注入 `SandboxManager` 时使用 `NoopSandboxManager`。

## 3. 测试

- [x] 3.1 新增单元测试：验证 `Agent::run_stream` 在无显式 orchestrator 时自动生成 `DefaultToolOrchestrator`。
- [x] 3.2 新增单元测试：验证显式注入的 `tool_orchestrator` 不会被默认装配覆盖。
- [x] 3.3 新增单元测试：验证 `HeadlessApprovalService` 默认拒绝 `bash`/`write`/`apply_patch`/`multi_edit` 工具调用。
- [x] 3.4 更新现有测试：确保 `Agent::resume` 路径同样使用 orchestrator。

## 4. 验证与清理

- [x] 4.1 运行 `cargo +nightly fmt --all`。
- [x] 4.2 运行 `cargo clippy --all-targets --all-features --tests --all` 并修复所有警告。
- [x] 4.3 运行 `cargo test` 并确保无回归。
- [x] 4.4 检查并删除本次变更引入的未使用 import/字段/变量。
