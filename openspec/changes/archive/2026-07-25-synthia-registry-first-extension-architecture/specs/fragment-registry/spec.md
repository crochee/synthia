# Spec: fragment-registry

## ADDED Requirements

### Requirement: ContextFragment trait — SHALL define prompt context fragment interface

`ContextFragment` trait SHALL 定义注入到 prompt 的上下文片段接口。

- **R1.1**: `ContextFragment` trait 包含 `name() -> &str`、`priority() -> u32`、`is_active(&FragmentContext) -> bool`、`render(&FragmentContext) -> Result<String, FragmentError>` 四个方法
- **R1.2**: `priority()` 越小越靠前，相同 priority 按注册顺序
- **R1.3**: `is_active()` 控制片段是否在当前上下文中激活
- **R1.4**: `render()` 返回片段的文本内容，注入到 system prompt
- **R1.5**: `ContextFragment` 是 `Send + Sync + 'static`

#### Scenario: fragment rendering with priority ordering
- **WHEN** multiple fragments are registered with different priorities
- **THEN** `render_active()` returns their text concatenated in ascending priority order, skipping inactive fragments

### Requirement: FragmentRegistry — SHALL manage and render context fragments

`FragmentRegistry` SHALL 管理一组 `Arc<dyn ContextFragment>`。
- **R2.2**: `register(fragment)` 注册片段
- **R2.3**: `unregister(name)` 反注册片段
- **R2.4**: `render_active(ctx)` 渲染所有激活片段，按 priority 排序，拼接为完整 system prompt 片段
- **R2.5**: 线程安全：使用 `RwLock` 保护内部 Vec

#### Scenario: thread-safe fragment registration
- **WHEN** fragments are registered and unregistered concurrently from multiple threads
- **THEN** FragmentRegistry remains consistent and `render_active()` produces a valid result without data races

### Requirement: 内建 ContextFragment — SHALL provide built-in fragment implementations

系统 SHALL 提供以下内建片段（替代现有 ContextAssembler 的硬编码逻辑）：

- **R3.1**: `SystemPromptFragment` — 系统提示（从 AgentConfig 读取）
- **R3.2**: `TokenBudgetFragment` — 令牌预算提示（当前/最大/剩余）
- **R3.3**: `SkillsFragment` — 技能指令（从 SkillRegistry 渲染）
- **R3.4**: `PermissionsFragment` — 权限说明（从 PermissionChecker 渲染）
- **R3.5**: `PluginsFragment` — 插件指令（从 PluginRegistry 渲染）
- **R3.6**: `EnvironmentFragment` — 环境信息（工作目录、操作系统等）
- **R3.7**: `RolloutBudgetFragment` — 变更预算提示（从 RolloutTracker 渲染）
- **R3.8**: `CustomFragment` — 自定义片段（用户通过配置或代码注册）

#### Scenario: built-in fragment coverage
- **WHEN** the default fragment set is registered
- **THEN** the rendered output covers system prompt, token budget, skills, permissions, plugins, environment, rollout budget, and custom fragments

### Requirement: ContextAssembler 迁移 — SHALL delegate to FragmentRegistry

`ContextAssembler` SHALL 委托给 `FragmentRegistry::render_active()`。
- **R4.2**: 现有 ContextAssembler 的硬编码逻辑拆分为独立 Fragment 实现
- **R4.3**: `ContextAssembler` 标记 `#[deprecated]`，6 个月后移除

#### Scenario: ContextAssembler delegates to FragmentRegistry
- **WHEN** `ContextAssembler::assemble()` is called
- **THEN** it delegates to `FragmentRegistry::render_active()` and produces the same output as before migration

### Requirement: FragmentContext — SHALL carry per-iteration context for fragment rendering

`FragmentContext` SHALL 包含 `session_id`、`agent_id`、`iteration`、`tool_count`、`token_usage` 等上下文信息。
- **R5.2**: `FragmentContext` 由主循环在每次 LLM 调用前构建

#### Scenario: FragmentContext construction per iteration
- **WHEN** the main loop prepares for an LLM call
- **THEN** a `FragmentContext` is built with the current session_id, agent_id, iteration, tool_count, and token_usage
