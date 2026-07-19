## ADDED Requirements

### Requirement: Plugin hooks SHALL be unified under AgentHook trait

The `synthia_plugin::HookRunner` and `synthia_hook::AgentHook` SHALL be unified into a single `AgentHook` trait. `HookRunner` SHALL be retained as an internal implementation detail, with `PluginHookAdapter` bridging external plugins.

#### Scenario: PluginHookAdapter implements AgentHook
- **WHEN** a plugin declares a hook in its manifest
- **THEN** the orchestrator SHALL create a `PluginHookAdapter` wrapping the plugin's `HookRunner`
- **AND** the adapter SHALL implement `AgentHook` (7 lifecycle methods)
- **AND** calling an AgentHook method SHALL delegate to the underlying `HookRunner::fire(event, payload)`

#### Scenario: 7 AgentHook lifecycle methods map to plugin events
- **WHEN** the AgentHook method `on_before_llm` is called
- **THEN** the adapter SHALL fire the plugin event `chat.message` (per opencode `packages/plugin/src/index.ts:234-243`)
- **AND** the payload SHALL be transformed from `AgentContext` to the plugin's expected `EventPayload` format
- **AND** the response SHALL be transformed back to `Result<(), HookError>`

#### Scenario: mapping table (AgentHook method -> Plugin event)
- **WHEN** the orchestrator fires an AgentHook method
- **THEN** the adapter SHALL map as follows:
  - `on_before_llm` -> `chat.message` (input) / `chat.params` (modify)
  - `on_after_llm` -> `chat.message` (output)
  - `on_before_tool(name, args)` -> `tool.execute.before` (modify args)
  - `on_after_tool(name, output)` -> `tool.execute.after` (modify output)
  - `on_error` -> `chat.message` (error)
  - `on_iteration_end` -> (no direct plugin event; emit P9 event)
  - `on_complete` -> (no direct plugin event; emit P9 event)

### Requirement: HookRunner SHALL be marked deprecated but remain functional

The `synthia_plugin::HookRunner` public API SHALL be marked `#[deprecated]` but SHALL continue to function. Internal migrations to `PluginHookAdapter` SHALL be staged.

#### Scenario: Deprecated marker
- **WHEN** a developer uses `HookRunner` directly
- **THEN** the compiler SHALL emit a deprecation warning
- **AND** the warning message SHALL recommend `PluginHookAdapter`

#### Scenario: Backward compatibility
- **WHEN** an existing plugin uses `HookRunner` via its public API
- **THEN** the plugin SHALL continue to work without modification
- **AND** the orchestrator SHALL automatically wrap the plugin's HookRunner in a `PluginHookAdapter`

### Requirement: Plugin manifest SHALL support Tool-typed hooks

The `PluginManifest::hooks` field SHALL support declaring a hook as a `Tool` (so plugin authors can register tools that integrate with the orchestrator's permission, doom loop, and execution mode features).

#### Scenario: HookSpec with kind
- **WHEN** a plugin manifest declares a hook
- **THEN** the hook SHALL have a `kind: HookKind` field
- **AND** `HookKind` SHALL be one of `Tool | Agent | Subscription`
- **AND** `kind: Tool` SHALL mean the hook is registered as a `Tool` in the ToolRegistry

#### Scenario: Manifest validation
- **WHEN** `PluginManifest::validate()` is called
- **THEN** it SHALL verify all `kind: Tool` hooks have a `name` and `description`
- **AND** it SHALL verify all `kind: Agent` hooks have a valid `matcher` (regex or glob)
- **AND** invalid manifests SHALL fail to load with a `PluginManifestError`

#### Scenario: Tool-typed hook registration
- **WHEN** a plugin loads
- **THEN** all `kind: Tool` hooks SHALL be registered in the appropriate scope (default: `Global`)
- **AND** the registered Tool SHALL appear in the LLM's `tool_choice` enumeration
- **AND** the registered Tool SHALL be subject to permission checks, doom loop detection, and execution mode routing

### Requirement: FailPolicy for plugin hooks SHALL be FailOpen

For plugin hooks, the default `FailPolicy` SHALL be `FailOpen` (the hook failure does not block the agent). This is per the opencode `packages/plugin/src/index.ts:223` semantics: plugin hooks are advisory, not gates.

#### Scenario: Plugin hook failure does not block
- **WHEN** a plugin hook raises an error during `on_before_tool`
- **THEN** the error SHALL be logged
- **AND** the tool execution SHALL proceed
- **AND** the agent SHALL continue

#### Scenario: Distinction from permission FailClosed
- **WHEN** comparing plugin hooks to permission checks
- **THEN** plugin hooks SHALL default to `FailOpen` (per opencode semantics)
- **AND** permission checks SHALL default to `FailClosed` (per project hard constraint: "Permission policy must default to 'AskUser' (fail-closed) instead of 'Allow' (fail-open)")
- **AND** the two are distinct systems: hooks are advice, permissions are gates
