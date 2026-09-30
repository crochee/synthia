## Why

synthia 当前已在 `anthropic/provider/request.rs` 发送 `anthropic-beta: prompt-caching-2024-07-31` header，但**不主动注入 `cache_control` hint** 到 tools / system / messages 字段，导致 Anthropic 无法识别可缓存的 prefix——API 层就绪但应用层缺位。同时 `cache_breaker`（每 5min TTL 注入随机数）破坏 P1 前缀一致性，已列为 P0-1 独立 change。本 change 聚焦 P0-2：实现 opencode `applyCachePolicy` 的 Rust 等价物，主动注入 cache_control，预期带来 10x 成本优化（Anthropic Prompt Caching 写 1.25x、读 0.1x）并使前缀一致性从"被动观测"升级为"主动干预"。

## What Changes

**CompletionRequest 增加 cache_policy 字段**
- From: `CompletionRequest` 无 cache 策略字段，provider 层无主动注入逻辑
- To: 新增 `cache_policy: Option<CachePolicy>`，由调用方（context 层）按需设置；`apply_cache_policy` 在 provider 调用前注入 cache_control
- Reason: opencode 教科书实现证明主动注入是 prompt caching 工作的前提，header 单独不够
- Impact: 非破坏性（字段默认 `None`，行为等同当前）

**AnthropicRequest.system 类型从 String 升级为结构化枚举**
- From: `system: Option<String>`（纯文本，丢失结构化能力，无法在 system 上挂 cache_control）
- To: `system: Option<AnthropicSystem>`，其中 `AnthropicSystem` 为 `Text(String) | Structured(Vec<AnthropicSystemBlock>)` 枚举
- Reason: Anthropic API 要求 `cache_control` 挂在 system block 上，纯字符串无法承载
- Impact: 破坏性（仅在 anthropic crate 内部，外部通过 CompletionRequest 抽象隔离）

**AnthropicContentBlock / AnthropicTool 新增 cache_control 字段**
- From: 两个类型无 cache_control 字段
- To: 新增 `cache_control: Option<CacheControl>` 字段，`#[serde(skip_serializing_if = "Option::is_none", default)]`
- Reason: Anthropic API 要求 cache_control 挂在最后一个 tool / 最后一个 content block 上
- Impact: 非破坏性（serde skip_serializing_if 保证 None 不输出，API 向后兼容）

**ModelProvider trait 新增 supports_inline_cache_hints 方法**
- From: trait 无 provider 能力探测方法
- To: 新增 `fn supports_inline_cache_hints(&self) -> bool { false }` 默认实现，`AnthropicProvider` override 返回 `true`
- Reason: 只有 Anthropic（和 Bedrock Converse）支持 inline cache hints，OpenAI 用隐式 prefix caching，需 provider 感知避免无效注入
- Impact: 非破坏性（默认实现不破坏现有 provider）

## Capabilities

### New Capabilities

- `cache-policy-injection`: Provider 感知的 cache_control hint 主动注入——在 transform_request 前调用 apply_cache_policy，按策略（tools/system/messages/ttl）在最后一个元素上打断点。包含 CachePolicy 结构、apply_cache_policy 函数、provider 能力探测。

### Modified Capabilities

（无——`cache-control-mark` 处理被动追踪与命名空间隔离，本 change 处理主动注入，两者互补不重叠。）

## Impact

**代码影响范围：**
- `crates/synthia-provider/src/types/completion.rs` — CompletionRequest 新增字段
- `crates/synthia-provider/src/traits.rs` — ModelProvider trait 新增方法
- `crates/synthia-provider/src/cache_policy.rs` — 新建模块（apply_cache_policy 实现）
- `crates/synthia-provider/src/anthropic/types.rs` — AnthropicSystem / AnthropicContentBlock / AnthropicTool / CacheControl 类型变更
- `crates/synthia-provider/src/anthropic/provider/transform.rs` — 在 transform_request 开头调用 apply_cache_policy
- `crates/synthia-provider/src/anthropic/provider/request.rs` — 无变更（header 已就绪）

**依赖：**
- 无新增 crate 依赖
- 复用现有 `serde` / `tracing`

**测试：**
- 单元测试：apply_cache_policy 各策略组合（4 个 mark 函数）
- 序列化测试：AnthropicRequest 带 cache_control 的 JSON 输出
- 集成测试：mock Anthropic API 验证 cache_control 字段被发送
- 性能测试：PrefixTracker.windowed_stability_ratio 启用前后对比

**可观测性：**
- 复用现有 PrefixStabilityEvent（不新增 metric）
- 复用 CompletionResponse.cached: bool 字段

**破坏性变更：**
- `AnthropicRequest.system` 类型变更（仅 anthropic crate 内部，外部通过 CompletionRequest 隔离）
- 所有 match AnthropicContentBlock 的分支需考虑新字段（但 `Option<CacheControl>` 默认 None 不影响语义）
