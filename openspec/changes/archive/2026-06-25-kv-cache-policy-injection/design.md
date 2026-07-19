## Context

synthia 当前 `synthia-context/src/system_context.rs` 使用 `cache_breaker`（每 5min TTL 过期后生成随机 `cb_xxxxxxxx`）注入 system prompt，违反 P1 前缀一致性原则。同时 `synthia-provider` 已通过 `anthropic-beta: prompt-caching-2024-07-31` header 启用 Anthropic Prompt Caching，但**不主动注入 `cache_control` hint** 到 tools / system / messages，导致 Anthropic 无法识别可缓存的 prefix——header 启用是必要非充分条件。

对标 opencode `packages/llm/src/cache-policy.ts` 的教科书实现：
- `applyCachePolicy(request)` 在请求构建最后一步主动注入 `cache_control` hint
- Provider 感知：仅在 `anthropic-messages` 和 `bedrock-converse` route 注入
- `latest-user-message` 策略：只在最后一条用户消息打断点，最大化 cache 命中
- 引用相等检查：三个标记都没变 → 返回原对象，下游 diff 识别"无变化"

本 change 是 P0-2（多专家对抗性讨论 2026-06-25 路线图），与 P0-1（删除 cache_breaker，独立 change）互补。本 change 不涉及 cache_breaker 的删除，仅实现主动注入逻辑。

**约束：**
- P1 前缀一致性是最高约束（违反代价 10x 成本 + I/O 瓶颈）
- cache_policy 注入必须幂等（同一 request 多次调用产生相同结果）
- provider 抽象不能泄漏 Anthropic 实现细节到 context 层
- MVP 不引入 Tail(n) / Bedrock / 引用相等精细优化

**利益相关者：**
- 架构师：保持 provider 抽象清晰，cache_policy 在 provider 层实现
- 性能工程师：10x 成本优化 + PrefixTracker 观测升级
- 可靠性工程师：注入必须幂等，不能引入新的 prefix 不稳定源
- DevOps：deploy 无变更，纯 crate 内部修改

## Goals / Non-Goals

**Goals:**
- 实现 `apply_cache_policy` 函数，按 `CachePolicy` 策略在 tools / system / messages 的最后一个元素上注入 `cache_control: {type: "ephemeral"}` hint
- Provider 感知：只在 `supports_inline_cache_hints() == true` 的 provider 上注入
- `CachePolicy` 结构支持 tools / system / messages 三个维度的开关 + TTL 选项
- MVP 支持 `LatestUserMessage` 策略（opencode 默认值），最大化 cache 命中
- 保持向后兼容：`cache_policy: None` 时行为完全等同当前
- 复用现有可观测性（PrefixStabilityEvent、CompletionResponse.cached），不新增 metric

**Non-Goals:**
- 不删除 `cache_breaker`（P0-1 独立 change 处理）
- 不实现 `Tail(n)` / `LatestAssistant` 策略（MVP 后扩展）
- 不实现 Bedrock Converse 支持（MVP 仅 anthropic-messages route）
- 不引入 PrefixGuard 拒绝调用（PrefixTracker 保持观测角色）
- 不实现引用相等精细优化（MVP 用 Vec + clone，profile 后再优化为 Cow）
- 不修改 `cache-control-mark` spec（命名空间隔离由该 spec 处理，与本 change 互补）
- 不新增 OTel metric（复用 PrefixStabilityEvent）

## Decisions

### D1：实现位置 — provider 层而非 context 层

- **选择**：在 `synthia-provider` crate 新建 `cache_policy.rs` 模块，导出 `apply_cache_policy(&mut CompletionRequest, &CachePolicy)` 函数。在 `AnthropicProvider::transform_request` 开头调用。
- **理由**：
  1. Provider 感知——只有 provider 知道 route 是否支持 inline cache hints（opencode 的 `RESPECTS_INLINE_HINTS`）
  2. CompletionRequest 是统一抽象，cache_policy 在其上操作，不污染 context 层
  3. transform_request 是 Anthropic body 构建最后一步，可在此注入 cache_control 到 JSON 结构
  4. OpenAI 等用隐式 prefix caching 的 provider 不需要注入，trait 默认 `supports_inline_cache_hints() -> false` 自动跳过
- **已考虑 alternative**：
  - 在 context 层实现（架构师初始建议）→ 拒绝，因为 context 层不知道 route 是否支持
  - 在 transform_request 内联实现 → 拒绝，因为 transform.rs 已 244 行，分离模块更清晰

### D2：CachePolicy 结构 — 对齐 opencode，MVP 简化

- **选择**：
  ```rust
  pub struct CachePolicy {
      pub tools: bool,
      pub system: bool,
      pub messages: MessageCacheStrategy,
      pub ttl_seconds: Option<u32>,
  }
  pub enum MessageCacheStrategy {
      None,
      LatestUserMessage,
  }
  impl Default for CachePolicy {
      fn default() -> Self {
          Self {
              tools: true, system: true,
              messages: MessageCacheStrategy::LatestUserMessage,
              ttl_seconds: None,  // None = Anthropic 默认 5min
          }
      }
  }
  ```
