## Context

synthia 已有 `synthia-guardian` crate（`SimpleGuardian` + `GuardianReviewer` + `GuardianCoordinator`），但存在两个关键缺陷：

1. **执行模型缺陷**：`GuardianReviewer` 内联调用 LLM（`router.route()` → `provider.complete()`），在调用方 task 中同步执行，无独立 session、无上下文隔离。codex 的 Guardian 以独立 subagent 形式运行，拥有隔离上下文、锁定配置、独立 prompt-cache key。
2. **接线缺失**：`GuardianReviewer`/`GuardianCoordinator` 在生产 agent loop 中零调用点。`GuardianCoordinator` 的 hybrid 升级路径（50-79 risk → LLM review）未实现。

synthia 已有可复用的 subagent 基础设施：`SubagentSessionFactory` trait（`synthia-agent/src/subagent/factory.rs`）、`AppStateSubagentFactory`（`synthia-server/src/state/subagent_factory.rs`）、`derive_subagent_permission`（Deny-only 继承，synthia 4 处领先之一）、`build_subagent_config`（过滤父消息历史）。

**约束**：
- P1 前缀一致性：Guardian subagent 不修改父 session prompt
- P6 不信任 LLM：Guardian subagent 配置必须锁定防递归
- P7 可中断性：`cancel_token` 贯穿 subagent 生命周期
- P10 文件即记忆：复用现有 subagent 框架，不造新抽象
- CLAUDE.md §2 Simplicity First：Phase 0 不做 trunk 复用、不做独立 thread

**参考实现**：codex `codex-rs/core/src/guardian/review_session.rs`（trunk+ephemeral 模式）、`codex-rs/ext/guardian/src/lib.rs`（`ThreadLifecycleContributor`）。

## Goals / Non-Goals

**Goals:**
- Guardian 以 subagent 形式运行，拥有独立 session 和隔离上下文
- Guardian subagent 配置三层锁定（permission Deny-only + `guardian_enabled: false` + 空工具表），防止递归
- `GuardianCoordinator` 补全 50-79 risk 区间的 LLM 升级路径
- Guardian 接入生产 agent loop（工具执行 permission gate）
- Guardian 决策回流复用现有 `AgentEvent` + `GuardianDecision` 类型
- 失败/超时渐进降级到 `SimpleGuardian`（P4 渐进降级）

**Non-Goals:**
- Trunk + Ephemeral 并发 review 模式（留作 P2，当 OTel 指标显示 cache miss >30% 时引入）
- 独立 OS thread + `oneshot::Receiver` 非阻塞 review（留作 P2）
- `ThreadLifecycleContributor` 抽象（synthia 无 extension 系统，过早抽象）
- Guardian 主动工具调用（read file 验证 patch 等）
- Guardian 跨 turn 状态累积（per-review 无状态，CircuitBreaker 已在父侧）
- 重写 `GuardianReviewer`（保留作为 subagent 内部的 LLM 调用逻辑）

## Decisions

### D1：复用 `SubagentSessionFactory`，不新建 Guardian 专用 spawn 路径
- **选择**：Guardian 作为普通 subagent role，通过 `SubagentSessionFactory::run_child` spawn
- **理由**：复用已有 `ChildSessionHandle`/`AppStateSubagentFactory`/事件镜像；与 `subagent-built-in-types` spec 一致；零新基础设施；P10 不造抽象
- **已考虑 alternative**：新建 Guardian 专用 spawn（仿 codex `run_codex_thread_interactive`）— 拒绝，因为 ~500+ 行新基础设施与现有 subagent 框架重复，且 trunk+ephemeral 是 codex 多并发优化，synthia 单用户场景过早引入

### D2：三层配置锁定防止递归
- **选择**：(1) `derive_subagent_permission` Deny-only 继承（已有）；(2) `guardian_enabled: false` + `max_iterations: 1`；(3) 空工具注册表
- **理由**：codex 用 `approval_policy = Never` + `read_only` + 无 MCP/skills/hooks；synthia 对应等价物是 Deny-only permission + 单轮迭代 + 空工具表。三层独立防护，任一层失效仍有兜底（P6 不信任 LLM）
- **已考虑 alternative**：仅靠 `guardian_enabled: false` 单层防护 — 拒绝，违反 P6「防护机制独立于 LLM」原则；单层防护若 config 注入失败则递归无限制

