<!--
Cumulative spec for cache-policy-injection capability.
Provider 感知的 cache_control hint 主动注入。

格式硬规则：
- Requirement 句子 MUST 含 SHALL 或 MUST
- 每个 Requirement MUST 至少有一个 `#### Scenario:`
- Scenario MUST 用 level-4 (`####`)
-->

# cache-policy-injection Specification

## Purpose

Define a provider-neutral `CachePolicy` and `apply_cache_policy` function that injects `cache_control` hints into `CompletionRequest` fields (tools, system, messages) so that provider `transform_request` implementations can translate the hints to provider-specific cache directives (e.g. Anthropic `{"type":"ephemeral"}`). Aligned with opencode's `applyCachePolicy` from `packages/llm/src/cache-policy.ts`.
## Requirements
### Requirement: CachePolicy struct SHALL encode tools/system/messages/ttl dimensions

`CachePolicy` SHALL be a struct with four fields:
- `tools: bool` — whether to inject `cache_control` hint on the last tool definition
- `system: bool` — whether to inject `cache_control` hint on the last system block
- `messages: MessageCacheStrategy` — message caching strategy (None or LatestUserMessage)
- `ttl_seconds: Option<u32>` — cache TTL in seconds; `None` means provider default (Anthropic: 5min)

`MessageCacheStrategy` SHALL be a `Copy + Eq` enum with variants `None` and `LatestUserMessage`.

`CachePolicy` SHALL implement `Default` returning `tools: true, system: true, messages: LatestUserMessage, ttl_seconds: None` (aligned with opencode `AUTO`).

`CachePolicy` and `MessageCacheStrategy` SHALL derive `Clone, Debug, Serialize, Deserialize` with `serde(default)` on deserialization.

#### Scenario: Default CachePolicy aligns with opencode AUTO

- **WHEN** `CachePolicy::default()` is constructed
- **THEN** `tools` SHALL be `true`
- **THEN** `system` SHALL be `true`
- **THEN** `messages` SHALL be `MessageCacheStrategy::LatestUserMessage`
- **THEN** `ttl_seconds` SHALL be `None`

#### Scenario: CachePolicy serializes with skip_serializing_if for None ttl

- **WHEN** a `CachePolicy { tools: true, system: false, messages: MessageCacheStrategy::None, ttl_seconds: None }` is serialized to JSON
- **THEN** the JSON output SHALL include `"tools": true` and `"system": false`
- **THEN** the JSON output SHALL include `"messages": "None"`
- **THEN** the JSON output SHALL NOT include `ttl_seconds` field

---

### Requirement: apply_cache_policy SHALL inject cache_control hints idempotently

`apply_cache_policy(request: &mut CompletionRequest, policy: &CachePolicy)` SHALL be a free function in `synthia_provider::cache_policy` module.

When `policy.tools == true` and `request.tools` is non-empty, the function SHALL set `cache_control` on the LAST element of `request.tools` (the last `ToolDefinition`).

When `policy.system == true` and a system message exists in `request.messages`, the function SHALL mark the system message for cache_control injection (the actual injection happens at provider transform time because AnthropicRequest.system is provider-specific).

When `policy.messages == LatestUserMessage`, the function SHALL mark the last user message in `request.messages` for cache_control injection.

The function SHALL be idempotent: calling it multiple times with the same `(request, policy)` SHALL produce the same resulting request state.

The function SHALL NOT modify the request when `policy.tools == false && policy.system == false && policy.messages == None`.

#### Scenario: Tools injection marks last tool definition

- **WHEN** `apply_cache_policy` is called with `policy.tools == true` on a request with 3 tool definitions
- **THEN** the 3rd tool definition SHALL have `cache_control` set to `Some(CacheControl::default())`
- **THEN** the 1st and 2nd tool definitions SHALL have `cache_control` set to `None`

#### Scenario: Idempotency on repeated calls

- **WHEN** `apply_cache_policy(req, &policy)` is called twice on the same `req` with the same `policy`
- **THEN** the resulting `req` state SHALL be byte-identical between the two calls
- **THEN** no duplicate `cache_control` markers SHALL be added

