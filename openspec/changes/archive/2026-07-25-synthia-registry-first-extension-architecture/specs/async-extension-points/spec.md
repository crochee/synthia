# Spec: async-extension-points

## ADDED Requirements

### Requirement: 异步 ExtensionPoints handler — SHALL convert sync handlers to async

ToolExtensionRegistry 和 LlmExtensionRegistry 的 handler SHALL 从同步改为异步。

- **R1.1**: `BeforeHandler` 签名从 `Arc<dyn Fn(&BeforeToolCall) -> Action<BeforeToolCall> + Send + Sync>` 改为 `Arc<dyn for<'a> Fn(&'a BeforeToolCall) -> Pin<Box<dyn Future<Output = Action<BeforeToolCall>> + Send + 'a>> + Send + Sync>`
- **R1.2**: `AfterHandler` 和 `DefinitionHandler` 同样改为异步签名
- **R1.3**: `fire_before`、`fire_after`、`fire_definition` 改为 `async fn`
- **R1.4**: LlmExtensionRegistry 的所有 handler 同样改为异步

#### Scenario: async handler invocation
- **WHEN** a tool extension handler is registered with an async signature
- **THEN** `fire_before` and `fire_after` return a `Future` that resolves to the handler result

### Requirement: 同步兼容包装 — SHALL provide sync-to-async wrapper

系统 SHALL 提供 `register_before_sync()` 方法，接受同步 handler 并包装为异步。
- **R2.2**: 包装逻辑：`|input| Box::pin(async move { sync_handler(input) })`
- **R2.3**: 现有同步 handler 注册自动迁移为 `register_before_sync()`

#### Scenario: sync handler auto-wrapping
- **WHEN** a sync handler is registered via `register_before_sync()`
- **THEN** it is wrapped as `Box::pin(async move { sync_handler(input) })` and behaves identically to the original sync invocation

### Requirement: 调用点适配 — SHALL adapt call sites to async

调用点 SHALL 适配异步上下文：main_loop.rs 中调用 `fire_before`/`fire_after` 的地方改为 `.await`。
- **R3.2**: OTel span 在异步上下文中正确传播（使用 `tracing::instrument`）

#### Scenario: async call site with tracing
- **WHEN** `fire_before` or `fire_after` is called in main_loop.rs
- **THEN** the call uses `.await` and OTel spans propagate correctly across the async boundary
