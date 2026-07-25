# Proposal: synthia-end-to-end-wiring

> Change #5 — 端到端连线：将已实现的 Registry-First 组件接入 Server→Agent→SSE 运行路径，使 AI Agent 可通过 HTTP SSE 端到端运行

## Why

Change #4 (Registry-First) 实现了全部核心组件（ToolName, RegistrationScope, FragmentRegistry, 5 个 Interceptor, ExtensionRegistry, SkillRegistry, PluginRegistry, RolloutTracker），但这些组件 **全部未连线**：

### 关键缺口

1. **ExtensionRegistry 未接入**: `AgentRunConfig` 中 `extension_registry: None` 出现 4 处（agent_factory, controller, resume, subagent/config），所有新 Registry 被跳过
2. **FragmentRegistry 未调用**: `fragment_delegation::build_system_prompt_from_fragments()` 已实现但 main_loop 不调用，system prompt 仍走旧 `ContextAssembler`
3. **InterceptorChain 未调度**: main_loop 工具执行仍走 `ToolOrchestrator` 直路径，不走 `interceptor_chain.dispatch(BeforeTool → execute → AfterTool)`
4. **Server 初始化不完整**: `AppState` 不构建 FragmentRegistry / InterceptorChain / SkillRegistry，导致 agent 运行时这些全部为 None
5. **Crate 冗余**: session/session-v2 双重存在、extension-v2 与 core 中 extension_registry 重叠、event-v2 功能单一、service 与 ExtensionRegistry 职责重叠

### 影响

当前状态下，`synthia-server` 启动后 `POST /api/v2/sessions/{id}/prompts` 可以创建会话并启动 `Agent::run_stream`，但：
- system prompt 缺少 skill/permission/rollout 片段
- 工具执行没有 interceptor 守卫（无权限检查、无循环检测、无重试）
- 文件变更追踪（RolloutTracker）未激活
- 技能系统未激活

**Agent 能跑，但不是生产级。**

## What Changes

### Phase 1: 连线已实现组件 (P0 — 最小可运行)

| # | 变更 | 文件 |
|---|------|------|
| 1 | AppState 构建 FragmentRegistry + 注册内建 Fragments | synthia-server/src/state/app_state.rs |
| 2 | AppState 构建 InterceptorChain + 注册 5 个 Interceptor | synthia-server/src/state/app_state.rs |
| 3 | AppState 构建 ExtensionRegistry（组合 5 个子 Registry） | synthia-server/src/state/app_state.rs |
| 4 | AgentFactory 传入 extension_registry (Some) | synthia-server/src/state/agent_factory.rs |
| 5 | Controller 传入 extension_registry + rollout_tracker (Some) | synthia-server/src/session/controller.rs |
| 6 | main_loop 使用 FragmentRegistry 构建 system prompt | synthia-agent/src/stream_builder/builder/run/main_loop.rs |
| 7 | main_loop 工具执行走 InterceptorChain | synthia-agent/src/stream_builder/builder/run/main_loop.rs |
| 8 | Resume / Subagent config 传入 extension_registry | synthia-agent/src/resume.rs, subagent/config.rs |

### Phase 2: Crate 精炼整合 (P1)

| # | 变更 | 说明 |
|---|------|------|
| 9 | session-v2 并入 session | session 内部已依赖 session-v2，合并消除间接层 |
| 10 | event-v2 并入 synthia-core | EventBus 功能单一，2 个文件即可 |
| 11 | extension-v2 评估保留或合并 | 与 core/extension_registry 职责重叠，需确定去留 |
| 12 | message-proxy 并入 synthia-server | 仅为 server 提供代理功能 |
| 13 | synthia-service 评估去留 | ServiceRegistry 与 ExtensionRegistry 概念重叠 |

### Phase 3: 修复 + 验证 (P0)

| # | 变更 |
|---|------|
| 14 | 修复 `l1_truncate_emits_recovery_applied_for_oversized_tool_output` 测试 |
| 15 | 端到端集成测试：HTTP POST prompt → SSE 事件流 → Agent 完成 |
| 16 | 更新 registry-first tasks.md 勾选已完成项 |

## Capabilities

### New Capabilities

| Capability | Description |
|------------|-------------|
| `extension-registry-wiring` | Server AppState 构建完整 ExtensionRegistry 并传入 AgentRunConfig，取代所有 `None` 占位 |
| `fragment-registry-active` | main_loop 通过 FragmentRegistry 构建 system prompt，内建 Fragments（SystemPrompt, Skills, Permissions, RolloutBudget 等）全部激活 |
| `interceptor-chain-active` | main_loop 工具执行走 InterceptorChain 调度，5 个 Interceptor（Permission, LoopDetect, Approval, Retry, Compact）全部生效 |
| `rollout-tracker-active` | Server 注入 RolloutTracker，main_loop 在工具执行后调用 record_change()，LLM 响应后调用 record_token_usage() |
| `crate-consolidation` | 精简冗余 crate（session-v2, event-v2, message-proxy 等），减少维护负担和编译时间 |

### Modified Capabilities

| Capability | Description |
|------------|-------------|
| `unified-extension-registry` | 从"已实现未连线"升级为"已实现已连线" — 所有子 Registry 在 Server 启动时构建并通过 AgentRunConfig 传入 |

## Impact

- **Code**: synthia-server (AppState, AgentFactory, Controller), synthia-agent (main_loop, resume, subagent/config), crate 整合涉及 Cargo.toml 和模块迁移
- **API**: 无外部 API 变更；内部 AgentRunConfig 字段从 None 变为 Some
- **Dependencies**: crate 整合后减少 workspace members
- **Backward compatibility**: 完全兼容 — 所有变更都是内部连线，不改变外部接口