#### Scenario: No-op when all dimensions disabled

- **WHEN** `apply_cache_policy` is called with `CachePolicy { tools: false, system: false, messages: MessageCacheStrategy::None, ttl_seconds: None }`
- **THEN** the request SHALL remain unmodified (byte-identical to input)

#### Scenario: Empty tools is skipped

- **WHEN** `apply_cache_policy` is called with `policy.tools == true` on a request with empty `tools` vec
- **THEN** the function SHALL NOT panic
- **THEN** no `cache_control` marker SHALL be injected

---

### Requirement: AnthropicProvider SHALL apply cache_policy in transform_request

`AnthropicProvider::transform_request` SHALL call `apply_cache_policy` on the `CompletionRequest` BEFORE constructing the `AnthropicRequest` body, but ONLY when `request.cache_policy` is `Some(policy)`.

When `request.cache_policy` is `None`, `transform_request` SHALL behave identically to the current implementation (no cache_control injection, backward compatible).

When `request.cache_policy` is `Some(policy)`, `transform_request` SHALL:
1. Apply the policy to mark cache_control hints on CompletionRequest fields
2. Construct `AnthropicRequest.system` as `Some(AnthropicSystem::Structured(...))` with cache_control on the last system block (when `policy.system == true`)
3. Construct `AnthropicRequest.tools` with cache_control on the last tool (when `policy.tools == true`)
4. Construct `AnthropicRequest.messages` with cache_control on the last user message content block (when `policy.messages == LatestUserMessage`)

#### Scenario: None cache_policy preserves current behavior

- **WHEN** `transform_request` is called with a `CompletionRequest` having `cache_policy: None`
- **THEN** the resulting `AnthropicRequest.system` SHALL be `Some(AnthropicSystem::Text(system_text))` (preserving current String form)
- **THEN** no `cache_control` fields SHALL appear in tools / system / messages
- **THEN** the resulting JSON SHALL be byte-identical to the current implementation's output

#### Scenario: Some cache_policy injects cache_control on last tool

- **WHEN** `transform_request` is called with `cache_policy: Some(CachePolicy::default())` and 3 tools
- **THEN** the resulting `AnthropicRequest.tools[2].cache_control` SHALL be `Some(CacheControl { r#type: "ephemeral" })`
- **THEN** `AnthropicRequest.tools[0].cache_control` and `tools[1].cache_control` SHALL be `None`

#### Scenario: Some cache_policy injects cache_control on last user message

- **WHEN** `transform_request` is called with `cache_policy: Some(CachePolicy::default())` and messages ending with a user message
- **THEN** the last content block of the last user message SHALL have `cache_control: Some(CacheControl { r#type: "ephemeral" })`

#### Scenario: Some cache_policy injects cache_control on structured system

- **WHEN** `transform_request` is called with `cache_policy: Some(CachePolicy::default())` and a system message present
- **THEN** the resulting `AnthropicRequest.system` SHALL be `Some(AnthropicSystem::Structured(blocks))` where the last block has `cache_control: Some(CacheControl { r#type: "ephemeral" })`

---

### Requirement: ModelProvider trait SHALL declare supports_inline_cache_hints with default false

`ModelProvider` trait SHALL include a method `fn supports_inline_cache_hints(&self) -> bool { false }` with a default implementation returning `false`.

`AnthropicProvider` SHALL override this method to return `true`.

`apply_cache_policy` SHALL be invoked ONLY when `provider.supports_inline_cache_hints()` returns `true`. When the provider returns `false`, the function SHALL be a no-op (regardless of `request.cache_policy` value).

#### Scenario: AnthropicProvider declares inline cache hint support

- **WHEN** `AnthropicProvider::supports_inline_cache_hints()` is called
- **THEN** it SHALL return `true`

#### Scenario: Default provider implementation returns false

