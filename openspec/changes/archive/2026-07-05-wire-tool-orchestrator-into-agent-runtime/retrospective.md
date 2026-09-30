# 回顾：wire-tool-orchestrator-into-agent-runtime

## 完成情况

- 所有 14 项任务已标记完成。
- 新增/修改文件：
  - `crates/synthia-agent/src/agent.rs`（核心实现 + 测试）
  - `crates/synthia-agent/src/tools/orchestrator.rs`（已存在的装配辅助函数）
  - `crates/synthia-agent/tests/e2e_resume_test.rs`（已存在，resume 路径间接验证）

## 做得好的地方

1. **双路径统一装配**：`run_stream` 静态入口通过自由函数 `auto_assemble_tool_orchestrator`，`resume` 通过实例方法 `assemble_default_orchestrator`，两者复用 `build_default_tool_orchestrator`，避免重复实现。
2. **默认 fail-closed**：未注入 `ApprovalService` 时使用 `HeadlessApprovalService`（deny-by-default），未注入 `SandboxManager` 时使用 `NoopSandboxManager`，与项目安全原则一致。
3. **测试覆盖充分**：新增参数化测试一次性覆盖 4 个危险工具的拒绝行为，避免为每个工具写重复用例。

## 遇到的问题

1. `cargo test` 多测试名参数限制：一次只能接受一个模式，改用 `"orchestrator"` 单模式过滤相关测试。
2. 最初 3.3 测试只覆盖 `bash`，后扩展为 `bash` / `write` / `apply_patch` / `multi_edit` 的参数化测试，确保所有危险工具都被拒绝。

## 经验教训

- 静态入口方法（如 `Agent::run_stream`）无法调用实例方法，必须显式提供自由函数作为装配入口，否则 CLI/示例代码会静默降级为无 orchestrator。
- 危险工具列表应集中维护，避免测试与生产代码中的列表分叉。

## 后续可改进项

- 考虑把 `["bash", "write", "apply_patch", "multi_edit"]` 提升为 `HeadlessApprovalService` 的常量或配置，便于后续扩展。
- 当 `otel` feature 稳定后，可在 orchestrator 生命周期事件中补充 span 属性。
