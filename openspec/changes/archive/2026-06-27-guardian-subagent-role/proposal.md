## Why

synthia 的 `GuardianReviewer` 当前内联调用 LLM（`router.route()` → `provider.complete()`），无独立 session、无上下文隔离，且在生产 agent loop 中完全未接线（zero call sites）。codex 的 Guardian 以 subagent 形式运行，拥有隔离上下文、锁定配置防递归、独立 prompt-cache key。本次变更将 Guardian 升级为 codex 式 subagent role，补全 `GuardianCoordinator` 缺失的 LLM 升级路径（50-79 risk 区间），并将 Guardian 接入生产 agent loop，使 P6「不信任 LLM」原则在工具执行前真正生效。

## What Changes

**Guardian 执行模型**
- From: `GuardianReviewer` 内联调用 LLM，共享调用方 `ModelRouter`，无独立 session，无上下文隔离，生产代码未接线
- To: Guardian 作为 subagent role 通过 `SubagentSessionFactory::run_child` spawn，拥有独立 session、隔离上下文、锁定配置（`guardian_enabled: false`、`max_iterations: 1`、空工具表）
- Reason: 独立上下文审查比内联调用更 robust（codex 验证）；隔离防递归（P6 不信任 LLM）
- Impact: 非破坏性 — `GuardianReviewer` 保留作为 subagent 内部 LLM 调用逻辑；新增 `GuardianSubagentReviewer` 包装层

**GuardianCoordinator hybrid 升级路径**
- From: `GuardianCoordinator::check` 仅走 `SimpleGuardian` fast-path + CircuitBreaker，50-79 risk 区间直接返回 `NeedUserConfirm`，不升级到 LLM review
- To: 50-79 risk 区间升级到 Guardian subagent review；subagent 失败/超时 fallback 到 `SimpleGuardian::NeedUserConfirm`（fail-closed）
- Reason: 补全 hybrid-layer spec 期望的 LLM 升级路径，减少不必要的用户打断
- Impact: 非破坏性 — 低风险（<50）和高风险（≥80）路径不变

**Guardian 接入 agent loop**
- From: `GuardianCoordinator`/`GuardianReviewer` 在 `synthia-agent`/`synthia-server` 生产代码中零调用点
- To: `GuardianCoordinator::check` 接入工具执行 permission gate（在 `PermissionChecker` 之后、工具执行之前）
- Reason: 使 Guardian 在生产中实际生效
- Impact: 工具执行路径新增 Guardian review 步骤；50-79 risk 工具调用会增加 ~90s 延迟（可配置）

**Guardian subagent 配置锁定**
- From: 无 Guardian subagent 配置
- To: Guardian 子 session 配置三层锁定：(1) `derive_subagent_permission` Deny-only 继承；(2) `guardian_enabled: false` + `max_iterations: 1`；(3) 空工具注册表
- Reason: 防止 Guardian spawn Guardian 递归；最小权限原则（P6）
- Impact: Guardian subagent 无法调用任何工具，仅输出文本 assessment

## Capabilities

### New Capabilities
- `guardian-subagent-role`: Guardian 以 subagent 形式运行，定义 spawn 生命周期、配置锁定（防递归）、决策回流、prompt-cache key 隔离

### Modified Capabilities
- `guardian-hybrid-layer`: 明确 50-79 risk 区间升级到 Guardian subagent review（当前 spec 仅说 "escalate to GuardianReviewer"，需明确为 subagent 模式 + fallback 语义）

## Impact

- **代码层面**：`synthia-guardian` 新增 `GuardianSubagentReviewer`（包装 `SubagentSessionFactory` + `GuardianReviewer`）；`GuardianCoordinator` 补全升级路径；`synthia-agent` 接线 Guardian permission gate
- **API 层面**：`GuardianCoordinator::check` 签名扩展（需接收 `SubagentSessionFactory` + `conversation` + `cancel_token`）；`GuardianConfig` 新增 `timeout`/`subagent_enabled` 字段
- **依赖层面**：`synthia-guardian` 新增对 `synthia-agent`（`SubagentSessionFactory` trait）的依赖
- **性能层面**：50-79 risk 工具调用增加 ~90s review 延迟（可配置 timeout）；低风险/高风险路径无影响
- **事件层面**：复用现有 `AgentEvent::GuardianConfirmationRequest`/`GuardianWarning`，不新增事件类型