- **WHEN** a `MockProvider` (using default trait impl) calls `supports_inline_cache_hints()`
- **THEN** it SHALL return `false`

#### Scenario: apply_cache_policy is no-op for non-supporting provider

- **WHEN** `apply_cache_policy` is gated by `provider.supports_inline_cache_hints() == false`
- **AND** `request.cache_policy` is `Some(CachePolicy::default())`
- **THEN** the request SHALL remain unmodified
- **THEN** no cache_control markers SHALL be injected

---

### Requirement: CacheControl struct SHALL serialize to Anthropic API schema

`CacheControl` SHALL be a struct with a single field `r#type: String` defaulting to `"ephemeral"`.

`CacheControl` SHALL derive `Clone, Debug, Serialize, Deserialize` and use `#[serde(rename = "type")]` on the `r#type` field.

When serialized, `CacheControl::default()` SHALL produce JSON `{"type": "ephemeral"}`.

When `ttl_seconds: Some(ttl)` is set on the originating `CachePolicy`, the `CacheControl` instance SHALL include a `ttl_seconds: u32` field with the value (Anthropic supports 5min = 300 / 1h = 3600). When `ttl_seconds: None`, the `CacheControl` SHALL NOT include the `ttl_seconds` field.

`CacheControl` SHALL be attached to:
- `AnthropicTool.cache_control: Option<CacheControl>` (on the last tool)
- `AnthropicContentBlock.cache_control: Option<CacheControl>` (on the last content block of last user message)
- `AnthropicSystemBlock.cache_control: Option<CacheControl>` (on the last system block)

All `cache_control` fields SHALL use `#[serde(skip_serializing_if = "Option::is_none", default)]` to ensure backward-compatible serialization.

#### Scenario: Default CacheControl serializes to ephemeral

- **WHEN** `CacheControl::default()` is serialized to JSON
- **THEN** the output SHALL be `{"type":"ephemeral"}` (no `ttl_seconds` field)

#### Scenario: CacheControl with ttl_seconds

- **WHEN** `CacheControl { r#type: "ephemeral".to_string(), ttl_seconds: Some(3600) }` is serialized
- **THEN** the output SHALL be `{"type":"ephemeral","ttl_seconds":3600}`

#### Scenario: None cache_control is omitted from serialization

- **WHEN** an `AnthropicTool` with `cache_control: None` is serialized
- **THEN** the JSON output SHALL NOT include a `cache_control` field

#### Scenario: AnthropicRequest with cache_control matches Anthropic API schema

- **WHEN** an `AnthropicRequest` is constructed with `cache_policy: Some(CachePolicy::default())` and serialized to JSON
- **THEN** the last tool definition SHALL include `"cache_control": {"type":"ephemeral"}`
- **THEN** the last user message content block SHALL include `"cache_control": {"type":"ephemeral"}`
- **THEN** the system field (when structured) SHALL have its last block include `"cache_control": {"type":"ephemeral"}`

---

### Requirement: AnthropicSystem enum SHALL preserve Text and Structured variants

`AnthropicSystem` SHALL be an enum with two variants:
- `Text(String)` — serializes as a plain JSON string (backward compatible with current `Option<String>`)
- `Structured(Vec<AnthropicSystemBlock>)` — serializes as a JSON array of system blocks

`AnthropicSystemBlock` SHALL be a struct with:
- `text: String` — the system prompt text
- `cache_control: Option<CacheControl>` — optional cache hint, `#[serde(skip_serializing_if = "Option::is_none", default)]`

`AnthropicRequest.system: Option<AnthropicSystem>` SHALL replace the current `Option<String>` field.

When `cache_policy: None` (or `policy.system == false`), `transform_request` SHALL construct `AnthropicSystem::Text(system_text)` to preserve current serialization behavior.

When `cache_policy: Some(policy)` with `policy.system == true`, `transform_request` SHALL construct `AnthropicSystem::Structured(vec![AnthropicSystemBlock { text: system_text, cache_control: Some(CacheControl::default()) }])`.

#### Scenario: Text variant serializes as plain string

