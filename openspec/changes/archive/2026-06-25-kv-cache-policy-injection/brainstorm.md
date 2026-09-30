<!--
Raw capture of brainstorming output (multi-expert adversarial discussion 2026-06-25).

本档原样捕捉 brainstorming skill 的产出，不强制结构。
本档来自本次会话的多专家对抗性讨论（架构师 + 性能工程师 + 安全工程师 +
可靠性工程师 + 沙箱专家 + 产品工程师 + 可观测性专家 + Agent 研究员 +
DevOps + synthia 维护者，14 轮 Sequential Thinking 对抗）。

design.md 从本档萃取并重新整理为结构化设计文档。
不要将本档的内容复制到 design.md — design.md 是独立的重组产物，
两者互补但不重疊。
-->

# KV Cache Policy Injection — 多专家对抗性讨论决策日志

## 背景

synthia 当前在 `synthia-context/src/system_context.rs` 用 `cache_breaker`
（每 5min TTL 过期后生成随机 `cb_xxxxxxxx`）注入 system prompt，破坏 P1
前缀一致性。同时 `synthia-provider` 已经发送
`anthropic-beta: prompt-caching-2024-07-31` header，但**不主动注入
`cache_control` hint** 到 tools/system/messages，导致 Anthropic 无法识别
可缓存的 prefix。

对比 opencode 的 `packages/llm/src/cache-policy.ts`（教科书级实现）：
- `applyCachePolicy` 在请求构建的最后一步主动注入 `cache_control` hint
- Provider 感知：只在 `anthropic-messages` 和 `bedrock-converse` route 注入
- `latest-user-message` 策略：只在最后一条用户消息打断点，最大化 cache 命中
- 引用相等检查：三个标记都没变 → 返回原对象，下游 diff 识别"无变化"

## 决策链

### Q1：cache_breaker 是"反模式"还是有用的"跨 session 隔离"机制？

**架构师视角**：cache_breaker 是反 P1 模式。每 5min TTL 过期 → 生成新
随机数 → system prompt 变化 → 所有后续调用 prefix 全废。

**性能工程师视角**（深入读 system_context.rs:67-72）：`generate_cache_breaker()`
每次 cache miss 都生成新随机数注入。SYSTEM_CONTEXT_TTL=5min 只是缓存有效期，
但只要 TTL 过期，新 breaker 就注入 system prompt。

**安全工程师视角**：可能原始设计是为了"防止用户切换 session 后命中旧 cache"
（跨 session 隔离）。如果是这个意图，memory constraint 里已有
`HMAC-SHA256(user_id ‖ session_id)[:32]` 做 `prompt_cache_key` 命名空间隔离，
cache_breaker 是冗余的。

**决议**：删除 cache_breaker，依赖 prompt_cache_key 命名空间隔离。这是 P0-1
（独立 change），不在本 change 范围。本 change 聚焦 P0-2 主动注入。

### Q2：applyCachePolicy 应该在 provider 层还是 context 层实现？

**架构师视角**：在 context 层（prompt builder 之后）实现，因为 context 层知道
system prompt 的完整结构。

**性能工程师视角**：在 provider 层实现，因为只有 provider 知道 route 是否支持
inline cache hints（opencode 的 `RESPECTS_INLINE_HINTS`）。

**DevOps 视角**：在 provider 层实现，因为 transform_request 是 Anthropic body
构建的最后一步，可以在这里注入 `cache_control` 到 JSON 结构。

**决议**：在 `synthia-provider` crate 新建 `cache_policy.rs` 模块，导出
`apply_cache_policy(&mut CompletionRequest, &CachePolicy)` 函数。在
`AnthropicProvider::transform_request` 之前调用。理由：
1. Provider 感知只在 anthropic 实现中调用（OpenAI 用隐式 prefix caching）
2. CompletionRequest 是统一抽象，cache_policy 在其上操作
3. 不污染 context 层（context 层只负责构建 prompt 内容）

### Q3：CachePolicy 的策略应该支持哪些维度？

