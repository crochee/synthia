## Context

Synthia 的 agent runtime 已经具备一个生产级工具编排器 `DefaultToolOrchestrator`（`crates/synthia-tool-orchestrator`），以及对应的工厂函数 `build_default_tool_orchestrator()`（`crates/synthia-agent/src/tools/orchestrator.rs`）。该编排器负责：

- 工具解析与查找
- 审批请求（`ApprovalService`）
- 沙箱选择（`SandboxManager`）
- 重试与并发控制
- 调用级取消
- 生命周期事件广播

然而，在 `Agent::run_stream` 与 `Agent::resume` 构建 `AgentRunConfig` 时，这三个字段被显式设为 `None`：

- `approval_service: None`
- `sandbox_manager: None`
- `tool_orchestrator: None`

因此 `StepToolExecute` 永远回退到 `execute_via_registry`，新的编排器、审批服务和沙箱管理器完全没被启用。这是一个“能力已写好但未接线”的生产级缺陷。

约束：
- 必须保持 `AgentRunConfig` 现有 API 兼容，避免破坏 CLI/server/子代理/测试。
- 必须遵守项目 memory 中的 fail-closed 原则：权限策略默认 ask/deny，不能默认 allow。
- 必须保持 Rust 编码规范：新产生的无用代码要删除，不能加 `dead_code`/`unused` 标签。

## Goals / Non-Goals

**Goals:**
1. 让 `Agent::run_stream` 和 `Agent::resume` 默认启用 `DefaultToolOrchestrator`。
2. 让 CLI/server 能够注入自定义 `ApprovalService` 和 `SandboxManager`。
3. 保持现有 `ToolRegistry` 回退路径可用（当显式注入 `tool_orchestrator: None` 时仍可通过 registry 执行）。
4. 通过 `cargo fmt` + `cargo clippy --all-targets --all-features --tests --all` 检查。

**Non-Goals:**
1. 不修改 `DefaultToolOrchestrator` 内部的审批超时、沙箱策略、重试策略。
2. 不实现真实沙箱 backend（bubblewrap/landlock/seatbelt）。
3. 不把 `StepToolExecute` 的默认权限启发式改为读取 `PermissionChecker`/`MergedPolicy`。
4. 不改动 `main_loop.rs` 的解构忽略（已有注释说明 orchestrator 由 `StepToolExecute` 消费）。

## Decisions

### D1：脱节点定位
- **选择**：根因是 `Agent::run_stream` 和 `Agent::resume` 未把审批/沙箱/编排器装配进 `AgentRunConfig`。
- **理由**：`main_loop.rs` 的解构忽略只是表象；`StepToolExecute::new(config)` 已经能消费 `tool_orchestrator`，但上游没传值。
- **已考虑 alternative**：在 `main_loop.rs` 里直接构造 orchestrator。被拒绝，因为这会把工具Resolver/审批/沙箱的构造责任下沉到 stream 层，破坏分层。

### D2：装配方案
- **选择**：在 `Agent` 结构体上增加可选的 `approval_service` 和 `sandbox_manager`，并在 `run_stream`/`resume` 中自动装配默认 orchestrator。
- **理由**：
  - 改动面最小，不破坏 `AgentRunConfigBuilder` API。
  - CLI/server 可以通过 `Agent::with_approval_service()` / `Agent::with_sandbox_manager()` 覆盖。
  - 子代理继承父 `AgentRunConfig` 时会自然携带 orchestrator，无需修改 `subagent/config.rs`。
- **已考虑 alternative B（Builder 强制装配）**：一致性好但会破坏所有调用点，改动过大。被拒绝。
- **已考虑 alternative C（启动代码显式注入）**：需要同步修改 run_stream/resume/子代理/恢复多条路径，容易遗漏。被拒绝。

### D3：默认服务选择
- **选择**：当调用者未注入任何服务时，默认使用：
  - `HeadlessApprovalService`
  - `NoopSandboxManager`
- **理由**：
  - `HeadlessApprovalService` 默认拒绝，符合 fail-closed 原则（项目 memory 要求默认 AskUser）。
  - `NoopSandboxManager` 保持当前“无沙箱”行为，不引入破坏性变更。
- **已考虑 alternative**：默认 allow 所有工具。被拒绝，违反安全原则。

### D4：是否保留 `ToolRegistry` 回退
- **选择**：保留。当 `AgentRunConfig` 中 `tool_orchestrator` 显式为 `Some` 时走 orchestrator；为 `None` 时继续走 `execute_via_registry`。
- **理由**：
  - 向后兼容测试和旧入口。
  - `StepToolExecute` 已经实现该分支，无需删除。
- **已考虑 alternative**：删除 registry 回退，强制 orchestrator。被拒绝，因为这会扩大变更范围并可能破坏现有测试。

## Risks / Trade-offs

- **[Risk] `HeadlessApprovalService` 默认拒绝，可能导致 CLI/headless 模式无法执行 `bash`/`write`/`apply_patch` 等工具。**
  → Mitigation：在实现 tasks 中增加测试覆盖，并在后续 change 为 CLI 注入一个基于 TUI/HTTP 的 `ApprovalService`。

- **[Risk] `NoopSandboxManager` 不提供真实隔离，恶意/误用工具仍可破坏用户环境。**
  → Mitigation：本次 change 明确 out-of-scope；后续 change 接入 `bubblewrap`/`landlock`/`seatbelt` backend。

- **[Trade-off] 默认装配在 `Agent` 层而非启动代码层。**
  → 接受理由：在保持 API 兼容的前提下立即启用编排器；server/CLI 仍可通过构造方法覆盖，不损失灵活性。

- **[Risk] 新增字段导致 `AgentInitConfig` 构造点需要更新。**
  → Mitigation：新增字段为 `Option`，并在 `Agent::new` 中默认设为 `None`，现有构造点无需修改。

## Migration Plan

本 change 不涉及部署变更，不需要数据库迁移或 endpoint 变更。

实施顺序：
1. 修改 `crates/synthia-agent/src/agent.rs`：增加字段、构造方法、默认装配逻辑。
2. 修改 `crates/synthia-agent/src/config/agent_config/run_config.rs` 和 `run_config_builder.rs`（若需要）：确保新字段可传递。
3. 更新/新增测试：验证 `Agent::run_stream` 在默认情况下使用 orchestrator；验证显式注入仍生效。
4. 运行 `cargo +nightly fmt --all` 和 `cargo clippy --all-targets --all-features --tests --all`。
5. 运行 `cargo test` 确认无回归。

Rollback：
- 若发现 regression，可将 `Agent::run_stream` 中的默认装配逻辑临时关闭（例如要求显式 `should_assemble_orchestrator` 开关），或回滚 git commit。

## Open Questions

1. CLI 入口是否应注入一个 `TuiApprovalService`？本次 change 先使用 `HeadlessApprovalService`，CLI 行为变更由后续 change 处理。
2. `HttpApprovalService`（Task 6 中提到的能力）是否已存在于 master？当前 master 未找到该实现，是否应纳入本次或后续 change？
3. 默认装配是否应在 `AgentRunConfigBuilder::build()` 中完成，而不是 `Agent::run_stream`？目前按方案 A 在 Agent 层完成，后续若发现不足可再迁移。