- **WHEN** `AnthropicSystem::Text("You are helpful.".to_string())` is serialized
- **THEN** the JSON output SHALL be `"You are helpful."` (a plain JSON string, not an object)

#### Scenario: Structured variant serializes as array with cache_control

- **WHEN** `AnthropicSystem::Structured(vec![AnthropicSystemBlock { text: "You are helpful.".to_string(), cache_control: Some(CacheControl::default()) }])` is serialized
- **THEN** the JSON output SHALL be `[{"type":"text","text":"You are helpful.","cache_control":{"type":"ephemeral"}}]`

#### Scenario: Text variant preserves backward compatibility

- **WHEN** `transform_request` is called with `cache_policy: None` and a system message `"You are helpful."`
- **THEN** the resulting `AnthropicRequest.system` SHALL be `Some(AnthropicSystem::Text("You are helpful.".to_string()))`
- **THEN** serializing the `AnthropicRequest` SHALL produce a JSON `system` field that is a plain string (matching pre-change behavior)

### Requirement: Production assembler paths SHALL inject default CachePolicy

The following production code paths that construct a `CompletionRequest` SHALL set `cache_policy: Some(CachePolicy::default())` instead of `cache_policy: None`:
- `synthia_context::assembler::pipeline::ContextAssembler::prepare`
- `synthia_context::service::DefaultContextService::assemble`
- `synthia_context::summarizer::generator` (the summarizer's context assembly call)
- `synthia_agent::context::assemble_context` (the agent's context assembly)

When the provider's `supports_inline_cache_hints()` returns `false` (the default for non-Anthropic providers), the injected `CachePolicy` SHALL be a no-op at the provider transform layer (per the existing `supports_inline_cache_hints` guard defined in the `ModelProvider trait SHALL declare supports_inline_cache_hints with default false` requirement). Thus injecting `Some(CachePolicy::default())` is safe for all providers.

The injected `CachePolicy::default()` SHALL carry `tools: true, system: true, messages: LatestUserMessage, ttl_seconds: None` (aligned with opencode `AUTO`).

#### Scenario: ContextAssembler prepare injects default cache policy

- **WHEN** `ContextAssembler::prepare` constructs a `CompletionRequest`
- **THEN** `request.cache_policy` SHALL be `Some(CachePolicy::default())`
- **AND** `CachePolicy::default()` SHALL have `tools: true`, `system: true`, `messages: LatestUserMessage`, `ttl_seconds: None`

#### Scenario: DefaultContextService assemble injects default cache policy

- **WHEN** `DefaultContextService::assemble` constructs a `CompletionRequest`
- **THEN** `request.cache_policy` SHALL be `Some(CachePolicy::default())`

#### Scenario: Summarizer generator injects default cache policy

- **WHEN** the summarizer's generator path constructs a `CompletionRequest`
- **THEN** `request.cache_policy` SHALL be `Some(CachePolicy::default())`

#### Scenario: Agent assemble_context injects default cache policy

- **WHEN** `assemble_context` in `synthia_agent::context` constructs a `CompletionRequest`
- **THEN** `request.cache_policy` SHALL be `Some(CachePolicy::default())`

#### Scenario: Non-Anthropic provider ignores injected cache policy

- **WHEN** a provider with `supports_inline_cache_hints() == false` receives a `CompletionRequest` with `cache_policy: Some(CachePolicy::default())`
- **THEN** `transform_request` SHALL NOT inject any `cache_control` markers
- **AND** the resulting request SHALL be byte-identical to a request with `cache_policy: None`

#### Scenario: CacheScope flows with injected CachePolicy

- **WHEN** `ContextAssembler::prepare` injects `Some(CachePolicy::default())`
- **AND** the assembler has access to `user_id` and `session_id` (via `CacheScope::new`)
- **THEN** the resulting `CacheControlMark` carried by the request SHALL have `scope = CacheScope::new(user_id, session_id)`
- **AND** the scope SHALL flow to `AnthropicProvider::transform_request` without lossy conversion

