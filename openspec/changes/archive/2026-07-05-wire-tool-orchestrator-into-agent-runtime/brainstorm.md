<!--
Raw capture of brainstorming output.

本檔原樣捕捉 brainstorming 的產出，不強制結構。本次因可用技能列表中沒有
`superpowers:brainstorming`，改以手動整理已完成的討論。

design.md 從本檔萃取並重新整理為結構化設計文件。
-->

# Brainstorm: Wire Tool Orchestrator into Agent Runtime

## Background

Synthia 在 `crates/synthia-tool-orchestrator` 中已经实现了一个生产级工具编排器：
- 审批（`ApprovalService`）
- 沙箱选择（`SandboxManager`）
- 重试策略
- 并发控制
- 调用级取消
- 生命周期事件

同时 `crates/synthia-agent/src/tools/orchestrator.rs` 提供了 `build_default_tool_orchestrator()` 工厂函数，可以把内置工具 + bash 组装成可执行的 orchestrator。

然而，在 `Agent::run_stream` 和 `Agent::resume` 中构建 `AgentRunConfig` 时，这三个字段被显式设为 `None`：
- `approval_service: None`
- `sandbox_manager: None`
- `tool_orchestrator: None`

导致 `StepToolExecute` 永远回退到 `execute_via_registry`，新编排器完全没被启用。

## Decision Chain

### Q1: 真正的脱节点在哪里？

最初以为是 `main_loop.rs` 中 `tool_orchestrator: _`、`approval_service: _`、`sandbox_manager: _` 的解构忽略。但进一步阅读 `StepToolExecute::new(config)` 后发现，它从 `AgentRunConfig` 读取 `tool_orchestrator` 并直接使用。因此根因是 **上游 `Agent::run_stream`/`Agent::resume` 没有把这些服务装配进 `AgentRunConfig`**。

### Q2: 这次修复要不要顺带重构权限/沙箱策略？

不。当前 `DefaultToolOrchestrator` 内部的审批超时、沙箱策略、重试策略已经存在，只是没被启用。本次范围限定为 **把它们接起来**，策略调优留给后续 change。

### Q3: 选择哪种装配方案？

讨论了三种方案：

| 方案 | 描述 | 优点 | 缺点 |
|---|---|---|---|
| A. Agent 层默认装配 | `Agent` 增加可选 `approval_service`/`sandbox_manager`；`run_stream`/`resume` 在 `tool_orchestrator` 为 None 时自动调用 `build_default_tool_orchestrator()` 装配 | 改动最小；自动启用已有实现；CLI/server 仍可显式注入 | 默认沙箱为 noop，需要第二步强化 |
| B. Builder 强制装配 | `AgentRunConfigBuilder` 在 `build()` 时统一组装 orchestrator | 配置一致性最好 | 破坏现有 builder API，影响所有调用点 |
| C. 启动代码显式注入 | 在 `synthia-cli`/`synthia-server` 中显式构造 orchestrator 注入 | 职责最清晰 | 需要同步修改 run_stream/resume/子代理/恢复路径，容易遗漏 |

**决定采用方案 A**：在 `Agent` 层默认装配，但允许 CLI/server 覆盖。这样能在不破坏现有 API 的前提下立即启用编排器，同时为后续 server 注入 `HttpApprovalService` 和真实沙箱保留扩展点。

## Approved Design Sketch

1. 在 `AgentInitConfig` / `Agent` 中增加：
   - `approval_service: Option<Arc<dyn ApprovalService>>`
   - `sandbox_manager: Option<Arc<dyn SandboxManager>>`

2. 提供 `Agent::with_approval_service()` / `Agent::with_sandbox_manager()` 构造方法。

3. 在 `Agent::run_stream` 和 `Agent::resume` 中，若 `tool_orchestrator` 为 None 且 approval/sandbox 至少有一个被注入，则调用 `build_default_tool_orchestrator()` 生成默认 orchestrator 并写入 `AgentRunConfig`。

4. 若没有任何服务注入，默认使用：
   - `HeadlessApprovalService`
   - `NoopSandboxManager`

5. `StepToolExecute` 保持现状（它已经能使用 orchestrator），但后续 change 会将其默认权限启发式改为读取 `PermissionChecker`/`MergedPolicy`。

## Risks & Open Questions

- `HeadlessApprovalService` 默认拒绝，可能导致 headless/CLI 模式无法执行 `bash`/`write` 等需要确认的工具。需要文档说明如何注入自定义 `ApprovalService`。
- `NoopSandboxManager` 不提供真实隔离，后续必须接一个真实沙箱 backend（bubblewrap/landlock/seatbelt）。
- 子代理路径 `subagent/config.rs` 继承父配置，会自然携带 orchestrator，无需额外修改。
