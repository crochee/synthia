# Spec: extension-registry

## ADDED Requirements

### Requirement: ExtensionRegistry 统一扩展总线 — SHALL aggregate five extension dimensions

ExtensionRegistry SHALL 作为五种扩展维度的协调器，提供统一的生命周期管理。

- **R1.1**: ExtensionRegistry 包含五个子 Registry：`tool_registry: ToolRegistry`、`fragment_registry: FragmentRegistry`、`interceptor_chain: InterceptorChain`、`skill_registry: SkillRegistry`、`plugin_registry: PluginRegistry`
- **R1.2**: ExtensionRegistry 提供 `shutdown()` 方法，按依赖逆序关闭所有子 Registry
- **R1.3**: Plugin 加载时，ExtensionRegistry 协调跨维度注册（Plugin 可能同时注册 tools + skills + fragments）
- **R1.4**: ExtensionRegistry 提供 `health_check()` 方法，返回所有子 Registry 的健康状态
- **R1.5**: ExtensionRegistry 是 `Send + Sync`，支持跨线程共享

#### Scenario: coordinated plugin loading
- **WHEN** a plugin that provides tools, skills, and fragments is loaded via ExtensionRegistry
- **THEN** all three dimensions are registered atomically and `health_check()` reflects the updated state

### Requirement: Agent 结构体瘦身 — SHALL reduce Agent to four core fields plus ExtensionRegistry

Agent SHALL 从 17 个字段瘦身到 4 核心 + 1 ExtensionRegistry。

- **R2.1**: Agent 只保留 `config: AgentConfig`、`provider: Arc<dyn ModelProvider>`、`session_manager: SessionManager`、`extensions: ExtensionRegistry` 四个核心字段
- **R2.2**: 旧字段通过 `impl Agent` 的方法保持访问，标记 `#[deprecated]`，引导迁移到 `self.extensions.xxx()`
- **R2.3**: `provider_registry` 和 `tool_registry` 迁移到 ExtensionRegistry 内部
- **R2.4**: `hook_registry`、`command_registry` 迁移到 InterceptorChain 和 ToolRegistry
- **R2.5**: `context_assembler` 迁移到 FragmentRegistry
- **R2.6**: `mcp_manager` 迁移到 ToolRegistry（作为 ToolProvider）
- **R2.7**: `approval_service`、`sandbox_manager` 迁移到 InterceptorChain（作为 Interceptor）
- **R2.8**: `steering_channel`、`config_watcher`、`memory_event_sender` 迁移到 InterceptorChain

#### Scenario: deprecated field access still works
- **WHEN** code accesses a deprecated Agent field via the old accessor method
- **THEN** the call delegates to `self.extensions.xxx()` and produces the same result with a deprecation warning

### Requirement: Agent::run_stream 兼容 — SHALL route run_stream through ExtensionRegistry

`run_stream` SHALL 从 `AgentRunConfig` 中获取 ExtensionRegistry 引用而非直接访问字段。
- **R3.2**: `ensure_tool_orchestrator` 通过 ExtensionRegistry 访问 tool_registry 和 approval_service

#### Scenario: run_stream uses ExtensionRegistry
- **WHEN** `Agent::run_stream` executes
- **THEN** it obtains tool_registry and approval_service through ExtensionRegistry instead of direct field access
