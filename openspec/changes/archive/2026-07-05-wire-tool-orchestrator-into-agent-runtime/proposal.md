## Why

Synthia 已经在 `synthia-tool-orchestrator` crate 中实现了一个具备审批、沙箱选择、重试、并发控制和调用级取消的生产级工具编排器，但 `Agent::run_stream` 和 `Agent::resume` 在组装 `AgentRunConfig` 时把 `approval_service`、`sandbox_manager`、`tool_orchestrator` 全部设为 `None`。这导致运行时永远回退到直接调用 `ToolRegistry`，编排器的能力被完全闲置。现在是处理这个问题的合适时机，因为 Task 6 已经完善了相关组件的接口，只差把线接起来。修复后，审批、沙箱、重试、取消等机制将真正生效，为后续接入真实沙箱和交互式审批打下基础。

## What Changes

**Agent 运行时工具执行路径**
- From: `Agent::run_stream`/`Agent::resume` 构造 `AgentRunConfig` 时 `approval_service: None`、`sandbox_manager: None`、`tool_orchestrator: None`，`StepToolExecute` 回退到 `ToolRegistry::run_with_context`。
- To: `Agent` 增加可选的 `approval_service` 和 `sandbox_manager` 字段，并提供 `with_approval_service()`/`with_sandbox_manager()` 方法；当 `tool_orchestrator` 未显式注入时，自动调用 `build_default_tool_orchestrator()` 装配默认编排器。
- Reason: 启用已经实现但未被使用的编排器能力。
- Impact: 非破坏性；未注入服务时默认使用 `HeadlessApprovalService` + `NoopSandboxManager`，保持 fail-closed；CLI/server 可显式注入自定义服务。

**向后兼容**
- 保留 `StepToolExecute` 的 registry 回退分支，显式传入 `tool_orchestrator: None` 时行为不变。

## Capabilities

### New Capabilities
- `agent-tool-orchestrator-wiring`: 将 `DefaultToolOrchestrator` 及其审批、沙箱服务默认接入 `Agent::run_stream` 与 `Agent::resume`，使工具调用经过统一编排层而非直接走 `ToolRegistry`。

### Modified Capabilities
- 无现有 spec 的需求变更；本次为纯运行时装配层改动。

## Impact

- `crates/synthia-agent/src/agent.rs`：新增字段与默认装配逻辑。
- `crates/synthia-agent/src/config/agent_config/run_config.rs` 及 `run_config_builder.rs`：可能无需修改，因为已有可选字段。
- `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs`：行为不变，但默认路径从 registry 切换到 orchestrator。
- CLI/server 入口：可选择性注入真实 `ApprovalService`/`SandboxManager`，不强制。
