## Context

`docs/superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md` (3183 行) 是 synthia 统一注册表架构的完整设计文档。该文档经过了已有审查 (372 行, 121 条发现) 和独立多专家对抗性审查 (10 Critical + 23 High 新发现) 的双重审查，经裁定后：

- 有效 Blocking 发现降为 0（B7 中 3/5 已在设计中，B9/B10 已被 §11.5 修复）
- 有效 High 发现约 16 条，其中 12 条需要修正设计文档

本变更仅修正设计文档，不涉及代码实施。修正确保后续 writing-plans 阶段基于正确、完整、无编译障碍的设计。

## Goals / Non-Goals

**Goals:**
- 修正设计文档中 12 处 High 级别缺陷（4 维度：类型系统、并发安全、安全隔离、API 一致性）
- 确保设计文档中的所有 Rust 类型定义可编译
- 确保并发语义（锁、事务、快照）的定义完整且无矛盾
- 确保安全修复（DeniedWithFeedback 隔离）在设计代码中落地
- 为后续 writing-plans 提供可靠的基础

**Non-Goals:**
- 不修改已有审查文档（保持审查历史）
- 不实施代码变更（仅修正设计文档）
- 不重新评估时间线（§11.5 已修正）
- 不添加 CodeMode 或 WASM 沙箱（明确 defer 的项目）

## Decisions

### D1：OutputBound 命名冲突解决
- **选择**：§10.5 重命名为 `StreamOutputResource`
- **理由**：§5.2 的 `OutputBound` 是核心工具截断概念，引用更广泛；§10.5 是 server 连接层的输出资源，语义不同
- **已考虑 alternative**：重命名 §5.2 → 拒绝，因为 §5.2 是 opencode 借鉴的通用概念名

### D2：McpTransport 统一为 enum + trait
- **选择**：§10.6 保留 `enum McpTransportConfig`（配置数据），§5.2 引入 `trait McpConnection`（运行时连接抽象）
- **理由**：配置是静态数据（enum 足矣），连接是动态行为（需要 trait object）。两层级分离符合 §3.3 的依赖规则
- **已考虑 alternative**：仅用 enum + exhaustive match → 拒绝，因为 MCP 传输协议可扩展（第三方 MCP 服务器可能用自定义协议）

### D3：DeniedWithFeedback 隔离标签方案
- **选择**：包裹在 `<user_denial_feedback>...</user_denial_feedback>` 标签中 + 剥离内部的角色标记
- **理由**：最小化 prompt injection 面积。标签让 LLM 知道这是用户拒绝反馈而非工具输出，同时剥离防止嵌套注入
- **已考虑 alternative**：完全截断用户反馈 → 拒绝，因为反馈对 LLM 自修正是必要的

### D4：ToolIdentity 值类型 vs Arc 共享
- **选择**：`ToolIdentity` 为 `#[derive(Clone)]` 值类型，包含 `name: String` + `generation: ToolGeneration(u64)`
- **理由**：snapshot 捕获时 clone identity 的当前值，后续 generation bump 不影响已捕获的 snapshot。这是 stale detection 工作的前提
- **已考虑 alternative**：`Arc<AtomicU64>` 内部可变 → 拒绝，因为 snapshot 和 registry 看到同一 generation 值，stale detection 不触发

### D5：ServiceRegistry TypeId 注册路径验证
- **选择**：注册时添加 `debug_assert!` 验证 `Any::type_id(&payload) == TypeId::of::<Arc<dyn SubTrait>>()`
- **理由**：编译期无法强制 TypeId 一致性（Rust 类型系统的限制），但 debug_assert 在测试中捕获不一致
- **已考虑 alternative**：类型状态模式（sealed builder）→ 过于复杂，收益不高

### D6：PluginRegistration 两阶段提交
- **选择**：预备阶段（验证不持锁）→ 提交阶段（按 Tool → Service → Hook → MCP 固定顺序获取锁并提交）→ 失败时逆序回滚
- **理由**：单进程内两阶段提交简单可靠。固定顺序避免死锁。逆序回滚保证已提交的注册表恢复一致
- **已考虑 alternative**：best-effort + 补偿日志 → 拒绝，因为 "all-or-nothing" 是用户期望

