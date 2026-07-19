<!--
Raw capture of multi-expert adversarial review + adjudication output.

本档原样捕捉两轮审查的结论：
1. 第一轮：6 专家并行审查设计文档，产出 10 Critical + 23 High 新发现
2. 第二轮：6 裁定官交叉质证每条发现，修正过度定级，产出最终行动清单

design.md 从本档萃取并重新整理为结构化设计文件。
-->

## 背景

设计文档 `docs/superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md` (3183 行)
经过了已有审查 `docs/superpowers/specs/2026-07-18-synthia-design-review.md` (372 行, 121 条发现)
和独立多专家对抗性审查（10 Critical + 23 High 新发现）的双重审查。

两轮审查后经裁定，有效 Blocking 发现降为 0 条，有效 High 发现约 16 条。

本变更采纳裁定报告的 P0 (4条) + P1 (8条) = 12 条行动项，对设计文档进行修正。

## 决议链

### Q1: 已有审查的 B1/B2/B3 (TypeId downcast) 是否真正 Blocking?

**裁定**: 不是。`Arc<dyn Trait>` 可以作为 `Any` payload 存储，`downcast_ref::<Arc<dyn Trait>>()` 在双层 Arc 模式下可工作。真正的问题是注册路径的 TypeId 一致性——需要文档约束 + debug_assert。

**决定**: High 级别，添加注册路径约束文档和 debug_assert 验证。

### Q2: 已有审查的 B7 (5个缺失生产模式) 是否 Blocking?

**裁定**: 不是。5 项中 3 项已在设计中（GoalService §6.3、RunCoordinator §7.4、DoomLoop §7.4+§8.3），1 项部分覆盖（PendingMessageQueue → SteeringService 有 overlap 但缺 deliverAs），1 项明确 defer（CodeMode）。

**决定**: 降级为 High。补充 PendingMessageQueue 的 deliverAs 语义到 SteeringService。

### Q3: HookPayload 是否导致借用冲突?

**裁定**: 不会。HookPayload 未在设计文档中定义是设计不完整，不是借用冲突。如果 HookPayload 是 owned struct，&mut 仅修改内部字段，与 loop 的其他 & 引用不冲突。

**决定**: Medium 级别，补充 HookPayload 定义为 owned struct。

### Q4: PluginRegistration 事务原子性是否可实现?

**裁定**: 单进程内可实现。通过两阶段提交（预备阶段验证但不提交，提交阶段按固定顺序获取锁并提交）或 best-effort + 补偿日志。

**决定**: High 级别，补充两阶段提交实现细节。

### Q5: bound_output 是否应该 async?

**裁定**: 取决于执行模型。如果内部使用 spawn_blocking 则 sync 签名可接受；如果直接在 tokio worker 上做文件 I/O 则必须 async。设计文档未明确。

**决定**: High 级别，明确 bound_output 的执行模型。

### Q6: DeniedWithFeedback 的安全隔离是否已落地?

**裁定**: 未落地。设计文档 §8.3 的代码直接将 message 写入 ContentPart::Text，没有 <user_denial_feedback> 标签。已有审查 Security H4 提出的修复在设计代码中未实现。

**决定**: High 级别，在 synthetic ToolResult 中添加角色标签隔离。

### Q7: ToolIdentity 的 Arc 共享是否与 stale detection 矛盾?

**裁定**: 是。如果 ToolIdentity 通过 Arc 共享，snapshot 和 registry 看到同一个 generation 值，stale detection 永远不触发。ToolIdentity 应该是值类型。

**决定**: High 级别，ToolIdentity 改为值类型，snapshot 捕获时 clone。

### Q8: EventBus publish 的 ephemeral 快速路径是否缺失?

**裁定**: 设计中已有 typed_pubsub (DashMap) 用于直接 broadcast，但 publish 方法签名暗示所有事件都走 mpsc actor。ephemeral 事件是否走快速路径未明确。

**决定**: High 级别，明确 ephemeral 事件不经 publish_tx actor。

### Q9: OutputBound 命名冲突如何解决?

**裁定**: §5.2 的 OutputBound (工具截断策略) 和 §10.5 的 OutputBound (server 连接输出资源) 同名不同义，编译器会拒绝。

**决定**: 重命名 §10.5 为 ConnectionOutputBound 或 StreamOutputResource。

### Q10: McpTransport enum vs trait 矛盾如何解决?

**裁定**: §5.2 用 Arc<dyn McpTransport>，§10.6 用 enum McpTransport。已有审查 H7 标记但设计未修改。

**决定**: 统一为 enum McpTransportConfig + trait McpConnection。

### Q11: LoopServices 是否区分 required vs optional 服务?

**裁定**: 未区分。GoalService 是新增的，旧配置没有它，如果 bootstrap 硬失败则升级后无法启动。

**决定**: High 级别，区分 required 和 optional 服务，optional 缺失时使用 no-op 默认实现。

### Q12: 扩展点数量是 43 还是 61?

**裁定**: 代码验证确认为 43 个 Handler 类型别名（10 个子模块中）。原始审查声称 61 是错误的。

**决定**: 维持 43 计数，不调整时间线。

## 设计取捨

1. **TypeId 注册路径约束 vs 编译期保证**: 选择文档约束 + debug_assert 而非复杂的类型级证明——编译期保证在 Rust 当前类型系统中无法实现（dyn Trait 的 TypeId 是运行时值）。

2. **两阶段提交 vs best-effort**: 选择两阶段提交——单进程内实现简单，且 PluginRegistration 的原子性是用户期望（"要么全部注册成功，要么全部回滚"）。

3. **ToolIdentity 值类型 vs Arc 共享**: 选择值类型——stale detection 的正确性比 Arc clone 的微弱开销更重要。

4. **ephemeral 快速路径 vs 统一 actor**: 选择快速路径——LLM delta 事件的高频率不允许串行化瓶颈。

5. **DeniedWithFeedback 隔离标签 vs 无标签**: 选择隔离标签——即使增加了少量 prompt 长度，防止 prompt injection 的安全收益远大于成本。

6. **HookPayload owned struct vs 引用**: 选择 owned struct——避免与 loop 状态的借用冲突，且 hook 执行是跨 await 边界的，引用生命周期无法