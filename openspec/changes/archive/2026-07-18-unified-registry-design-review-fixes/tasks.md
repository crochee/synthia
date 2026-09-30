## 1. API 一致性修正（编译级问题）

- [x] 1.1 修正 §10.5 `OutputBound` → `StreamOutputResource`：重命名 struct、更新所有交叉引用、更新 §10.7 Key Decisions #6 中的引用
- [x] 1.2 修正 §5.2 `Arc<dyn McpTransport>` → `Arc<dyn McpConnection>`：添加 `McpConnection` trait 定义到 §5.2，更新 `McpToolProvider` 字段
- [x] 1.3 修正 §10.6 `McpTransport` → `McpTransportConfig`：重命名 enum、更新所有交叉引用

## 2. 安全修复落地

- [x] 2.1 修正 §8.3 `DeniedWithFeedback` 代码：在 `on_approval_denied_with_feedback` 中添加 `<user_denial_feedback>` 标签包裹逻辑
- [x] 2.2 添加标签剥离逻辑：在包裹前先 strip 消息中已有的 `<user_denial_feedback>` / `</user_denial_feedback>` 标签防止嵌套注入
- [x] 2.3 更新 §8.3 Key Decisions 说明隔离标签机制

## 3. 并发安全语义修正

- [x] 3.1 修正 §5.2 `Materialization` 中的 `Arc<ToolIdentity>` → 值类型 `ToolIdentity`：更新 struct 定义和 snapshot 捕获逻辑说明
- [x] 3.2 添加 `ToolIdentity` 值类型定义到 §5.2（`#[derive(Clone, Debug, PartialEq, Eq)]` + `name: String` + `generation: ToolGeneration(u64)`）
- [x] 3.3 修正 §9.3 `with_plugin_lock` 签名：将 guard 生命周期与 future 绑定（`FnOnce(OwnedMutexGuard<()>) -> Fut`）
- [x] 3.4 补充 §9.3 `PluginRegistration` 两阶段提交实现细节：预备验证 → 按序提交 (Tool→Service→Hook→MCP) → 失败逆序回滚

## 4. 类型系统约束补充

- [x] 4.1 添加 §6.2 TypeId 注册路径验证：在 `ServiceProvider` 注册时添加 `debug_assert!` 验证 TypeId 一致性
- [x] 4.2 添加 §6.2 注册代码示例：展示如何以精确的 `Arc<dyn SubTrait>` 类型构造 `Any` payload
- [x] 4.3 补充 §8.4 `HookPayload` owned struct 定义：列出所有字段和可变性约束

## 5. 执行模型与事件流修正

- [x] 5.1 修正 §5.2 `bound_output` 签名为 `async fn`：更新方法签名和内部 I/O 模型说明
- [x] 5.2 修正 §10.2 `EventBus::publish` 区分 ephemeral/durable 路径：ephemeral 事件直接 broadcast，durable 事件走 actor
- [x] 5.3 添加 §10.3/§10.4 事件流向声明：LlmEvent 不进入 EventBus，AgentEvent 进入 EventBus
- [x] 5.4 补充 §7.4 `LoopServices` required vs optional 服务区分：列出 required 和 optional 服务清单及 no-op 默认行为

## 6. 功能增强

- [x] 6.1 添加 §7.4 `QueueMode::DeliverAs { as_role: MessageRole }` variant 到 `SteeringService` 定义
- [x] 6.2 添加 `is_hidden()` / `is_user_invocable()` 到 §5.2 `ToolDescriptor`（当前 Tool trait 有但设计中遗漏）

## 7. 一致性验证

- [x] 7.1 全文搜索设计文档中所有 `OutputBound` 引用确保无遗漏
- [x] 7.2 全文搜索 `McpTransport` 引用确保 §5.2 用 `McpConnection`、§10.6 用 `McpTransportConfig`
- [x] 7.3 验证所有 12 项 spec requirement 在设计文档