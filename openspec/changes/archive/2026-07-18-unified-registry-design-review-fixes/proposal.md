## Why

统一注册表架构设计文档经过两轮多专家对抗性审查（已有审查 121 条发现 + 独立审查 33 条新发现），裁定后确认 0 条 Blocking 但 16 条 High 级别有效发现未在设计文档中落地。核心问题集中在：Rust 类型系统约束未完整处理（TypeId 注册路径、HookPayload 未定义、McpTransport 矛盾）、并发安全性缺失（PluginRegistration 事务、ToolIdentity 共享语义、KeyedMutex guard 生命周期）、安全修复未落地（DeniedWithFeedback 隔离标签）、以及 API 命名冲突（OutputBound 同名不同义）。这些缺陷会导致后续 Phase 实施时编译失败或语义错误，必须在 writing-plans 之前修正。

## What Changes

**OutputBound 命名冲突**
- From: §5.2 工具截断策略 `OutputBound` 与 §10.5 server 连接输出 `OutputBound` 同名
- To: §10.5 重命名为 `StreamOutputResource`
- Reason: 同一 crate 内同名类型编译失败
- Impact: non-breaking

**McpTransport enum vs trait 矛盾**
- From: §5.2 用 `Arc<dyn McpTransport>`，§10.6 用 `enum McpTransport`
- To: §10.6 保留 `enum McpTransportConfig`，§5.2 改为 `Arc<dyn McpConnection>`
- Reason: enum 不能作为 trait object，配置与连接是不同抽象层级
- Impact: non-breaking

**DeniedWithFeedback 安全隔离标签**
- From: `message.clone()` 直接写入 `ContentPart::Text`，无角色隔离
- To: 消息包裹在 `<user_denial_feedback>...</user_denial_feedback>` 标签中
- Reason: prompt injection 向量（用户拒绝反馈可能包含对抗性指令）
- Impact: non-breaking（安全增强）

**ToolIdentity 值类型语义**
- From: `Materialization` 持有 `Arc<ToolIdentity>`，与 registry 共享 identity
- To: `ToolIdentity` 为值类型（`Clone`），snapshot 捕获时 clone 当前值
- Reason: Arc 共享导致 stale detection 永远不触发
- Impact: non-breaking（正确性修复）

**ServiceRegistry TypeId 注册路径约束**
- From: 仅声明 `Arc<dyn Any + Send + Sync>` + `downcast_ref`
- To: 添加 debug_assert 验证 TypeId 一致性 + 注册代码文档示例
- Reason: 类型不一致时 downcast 静默返回 None
- Impact: non-breaking

**PluginRegistration 两阶段提交**
- From: 声明 "all-or-nothing" 但无实现机制
- To: 补充两阶段提交细节（预备验证 → 按序提交 → 失败逆序回滚）
- Reason: 三独立注册表无跨表事务，partial commit 导致不一致
- Impact: non-breaking

**bound_output 执行模型**
- From: sync 签名但可能做文件 I/O，模型未明确
- To: 改为 `async fn`，文件 I/O 不阻塞 tokio worker
- Reason: 每次工具调用后执行，频率高
- Impact: non-breaking

**HookPayload 定义补充**
- From: 设计文档中未定义 `HookPayload`
- To: 定义为 owned struct，&mut 仅修改内部字段
- Reason: 跨 await 边界需要 owned 数据
- Impact: non-breaking

**EventBus ephemeral 快速路径**
- From: `publish` 签名暗示所有事件走 mpsc actor
- To: ephemeral 事件直接 broadcast，durable 事件走 actor
- Reason: LLM delta 高频事件不允许串行化瓶颈
- Impact: non-breaking

**StreamFn 与 EventBus 事件流向**
- From: LlmEvent 和 AgentEvent 流向关系未明确
- To: LlmEvent 不进入 EventBus，AgentEvent 进入
- Reason: 避免双重发布
- Impact: non-breaking

**LoopServices required vs optional**
- From: 所有缺失服务返回 `RequiredServiceMissing`
- To: 区分 required（硬失败）和 optional（no-op 默认），GoalService 为 optional
- Reason: 新增服务导致旧配置无法启动
- Impact: non-breaking

**SteeringService deliverAs 语义**
- From: `QueueMode` 无 `deliverAs`
- To: 添加 `DeliverAs { as_role: MessageRole }` variant
- Reason: pi-mono PendingMessageQueue 支持，用于系统注入
- Impact: non-breaking

## Capabilities

### New Capabilities
- `design-review-fixes`: 多专家对抗性审查裁定后的设计文档修正（类型系统、并发安全、安全隔离、API 一致性 4 维度 12 项修复）

### Modified Capabilities

## Impact

- **设计文档**：修正 §5.2, §6.2, §8.3, §8.4, §9.3, §10.2, §10.5, §10.6 共 8 个 section
- **依赖**：无新外部依赖
- **API**：设计阶段修正，无已发布 API breaking change
- **迁移**：不改变 §11.5 时间线评估