### D7：bound_output 改为 async
- **选择**：`async fn bound_output(...)` + 内部 `tokio::fs::write` 处理文件 I/O
- **理由**：工具截断后的 spill-to-disk 是文件 I/O，不能阻塞 tokio worker。async 签名使 I/O 自然融入 tokio 运行时
- **已考虑 alternative**：sync + `spawn_blocking` → 可行但 API 不一致（调用者需要理解何时在 async 上下文中使用）

### D8：HookPayload owned struct 定义
- **选择**：`HookPayload` 为 owned struct，包含 `session_id: SessionId`、`turn_id: TurnId`、`tool_name: Option<String>`、`metadata: serde_json::Value`、`mutable_data: Option<serde_json::Value>`
- **理由**：hook 执行跨 await 边界，引用类型无法保证生命周期。owned 数据 + `&mut` 仅修改 `mutable_data` 字段
- **已考虑 alternative**：`&mut dyn Any` 动态类型 → 类型不安全，且与 `Send` 约束冲突

### D9：EventBus ephemeral 快速路径
- **选择**：`publish<E>` 方法内检查 `E::SYNC`——如果 `None`（ephemeral），直接通过 `typed_pubsub` + `all` broadcast 发布；如果 `Some`（durable），通过 `publish_tx` actor 序列化
- **理由**：LLM delta 事件频率可达数百/秒，全局串行化瓶颈不可接受。ephemeral 事件不需要全局序列号
- **已考虑 alternative**：所有事件走 actor → 拒绝，性能不可接受

### D10：StreamFn 与 EventBus 事件流向分离
- **选择**：`LlmEvent` 不进入 EventBus。`StreamFn` 产出的 `LlmEvent` 直接推送给 Agent loop。`AgentEvent`（高层语义事件）进入 EventBus
- **理由**：`LlmEvent` 是低层流事件（delta/finish），频率极高，经 EventBus 广播会导致订阅者泛滥。`AgentEvent` 是高层语义事件（TurnStart/ToolCallComplete），频率可控
- **已考虑 alternative**：LlmEvent 也进 EventBus → 拒绝，订阅者无法承受 delta 频率

### D11：LoopServices required vs optional 服务区分
- **选择**：`LoopServices::bootstrap` 对 required 服务（Session, Permission, Hook, Provider）硬失败，对 optional 服务（Goal, Steering, AgentControl, Guardian, Context, Sandbox, Extension, ModelRouter, Memory, Skill, Command, Task, Telemetry）使用 no-op 默认实现
- **理由**：GoalService 是新增的，旧配置没有。其他 optional 服务在降级模式下可用空操作继续
- **已考虑 alternative**：全部 required → 拒绝，升级后无法启动

### D12：SteeringService 添加 DeliverAs variant
- **选择**：`QueueMode` 添加 `DeliverAs { as_role: MessageRole }` variant，允许系统以指定角色注入消息
- **理由**：pi-mono 的 PendingMessageQueue 支持此功能。用于 compact 触发的系统消息、subagent 结果注入等场景
- **已考虑 alternative**：单独的 `inject_message` 方法 → 拒绝，与 `enqueue` 语义重叠

## Risks / Trade-offs

[Risk] 修正设计文档时可能引入新的不一致 → Mitigation: 修正后全文搜索所有交叉引用，确保 §5-§10 的类型名和 API 签名一致

[Risk] async bound_output 可能与当前 truncate_output (sync) 的迁移路径不兼容 → Mitigation: Phase 1 的 BuiltinToolProvider 可先使用 sync truncate（在 spawn_blocking 中），Phase 6 再迁移到 async bound_output

[Trade-off] ephemeral 事件不经 actor 意味着没有全局序列号 → 接受理由：ephemeral 事件不需要跨类型排序，per-type 的 broadcast 已保证同类型内的顺序

[Trade-off] ToolIdentity clone 比 Arc clone 略贵（String clone vs Arc refcount increment）→ 接受理由：snapshot 仅在 session 创建时执行一次，非 hot path

## Migration Plan

N/A — 本变更仅修正设计文档，不涉及部署变更。修正后的设计文档是后续 8-phase 实施计划的基础。

## Open Questions

1. `McpConnection` trait 的 object-safety 需要确认——如果 `connect()` 返回 `Self`，则不是 object-safe。需要设计为 `async fn connect(&self) -> Result<McpConnectionHandle, McpError>` 返回非 Self 类型。

2. `HookPayload::mutable_data: Option<serde_json::Value>` 是否足够表达 hook 需要修改的所有数据？如果 hook 需要修改 `ToolOutput`（如 after_tool_call 修改输出），当前设计没有对应字段。
