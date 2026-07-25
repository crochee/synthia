# tool-namespace-scope Specification

## Purpose
TBD - created by archiving change synthia-registry-first-extension-architecture. Update Purpose after archive.
## Requirements
### Requirement: ToolName Namespace 隔离 — SHALL prevent tool name collisions via namespacing

`ToolName` SHALL 支持 `namespace::tool` 格式以防止多源工具名冲突。

- **R1.1**: `ToolName` 结构体包含 `namespace: Option<String>` 和 `name: String`
- **R1.2**: `ToolName::plain(name)` 创建无命名空间的工具名
- **R1.3**: `ToolName::namespaced(ns, name)` 创建带命名空间的工具名
- **R1.4**: `ToolName::full_name()` 返回 `namespace::name` 或 `name`
- **R1.5**: MCP Server 提供的工具自动使用 `mcp__{server_name}` 作为命名空间
- **R1.6**: Plugin 提供的工具自动使用 `plugin__{plugin_name}` 作为命名空间
- **R1.7**: 平面字符串自动转换为 `ToolName::plain()`，现有代码无需修改
- **R1.8**: ToolRegistry 的 HashMap key 从 `String` 改为 `ToolName`
- **R1.9**: `ToolName` 实现 `Display`、`Hash`、`Eq`、`Serialize`、`Deserialize`

#### Scenario: namespaced tool name prevents collision
- **WHEN** two tools with the same local name are registered from different MCP servers
- **THEN** their full names differ (`mcp__server1::tool` vs `mcp__server2::tool`) and both coexist in ToolRegistry

### Requirement: RegistrationScope 生命周期 — SHALL auto-unregister tools on scope drop

工具注册 SHALL 带 Scope，Scope 结束时自动反注册。

- **R2.1**: `RegistrationScope` 持有 `RegistrationToken` 和 `Weak<ToolRegistry>`
- **R2.2**: `impl Drop for RegistrationScope` 调用 `ToolRegistry::unregister_by_token()`
- **R2.3**: `ToolRegistry::register_scoped()` 返回 `RegistrationScope`
- **R2.4**: `ToolRegistry::register_scoped_with_namespace()` 支持带命名空间的 scoped 注册
- **R2.5**: Session 创建时获得 RegistrationScope，Session 结束时 Scope Drop 自动清理会话级工具
- **R2.6**: `ToolRegistry::unregister()` 实际实现（当前是 TODO）

#### Scenario: scoped registration auto-cleanup on drop
- **WHEN** a RegistrationScope goes out of scope (e.g., session ends)
- **THEN** all tools registered under that scope are automatically unregistered from ToolRegistry

### Requirement: ToolExposure 延迟加载 — SHALL defer tool definition loading until first call

`ToolExposure` SHALL 支持延迟加载，工具首次调用时才加载完整定义。

- **R3.1**: `ToolExposure` 枚举包含 `Direct`、`Deferred`、`Hidden` 三个变体
- **R3.2**: `Deferred` 工具仅暴露名称和简要描述（无参数 schema）
- **R3.3**: `Deferred` 工具首次调用时，通过 `ToolProvider::get_tool()` 加载完整定义
- **R3.4**: `Hidden` 工具不暴露给 LLM，仅内部使用
- **R3.5**: `ToolDescriptor` 增加 `exposure: ToolExposure` 字段
- **R3.6**: `ToolRegistry::materialize()` 根据 exposure 决定是否包含完整定义
- **R3.7**: 提供 `tool_search` 内建工具，支持 BM25 搜索发现 Deferred 工具

#### Scenario: deferred tool materialization on first call
- **WHEN** a Deferred tool is called for the first time
- **THEN** its full definition is loaded via `ToolProvider::get_tool()` and subsequent calls use the cached definition