### D3：50-79 risk 区间升级到 subagent review，失败 fallback 到 SimpleGuardian
- **选择**：`GuardianCoordinator::check` 中，50 ≤ risk < 80 → spawn Guardian subagent；subagent 失败/超时 → fallback `SimpleGuardian::NeedUserConfirm`（fail-closed）
- **理由**：补全 hybrid-layer spec 期望的 LLM 升级路径；50-79 是「不确定风险」区间，LLM review 比规则匹配更准确；失败时 fallback 到 `NeedUserConfirm` 而非 `Deny`，因为规则引擎已判定非高风险（<80），用户确认是合理兜底（P4 渐进降级）
- **已考虑 alternative**：失败时直接 `Deny`（更保守）— 拒绝，因为 risk < 80 说明规则引擎不认为是高风险，强制 Deny 会导致过多 false positive 阻断

### D4：同步 async review（`.await` subagent），不做独立 thread
- **选择**：`GuardianCoordinator::check`（已是 async）直接 `.await` `run_child` 结果，用 `tokio::time::timeout` 包装
- **理由**：synthia 当前 CLI/IDE 场景单用户，review 期间父 session 阻塞可接受（不消耗 LLM token）；独立 thread 模式增加复杂度（oneshot channel + OS thread + tokio runtime），违反 CLAUDE.md §2 Simplicity First
- **已考虑 alternative**：codex 的 `spawn_approval_request_review`（独立 OS thread + `oneshot::Receiver`）— 拒绝，留作 P2 当 CLI/Server 需要非阻塞 UI 时

### D5：复用现有 `build_review_prompt` 作为 subagent user message
- **选择**：Guardian subagent 的 system prompt = guardian policy（独立）；user message = `build_review_prompt(transcript, action_json, None)`（已有）；期望输出 = JSON assessment
- **理由**：`build_review_prompt` 已实现 transcript 压缩 + action JSON + risk criteria；subagent 模式只改变 LLM 调用方式（独立 session vs 内联），不改变 prompt 语义
- **已考虑 alternative**：重新设计 prompt — 拒绝，无必要变更，违反 CLAUDE.md §3 Surgical Changes

### D6：不引入 `ThreadLifecycleContributor` 抽象
- **选择**：直接用 `SubagentSessionFactory::create_child(parent_session_id)` 的天然 fork 关系
- **理由**：codex 的 `ThreadLifecycleContributor` 是为第三方 extension 系统设计（记录 `forked_from_thread_id` 供后续 fork）；synthia 无 extension 系统，`parent_session_id` 已通过 `ChildSessionHandle` 传递；forked history 通过 `build_subagent_config` 过滤实现（已有）。引入此抽象违反 P10
- **已考虑 alternative**：引入 `ThreadLifecycleContributor` trait — 拒绝，过早抽象

### D7：复用现有 `AgentEvent` + `GuardianDecision`，不新增事件类型
- **选择**：父侧 emit `AgentEvent::GuardianConfirmationRequest`（review 开始）+ `AgentEvent::GuardianWarning`（review 结果）；Guardian subagent 的 `Finish { output }` 被父侧解析为 `GuardianDecision`
- **理由**：现有事件类型已覆盖 Guardian 决策语义；新增类型违反 CLAUDE.md §3 Surgical Changes；`GuardianConfirmationRequest`/`GuardianWarning` 已在 `AgentEvent` enum 中（durable/ephemeral 分类已定）
- **已考虑 alternative**：新增 `AgentEvent::GuardianAssessment`（仿 codex）— 拒绝，语义与 `GuardianWarning` 重叠

### D8：Guardian subagent prompt-cache key 用 `guardian:{parent_session_id}`
- **选择**：通过 `SystemContext Source`（P1-4 已完成）注入 `guardian:{parent_session_id}` 作为 Guardian subagent 的 prompt-cache key
- **理由**：跨 review 复用 cache（同父 session 的多次 review 共享前缀）；namespace 隔离防止跨 session 污染（满足 project memory 硬约束「cache hash 必须含 user_id 命名空间」— `parent_session_id` 已含 user_id namespace）
- **已考虑 alternative**：每次 review 用随机 cache key — 拒绝，cache miss 率 100%，浪费成本