- **理由**：对齐 opencode 的 `AUTO` 默认值，MVP 只支持 `LatestUserMessage`（opencode 默认且最高 ROI 策略），`Tail(n)` / `LatestAssistant` 留待后续扩展
- **已考虑 alternative**：
  - 用 `Vec<MessageCacheStrategy>` 支持多策略组合 → 拒绝，YAGNI，MVP 不需要
  - 用 `enum CachePolicy { Auto, Off, Custom(...) }` → 拒绝，扁平字段更易序列化和测试

### D3：CompletionRequest 字段注入 — Option + serde skip

- **选择**：`CompletionRequest` 新增 `cache_policy: Option<CachePolicy>` 字段，`#[serde(skip_serializing_if = "Option::is_none", default)]`
- **理由**：
  1. `None` 时行为完全等同当前（向后兼容）
  2. 由调用方（context 层）按需设置，provider 层不主动创建
  3. `apply_cache_policy` 在 `Some(policy)` 时注入，`None` 时直接返回
- **已考虑 alternative**：
  - 默认 `CachePolicy::default()` 而非 `None` → 拒绝，会改变未启用 provider 的行为
  - 在 trait 层注入 → 拒绝，破坏 provider 抽象（context 层不应感知 cache_policy 存在）

### D4：AnthropicRequest.system 类型升级 — String → 枚举

- **选择**：`system: Option<AnthropicSystem>`，其中 `AnthropicSystem` 为 `Text(String) | Structured(Vec<AnthropicSystemBlock>)` 枚举。`AnthropicSystemBlock` 含 `text: String` + `cache_control: Option<CacheControl>`。
- **理由**：
  1. Anthropic API 要求 `cache_control` 挂在 system block 上，纯字符串无法承载
  2. 枚举保留 `Text` 变体向后兼容序列化（无 cache_control 时序列化为纯字符串）
  3. `Structured` 变体支持后续扩展（多个 system part + 各自 cache_control）
- **已考虑 alternative**：
  - 直接改为 `Vec<AnthropicSystemBlock>` → 拒绝，破坏单 system 文本场景的序列化
  - 用 `serde_json::Value` → 拒绝，丢失类型安全
- **破坏性影响**：仅 anthropic crate 内部，外部通过 CompletionRequest 抽象隔离

### D5：AnthropicContentBlock / AnthropicTool 字段新增 — Option + skip_serializing_if

- **选择**：两个类型新增 `cache_control: Option<CacheControl>` 字段，`#[serde(skip_serializing_if = "Option::is_none", default)]`
- **理由**：
  1. Anthropic API 要求 cache_control 挂在最后一个 tool / 最后一个 content block 上
  2. `skip_serializing_if` 保证 None 不输出，API 向后兼容
  3. 所有 match 分支默认 `None`，不破坏语义
- **已考虑 alternative**：
  - 用 wrapper enum `AnthropicContentBlock::WithCache(AnthropicContentBlock, CacheControl)` → 拒绝，match 分支爆炸
  - 用 `Box<CacheControl>` → 拒绝，Option 已足够，无需堆分配

### D6：Provider 能力探测 — trait 默认方法

- **选择**：`ModelProvider` trait 新增 `fn supports_inline_cache_hints(&self) -> bool { false }` 默认实现，`AnthropicProvider` override 返回 `true`
- **理由**：
  1. 只有 Anthropic（和 Bedrock Converse）支持 inline cache hints，OpenAI 用隐式 prefix caching
  2. 默认 `false` 不破坏现有 provider（OpenAI / mock / test provider 都不需修改）
  3. `apply_cache_policy` 在调用前检查 `supports_inline_cache_hints()`，false 时直接返回不修改
- **已考虑 alternative**：
  - 用 `ProviderCapability` enum 标记 → 拒绝，过度设计
  - 在 transform_request 内部硬编码（Anthropic 始终注入）→ 拒绝，无法支持用户显式关闭

### D7：引用相等检查 — MVP 用 Vec + clone

- **选择**：MVP 阶段 `mark_last_tool` / `mark_last_system` / `mark_messages` 用 `Vec` + `clone`，不实现引用相等检查。`apply_cache_policy` 始终返回修改后的 request。
- **理由**：
  1. Rust 没有运行时引用相等检查的同样需求（无 V8 同引用语义）
  2. clone 成本在小规模 tools（< 50）和 messages（< 100）场景可忽略
  3. profile 后再优化为 `Cow<'a, [T]>` 或 `Arc<[T]>`
- **已考虑 alternative**：
  - 直接用 `Cow<'a, [ToolDefinition]>` → 拒绝，引入生命周期复杂度，MVP 不必要
  - 用 `Arc<[T]>` + copy-on-write → 拒绝，过度优化