**学习 opencode**：
- `tools: bool` — 是否在最后一个 tool definition 打断点
- `system: bool` — 是否在最后一个 system part 打断点
- `messages: MessageCacheStrategy` — None / LatestUserMessage / LatestAssistant / Tail(n)
- `ttl_seconds: Option<u32>` — cache TTL（Anthropic 支持 5min/1h）

**决议**：采用 opencode 的策略结构，但简化为 MVP：
```rust
pub struct CachePolicy {
    pub tools: bool,
    pub system: bool,
    pub messages: MessageCacheStrategy,
    pub ttl_seconds: Option<u32>,
}

pub enum MessageCacheStrategy {
    None,
    LatestUserMessage,  // MVP 只支持这个（opencode 默认值）
    // LatestAssistant, Tail(usize) — 后续扩展
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            tools: true,
            system: true,
            messages: MessageCacheStrategy::LatestUserMessage,
            ttl_seconds: None,  // None = Anthropic 默认 5min
        }
    }
}
```

### Q4：CompletionRequest 当前没有 cache_control 字段，如何注入？

**当前结构**（synthia-provider/src/types/completion.rs:12-24）：
```rust
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
    pub stop_sequences: Vec<String>,
    pub extra_body: Option<HashMap<String, serde_json::Value>>,
}
```

**问题**：
1. system prompt 作为 Role::System 的 Message 放在 messages 数组开头
2. AnthropicRequest.system 当前是 `Option<String>`（纯文本），丢失结构化能力
3. AnthropicContentBlock 没有 cache_control 字段
4. AnthropicTool 没有 cache_control 字段

**决议**（多专家共识）：
1. CompletionRequest 新增 `cache_policy: Option<CachePolicy>` 字段
2. AnthropicRequest.system 改为 `Option<AnthropicSystem>` 枚举（String 或结构化数组）
3. AnthropicContentBlock 新增 `cache_control: Option<CacheControl>` 字段
4. AnthropicTool 新增 `cache_control: Option<CacheControl>` 字段
5. 新建 `cache_policy.rs` 实现 `apply_cache_policy` 函数
6. 在 `AnthropicProvider::transform_request` 开头调用 `apply_cache_policy`

### Q5：引用相等检查在 Rust 中如何实现？

**opencode 的关键技巧**：
```typescript
if (tools === request.tools && system === request.system && messages === request.messages) 
    return request  // 下游 diff 识别"无变化"
```

**Rust 等价**：Rust 没有运行时引用相等检查的同样需求，因为：
1. 如果 `apply_cache_policy` 不修改 request，`&mut` 不会触发重建
2. 用 `Cow` 或 `&mut` 自然处理"无变化时不重建"
3. 关键是 `mark_last_tool` / `mark_last_system` / `mark_messages` 在已标记时
   返回原对象（用 `Cow<ToolDefinition>`）

**决议**：使用 `Cow<'a, [ToolDefinition]>` 等避免不必要的克隆。但 MVP 阶段
可以先用 `Vec` + clone，性能问题留待 profile 后优化。

### Q6：Provider 感知如何实现？

**opencode**：`RESPECTS_INLINE_HINTS = new Set(["anthropic-messages", "bedrock-converse"])`

**synthia**：当前没有 route 概念，但有 `ProviderConfig` 和具体 provider 实现。

**决议**：在 `ModelProvider` trait 新增 `supports_inline_cache_hints(&self) -> bool`
方法，默认返回 `false`。`AnthropicProvider` 覆盖返回 `true`。OpenAI provider
不覆盖（用隐式 prefix caching）。

`apply_cache_policy` 在调用前检查 `provider.supports_inline_cache_hints()`，
false 时直接返回不修改。

### Q7：如何验证 cache 命中？

**可观测性专家**：`CompletionResponse` 已有 `cached: bool` 字段
（types/completion.rs:43）。PrefixTracker 已观测 prefix hash 稳定性。
但需要新增 `cache_hit_ratio` metric。

**决议**：
1. `apply_cache_policy` 注入后记录 `cache_breakpoints_injected: u32` 到
   `ContextTrace`
