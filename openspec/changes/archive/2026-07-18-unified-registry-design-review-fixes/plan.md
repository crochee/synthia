# Design Review Fixes Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** 修正统一注册表架构设计文档中 12 处 High 级别缺陷，确保后续 writing-plans 基于可编译、无矛盾的设计。

**Architecture:** 修改单一设计文档 `docs/superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md` 的 8 个 section (§5.2, §6.2, §8.3, §8.4, §9.3, §10.2, §10.5, §10.6)，涵盖类型系统、并发安全、安全隔离、API 一致性 4 个维度。

**Tech Stack:** Markdown 文档编辑，Rust 伪代码验证

---

## Task 1: API 一致性修正（编译级问题）

- [ ] **Step 1:** 搜索设计文档中所有 `OutputBound` 出现位置，区分 §5.2（工具截断）和 §10.5（server 输出）的引用
- [ ] **Step 2:** 修正 §10.5 的 `OutputBound` struct 定义为 `StreamOutputResource`，更新字段注释
- [ ] **Step 3:** 修正 §10.7 Key Decisions #6 中对 `OutputBound` 的引用
- [ ] **Step 4:** 修正 §5.2 `McpToolProvider` 中的 `Arc<dyn McpTransport>` 为 `Arc<dyn McpConnection>`，添加 `McpConnection` trait 定义
- [ ] **Step 5:** 修正 §10.6 `enum McpTransport` 为 `enum McpTransportConfig`，更新变体注释
- [ ] **Step 6:** 全文搜索确认无遗留的 `OutputBound`（§10.5 语境）和 `McpTransport`（无 Config 后缀）引用
- [ ] **Commit:** `docs: fix OutputBound naming conflict and McpTransport enum/trait split`

---

## Task 2: 安全修复落地

- [ ] **Step 1:** 定位 §8.3 `on_approval_denied_with_feedback` 函数代码块
- [ ] **Step 2:** 添加标签剥离辅助函数 `fn strip_denial_tags(s: &str) -> String`：移除所有 `<user_denial_feedback>` 和 `</user_denial_feedback>` 标签
- [ ] **Step 3:** 修正 `ContentPart::Text { text: message.clone() }` 为 `ContentPart::Text { text: format!("<user_denial_feedback>{}</user_denial_feedback>", strip_denial_tags(&message)) }`
- [ ] **Step 4:** 更新 §8.3 Key Decisions 第 2 条说明隔离标签机制
- [ ] **Commit:** `docs: add role isolation tags to DeniedWithFeedback`

---

## Task 3: 并发安全语义修正

- [ ] **Step 1:** 修正 §5.2 `ToolEntry` 中的 `identity: Arc<ToolIdentity>` 为 `identity: ToolIdentity`
- [ ] **Step 2:** 修正 §5.2 `Materialization` 中的 `HashMap<String, (Arc<dyn Tool>, Arc<ToolIdentity>)>` 为 `HashMap<String, (Arc<dyn Tool>, ToolIdentity)>`
- [ ] **Step 3:** 添加 `ToolIdentity` 值类型定义：`#[derive(Clone, Debug, PartialEq, Eq)] pub struct ToolIdentity { pub name: String, pub generation: ToolGeneration }` 和 `#[derive(Copy, Clone, ...)] pub struct ToolGeneration(pub u64)`
- [ ] **Step 4:** 添加 stale detection 机制说明：`resolve` 比较 `snapshot.identity.generation` 与 `entry.identity.generation`
- [ ] **Step 5:** 修正 §9.3 `with_plugin_lock` 签名为 `async fn with_plugin_lock<F, R>(&self, id: &PluginId, f: impl FnOnce(OwnedMutexGuard<()>) -> F) -> R where F: Future<Output = R>`
- [ ] **Step 6:** 补充 §9.3 `PluginRegistration` 两阶段提交实现说明：预备阶段（validate 无锁）→ 提交阶段（按 Tool→Service→Hook→MCP 固定顺序获取锁并提交）→ 失败逆序回滚
- [ ] **Commit:** `docs: fix ToolIdentity value-type semantics and PluginRegistration 2PC`

---

## Task 4: 类型系统约束补充

- [ ] **Step 1:** 在 §6.2 `ServiceRegistry::register_provider` 方法中添加 TypeId 验证注释和 debug_assert 代码：
  ```rust
  #[cfg(debug_assertions)]
  debug_assert_eq!(
      entry.service.type_id(),
      std::any::TypeId::of::<Arc<dyn SessionService>>(),
      "TypeId mismatch for service '{}': the Any payload type must be exactly Arc<dyn SubTrait>",
      entry.descriptor.name,
  );
  ```
- [ ] **Step 2:** 添加注册代码示例：展示 `ServiceProvider` 如何以精确的 `Arc<dyn SubTrait>` 类型构造 `Any` payload
- [ ] **Step 3:** 补充 §8.4 `HookPayload` 定义为 owned struct，列出字段和可变性约束
- [ ] **Commit:** `docs: add TypeId registration validation and HookPayload definition`

---

## Task 5: 执行模型与事件流修正

- [ ] **Step 1:** 修正 §5.2 `bound_output` 签名为 `pub async fn bound_output(&self, output: ToolOutput, session_id: &SessionId, call_id: &str) -> (ToolOutput, Vec<ManagedPath>)`
- [ ] **Step 2:** 修正 §10.2 `EventBus::publish` 方法添加 ephemeral/durable 分支：当 `E::SYNC == None` 时直接 broadcast，当 `E::SYNC == Some` 时走 actor
- [ ] **Step 3:** 添加 §10.7 Key Decision 声明 LlmEvent 不进入 EventBus
- [ ] **Step 4:** 修正 §7.4 `LoopServices::bootstrap` 区分 required (Session, Permission, Hook, Provider) 和 optional (其余) 服务
- [ ] **Step 5:** 添加 optional 服务的 no-op 默认实现说明
- [ ] **Commit:** `docs: fix bound_output async, EventBus ephemeral path, and LoopServices optional services`

---

## Task 6: 功能增强与遗漏补充

- [ ] **Step 1:** 在 §7.4 `QueueMode` enum 中添加 `DeliverAs { as_role: MessageRole }` variant
- [ ] **Step 2:** 在 §5.2 `ToolDescriptor` 中添加 `is_hidden: bool` 和 `is_user_invocable: bool` 字段（当前 Tool trait 有但设计中遗漏）
- [ ] **Commit:** `docs: add DeliverAs queue mode and missing ToolDescriptor fields`

---

## Task 7: 一致性验证

- [ ] **Step 1:** 全文搜索 `OutputBound` 确认仅 §5.2 使用
- [ ] **Step 2:** 全文搜索 `McpTransport` 确认 §5.2 用 `McpConnection`、§10.6 用 `McpTransportConfig`
- [ ] **Step 3:** 对照 12 条 spec requirement 逐项验证设计文档中有对应修正
- [ ] **Step 4:** 最终通读设计文档验证无新增矛盾
- [ ] **Commit:** `docs: verify all review fixes applied consistently`
