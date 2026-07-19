## 1. Anthropic 类型扩展（foundation）

- [ ] 1.1 在 `crates/synthia-provider/src/anthropic/types.rs` 新增 `CacheControl` 结构（`r#type: String` + `ttl_seconds: Option<u32>`，`#[serde(rename = "type")]` + `skip_serializing_if`，实现 `Default` 返回 `{"type":"ephemeral"}`）
- [ ] 1.2 在 `anthropic/types.rs` 新增 `AnthropicSystemBlock` 结构（`text: String` + `cache_control: Option<CacheControl>`）
- [ ] 1.3 在 `anthropic/types.rs` 新增 `AnthropicSystem` 枚举（`Text(String)` | `Structured(Vec<AnthropicSystemBlock>)`），实现 `Serialize`/`Deserialize` 保证 `Text` 变体序列化为纯 JSON 字符串
- [ ] 1.4 给 `AnthropicTool` 新增 `cache_control: Option<CacheControl>` 字段（`#[serde(skip_serializing_if = "Option::is_none", default)]`）
- [ ] 1.5 给 `AnthropicContentBlock` 所有变体（或枚举顶层字段）新增 `cache_control: Option<CacheControl>` 字段，`#[serde(skip_serializing_if = "Option::is_none", default)]`
- [ ] 1.6 将 `AnthropicRequest.system` 字段类型从 `Option<String>` 改为 `Option<AnthropicSystem>`

## 2. CachePolicy 模块（新模块）

- [ ] 2.1 创建 `crates/synthia-provider/src/cache_policy.rs` 文件
- [ ] 2.2 定义 `MessageCacheStrategy` 枚举（`None` | `LatestUserMessage`，derive `Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize`）
- [ ] 2.3 定义 `CachePolicy` 结构（`tools: bool` + `system: bool` + `messages: MessageCacheStrategy` + `ttl_seconds: Option<u32>`，`#[serde(default)]`）
- [ ] 2.4 实现 `Default for CachePolicy`（对齐 opencode AUTO：`tools: true, system: true, messages: LatestUserMessage, ttl_seconds: None`）
- [ ] 2.5 实现 `pub fn apply_cache_policy(request: &mut CompletionRequest, policy: &CachePolicy)` 函数，包含 `mark_last_tool` / `mark_last_system` / `mark_last_user_message` 内部 helper
- [ ] 2.6 保证 `apply_cache_policy` 幂等性（同 `(request, policy)` 多次调用产生相同状态，不重复添加 marker）
- [ ] 2.7 在 `crates/synthia-provider/src/lib.rs` 导出 `cache_policy` 模块及 `CachePolicy` / `MessageCacheStrategy` / `apply_cache_policy`

## 3. CompletionRequest 字段扩展

- [ ] 3.1 在 `crates/synthia-provider/src/types/completion.rs` 的 `CompletionRequest` 新增 `cache_policy: Option<CachePolicy>` 字段，`#[serde(skip_serializing_if = "Option::is_none", default)]`
- [ ] 3.2 更新 `CompletionRequest` 的 `Default` impl，`cache_policy: None`
- [ ] 3.3 验证 `cache_policy: None` 时序列化结果不含 `cache_policy` 字段（向后兼容）

## 4. ModelProvider trait 能力探测

- [ ] 4.1 在 `crates/synthia-provider/src/traits.rs` 的 `ModelProvider` trait 新增 `fn supports_inline_cache_hints(&self) -> bool { false }` 默认方法
- [ ] 4.2 在 `AnthropicProvider` impl 中 override `supports_inline_cache_hints` 返回 `true`
- [ ] 4.3 验证其他 provider（OpenAI / mock / test）使用默认 `false`，无需修改

## 5. transform_request 集成