## Risks / Trade-offs

- [Risk] Guardian subagent spawn 延迟（~1-2s session 创建 + ~90s LLM review）阻断父 session → Mitigation: `GuardianConfig::timeout` 可配置（默认 90s）；CLI/Server UI 显示 "Guardian reviewing..."；低风险（<50）和高风险（≥80）路径不 spawn subagent
- [Risk] Guardian subagent 配置注入失败导致递归（Guardian spawn Guardian） → Mitigation: 三层独立锁定（D2）；`guardian_enabled: false` 在 config 层；`max_iterations: 1` 在 runtime 层；空工具表在 registry 层
- [Risk] `SubagentSessionFactory::run_child` 的 300s 硬编码超时不匹配 Guardian 90s → Mitigation: `GuardianCoordinator::check` 用 `tokio::time::timeout(90s, run_child(...))` 包装，超时即 fallback
- [Risk] Guardian subagent 输出解析失败（非 JSON assessment） → Mitigation: `parse_assessment_response` 已有错误处理；解析失败 → fallback `SimpleGuardian::NeedUserConfirm`（P4 渐进降级）
- [Trade-off] Phase 0 不做 trunk 复用，每次 review spawn 新 subagent（cache miss） → 接受理由：Guardian review 频率低（仅 50-79 risk 区间）；OTel 指标监控 cache miss 率；>30% 时引入 trunk（P2）
- [Trade-off] 同步阻塞 review（父 session 等待） → 接受理由：review 期间父 session 不消耗 LLM token；`cancel_token` 可中断（P7）；非阻塞模式留作 P2
- [Trade-off] Guardian subagent 空工具表（无法主动 read file 验证 patch） → 接受理由：review prompt 已含 action JSON（完整命令/patch 内容）；codex 也是 `read_only` + 无 MCP

## Migration Plan

**部署顺序**：
1. 新增 `GuardianSubagentReviewer`（`synthia-guardian` 内，包装 `SubagentSessionFactory` + `GuardianReviewer`）
2. `GuardianCoordinator` 扩展 `check` 签名（接收 `SubagentSessionFactory` + `conversation` + `cancel_token`）
3. `GuardianCoordinator::check` 补全 50-79 risk 升级路径
4. `synthia-agent` 工具执行路径接入 `GuardianCoordinator::check`（permission gate）
5. `GuardianConfig` 新增 `timeout`/`subagent_enabled` 字段（`#[serde(default)]` 兼容旧配置）

**Rollback 策略**：
- `GuardianConfig::subagent_enabled: false`（默认）→ Guardian 走旧路径（`SimpleGuardian` + `NeedUserConfirm`），不 spawn subagent
- `GuardianCoordinator` 的 fallback 逻辑确保 subagent 失败时降级到 `SimpleGuardian`

**验收条件**：
- `cargo test -p synthia-guardian` 全部通过（含新增 subagent review 测试）
- `cargo test -p synthia-agent` 全部通过（含 Guardian permission gate 接线测试）
- 50-79 risk 工具调用触发 Guardian subagent review（集成测试验证）
- Guardian subagent 配置三层锁定验证（递归防护测试）
- Guardian subagent 超时/失败 fallback 到 `SimpleGuardian`（降级测试）

## Open Questions

- Guardian subagent 的 system prompt（guardian policy）应放在 `synthia-guardian` 还是配置文件？→ 倾向 `synthia-guardian` 常量（P10 文件即记忆，但 policy 是代码级常量非用户配置）
- `GuardianCoordinator::check` 接入工具执行路径的具体位置：在 `PermissionChecker::check` 之后、`ToolOrchestrator::execute` 之前？还是嵌入 `ToolOrchestrator` 内部？→ 倾向后者（`ToolOrchestrator` 内部 permission gate 之后），因为 `ToolOrchestrator` 是统一入口（`production-tool-execution-sandbox` change 定义）