- **Trade-off**：MVP 牺牲少量 clone 性能换取代码简洁，接受理由是 tools / messages 规模通常小

### D8：可观测性 — 复用现有指标

- **选择**：不新增 metric，复用现有 PrefixStabilityEvent 和 CompletionResponse.cached 字段
- **理由**：
  1. PrefixTracker 已观测 prefix hash 稳定性（windowed_stability_ratio），作为 cache 命中的代理指标
  2. CompletionResponse.cached: bool 已记录每次调用是否命中 cache
  3. MVP 阶段不引入新 metric 避免增加 telemetry 表面积
- **已考虑 alternative**：
  - 新增 `cache_breakpoints_injected: u32` 到 ContextTrace → 拒绝，YAGNI，PrefixStabilityEvent 已足够观测效果
  - 新增 OTel metric → 拒绝，OTel 集成是 P1-5 独立 change

## Risks / Trade-offs

- **[Risk] AnthropicRequest.system 类型变更破坏 anthropic crate 内部代码** → Mitigation: 用 `AnthropicSystem::Text(String)` 变体保留纯文本序列化路径；`transform_request` 在无 cache_policy 时构造 `Text(system_text)`，行为等同当前
- **[Risk] AnthropicContentBlock 所有 match 分支需考虑新字段** → Mitigation: `Option<CacheControl>` 默认 `None`，serde `skip_serializing_if` 保证不输出；现有 match 分支用 `_` 通配或显式 `cache_control: _` 不影响语义
- **[Risk] 测试覆盖不足导致 Anthropic API 拒绝** → Mitigation: 新增序列化测试（带 cache_control 的 JSON 输出对比预期 schema）+ 集成测试（mock Anthropic API 验证字段被发送）
- **[Risk] Provider 抽象泄漏**：`supports_inline_cache_hints` 暴露 provider 实现细节到 trait 层 → Mitigation: 这是合理的——cache 是 provider 能力（opencode 同设计），trait 方法名表达"能力"而非"实现"
- **[Trade-off] MVP 用 Vec + clone 牺牲少量性能** → 接受理由：tools / messages 规模通常小（< 100），clone 成本可忽略；profile 后可优化为 Cow
- **[Trade-off] 不实现 Tail(n) / Bedrock / 引用相等精细优化** → 接受理由：MVP 聚焦最高 ROI 的 `LatestUserMessage` + Anthropic，避免过早泛化
- **[Risk] cache_policy 注入不幂等导致 prefix 不稳定** → Mitigation: `apply_cache_policy` 必须满足幂等性（同一 request + policy 多次调用产生相同结果）；单元测试覆盖幂等性场景

## Migration Plan

**部署顺序：**
1. Phase 1（本 change）：实现 cache_policy.rs + 类型变更 + transform_request 集成。`cache_policy: None` 默认行为等同当前，零风险部署。
2. Phase 2（独立 change，P0-1）：删除 `cache_breaker`，依赖 `prompt_cache_key` 命名空间隔离。
3. Phase 3（后续）：context 层主动设置 `cache_policy: Some(CachePolicy::default())`，启用主动注入。
4. Phase 4（后续）：扩展 `Tail(n)` / `LatestAssistant` / Bedrock 支持。

**Rollback 策略：**
- 代码层：revert 本 change 的 commit（`cache_policy: None` 默认保证 revert 无副作用）
- 运行时：context 层将 `cache_policy` 设为 `None` 即可禁用主动注入

**验收条件：**
- 单元测试：`apply_cache_policy` 各策略组合通过
- 序列化测试：带 cache_control 的 AnthropicRequest JSON 输出符合 Anthropic API schema
- 集成测试：mock Anthropic API 收到 cache_control 字段
- 性能测试：PrefixTracker.windowed_stability_ratio 在启用前后对比（启用后应 ≥ 95%）
- 幂等性测试：同一 request + policy 多次调用产生相同结果

## Open Questions

- **OQ1**：context 层何时主动设置 `cache_policy: Some(CachePolicy::default())`？是默认启用还是配置开关？
  - 当前决议：MVP 阶段 `cache_policy: None` 默认，由后续 change 决定启用策略
  - 待 Phase 3 决议
- **OQ2**：`ttl_seconds: Some(3600)`（1h TTL）是否需要 anthropic-beta header 升级？
  - 当前决议：MVP 不支持 1h TTL，`ttl_seconds` 始终为 `None`（Anthropic 默认 5min）
  - 待后续验证 Anthropic API 文档
- **OQ3**：`cache-control-mark` spec 的 `CacheControlMark` 与本 change 的 `CachePolicy` 是否需要统一？
  - 当前决议：不统一，两者职责不同（前者追踪/命名空间隔离，后者主动注入）
  - 待后续评估是否合并为统一抽象