2. `CompletionResponse.cached` 已有，主循环已有 telemetry emit
3. PrefixTracker 已有 `windowed_stability_ratio`，作为 cache 命中的代理指标
4. MVP 不新增 metric，复用现有 PrefixStabilityEvent

## 设计取舍

### 优点
1. **10x 成本优化**：Anthropic Prompt Caching 写 1.25x、读 0.1x，单次重用即赢
2. **P1 前缀一致性的主动干预**：从"事后观测"升级为"事前注入"
3. **与 opencode 教科书实现对齐**：跨语言跨生态可复用知识
4. **Provider 感知**：不为 OpenAI 等不支持 inline hints 的 provider 浪费字段

### 风险
1. **AnthropicRequest.system 类型变更**：从 `Option<String>` 到 `Option<AnthropicSystem>`
   是破坏性变更，需要更新 transform.rs 和 tests
2. **AnthropicContentBlock 变更**：新增 `cache_control` 字段需要更新所有 match 分支
3. **测试覆盖**：需要新增 cache_control 序列化测试，确保 Anthropic API 接受
4. **Provider 抽象泄漏**：`supports_inline_cache_hints` 暴露了 provider 实现细节
   到 trait 层，但这是合理的（cache 是 provider 能力）

### 不做
1. **不实现 Tail(n) 策略**：MVP 只做 LatestUserMessage
2. **不实现 Bedrock 支持**：MVP 只支持 anthropic-messages route
3. **不引入 PrefixGuard 拒绝调用**：可靠性工程师反对，PrefixTracker 保持观测
4. **不实现引用相等检查的精细优化**：MVP 用 Vec + clone，profile 后再优化

## 迁移影响

### 破坏性变更
1. `AnthropicRequest.system: Option<String>` → `Option<AnthropicSystem>`
2. `AnthropicContentBlock` 新增 `cache_control: Option<CacheControl>` 字段
3. `AnthropicTool` 新增 `cache_control: Option<CacheControl>` 字段
4. `CompletionRequest` 新增 `cache_policy: Option<CachePolicy>` 字段
5. `ModelProvider` trait 新增 `supports_inline_cache_hints(&self) -> bool` 方法

### 兼容性
- 所有新字段用 `#[serde(skip_serializing_if = "Option::is_none", default)]`
- `supports_inline_cache_hints` 提供默认实现返回 `false`，不破坏现有 provider
- `cache_policy: Option<CachePolicy>` 默认 `None`（等同不注入）
- 当 `cache_policy` 为 `None` 时，行为完全等同当前

### 测试策略
1. 单元测试：`apply_cache_policy` 各策略组合
2. 序列化测试：AnthropicRequest 带 cache_control 的 JSON 输出
3. 集成测试：mock Anthropic API 验证 cache_control 字段被发送
4. 性能测试：PrefixTracker.windowed_stability_ratio 在启用前后对比

## 关键代码引述

### opencode cache-policy.ts 核心思想
```typescript
const AUTO: CachePolicyObject = {
  tools: true,
  system: true,
  messages: "latest-user-message",
}
const RESPECTS_INLINE_HINTS = new Set(["anthropic-messages", "bedrock-converse"])

export const applyCachePolicy = (request: LLMRequest): LLMRequest => {
  if (!RESPECTS_INLINE_HINTS.has(request.model.route.id)) return request
  const policy = resolve(request.cache)
  if (!policy.tools && !policy.system && !policy.messages) return request

  const hint = makeHint(policy.ttlSeconds)
  const tools = policy.tools ? markLastTool(request.tools, hint) : request.tools
  const system = policy.system ? markLastSystem(request.system, hint) : request.system
  const messages = policy.messages ? markMessages(request.messages, policy.messages, hint) : request.messages

  if (tools === request.tools && system === request.system && messages === request.messages) return request
  return LLMRequest.update(request, { tools, system, messages })
}
```

### synthia 当前 Anthropic provider 已有 beta header
```rust
// anthropic/provider/request.rs:42
.header("anthropic-beta", "prompt-caching-2024-07-31")
```
说明 API 层已就绪，只差主动注入 cache_control 字段。