- [ ] 5.1 在 `crates/synthia-provider/src/anthropic/provider/transform.rs` 的 `transform_request` 开头，当 `request.cache_policy` 为 `Some(policy)` 时调用 `apply_cache_policy`
- [ ] 5.2 更新 `transform_request` 的 system 字段构造逻辑：`cache_policy: None` 或 `policy.system == false` 时用 `AnthropicSystem::Text(system_text)`；否则用 `AnthropicSystem::Structured(vec![AnthropicSystemBlock { text, cache_control: Some(CacheControl::default()) }])`
- [ ] 5.3 更新 tools 构造逻辑：当 `policy.tools == true` 且 tools 非空时，最后一个 `AnthropicTool` 的 `cache_control` 设为 `Some(CacheControl::default())`
- [ ] 5.4 更新 messages 构造逻辑：当 `policy.messages == LatestUserMessage` 且最后一条消息是 user 时，在其最后一个 `AnthropicContentBlock` 上设 `cache_control: Some(CacheControl::default())`
- [ ] 5.5 处理 `supports_inline_cache_hints() == false` 时的 short-circuit（不调用 `apply_cache_policy`，即使 `cache_policy: Some`）

## 6. 单元测试

- [ ] 6.1 `cache_policy.rs` 单元测试：`CachePolicy::default()` 字段值正确
- [ ] 6.2 `cache_policy.rs` 单元测试：`apply_cache_policy` 在 `tools: true` 时只标记最后一个 tool
- [ ] 6.3 `cache_policy.rs` 单元测试：`apply_cache_policy` 在 `system: true` 时标记 system（通过 CompletionRequest 的 system message 标记，不直接修改 AnthropicRequest）
- [ ] 6.4 `cache_policy.rs` 单元测试：`apply_cache_policy` 在 `messages: LatestUserMessage` 时只标记最后一条 user message
- [ ] 6.5 `cache_policy.rs` 单元测试：幂等性（连续调用 2 次产生相同 request 状态）
- [ ] 6.6 `cache_policy.rs` 单元测试：`tools: false && system: false && messages: None` 时不修改 request
- [ ] 6.7 `cache_policy.rs` 单元测试：空 tools 不 panic

## 7. 序列化测试

- [ ] 7.1 `anthropic/types.rs` 测试：`CacheControl::default()` 序列化为 `{"type":"ephemeral"}`（无 `ttl_seconds`）
- [ ] 7.2 `anthropic/types.rs` 测试：`CacheControl { ttl_seconds: Some(3600) }` 序列化为 `{"type":"ephemeral","ttl_seconds":3600}`
- [ ] 7.3 `anthropic/types.rs` 测试：`AnthropicTool` `cache_control: None` 时序列化不含 `cache_control` 字段
- [ ] 7.4 `anthropic/types.rs` 测试：`AnthropicSystem::Text("...")` 序列化为纯 JSON 字符串
- [ ] 7.5 `anthropic/types.rs` 测试：`AnthropicSystem::Structured(...)` 序列化为数组带 `cache_control`
- [ ] 7.6 `anthropic/provider/transform.rs` 测试：`cache_policy: None` 时 `AnthropicRequest` JSON 字节等同当前实现
- [ ] 7.7 `anthropic/provider/transform.rs` 测试：`cache_policy: Some(default)` 时 last tool / last user message / system 都带 `cache_control: {"type":"ephemeral"}`
- [ ] 7.8 `anthropic/provider/transform.rs` 测试：`cache_policy: Some({tools: true, system: false, messages: None, ...})` 时只有 last tool 带 `cache_control`

## 8. Trait & Provider 测试

- [ ] 8.1 `traits.rs` 测试：`MockProvider`（默认 impl）`supports_inline_cache_hints()` 返回 `false`
- [ ] 8.2 `anthropic/provider` 测试：`AnthropicProvider::supports_inline_cache_hints()` 返回 `true`
- [ ] 8.3 集成测试：`apply_cache_policy` 在 `supports_inline_cache_hints() == false` 的 provider 上是 no-op

## 9. 文档与验证

- [ ] 9.1 更新 `cache_policy.rs` 模块级 doc 注释（说明对齐 opencode `applyCachePolicy`）
- [ ] 9.2 运行 `cargo +nightly fmt --all` 格式化
- [ ] 9.3 运行 `cargo clippy --all-targets --all-features --tests --all` 修复所有警告
- [ ] 9.4 运行 `cargo test -p synthia-provider` 确保所有测试通过
- [ ] 9.5 运行 `cargo check --workspace` 确保无破坏性变更泄漏到其他 crate
- [ ] 9.6 验证 `AnthropicRequest.system` 类型变更不影响外部 crate（grep 工作区 `.system` 字段引用，确认仅 anthropic crate 内部使用）
