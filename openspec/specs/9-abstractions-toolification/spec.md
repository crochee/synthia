# 9-abstractions-toolification Specification

## Purpose
TBD - created by archiving change tool-abstraction-and-extensibility. Update Purpose after archive.
## Requirements
### Requirement: 9 existing non-Tool abstractions SHALL be migrated to Tool trait

The following 9 abstractions SHALL be migrated to the `synthia_tool::Tool` trait and registered in the `ToolRegistry`, so the LLM can discover and call them via the standard tool flow.

#### Scenario: Migration list
- **WHEN** the migration is complete
- **THEN** all 9 abstractions SHALL be discoverable by the LLM via `tool_choice` enumeration:
  1. `synthia_context::compact_context_tool`
  2. `synthia_skill::implicit_tools::load_skill`
  3. `synthia_agent::subagent::AgentTool`
  4. `synthia_guardian::SELF_REFLECT_TOOL_NAME`
  5. `synthia_tool_bash::MonitorTool`
  6. Each `synthia_mcp` server (registered as `McpTool { server, name }`)
  7. `synthia_plugin::HookRunner` external subprocess hooks (registered as `ExternalHookTool`)
  8. `synthia_skill::usage` tracker (registered as `QuerySkillUsageTool`)
  9. Plugin CLI entries (registered via manifest with `kind: Tool`)

### Requirement: compact_context_tool SHALL be registered as a standard Tool

The `synthia_context::compact_context_tool` SHALL be registered in the `ToolRegistry` and called via the standard orchestrator flow.

#### Scenario: main_loop uses standard call path
- **WHEN** the main loop needs to compact context
- **THEN** it SHALL call `registry.run_with_context("compact_context_tool", input)`
- **AND** the `main_loop.rs` SHALL NOT contain a special-case facade for compact_context_tool
- **AND** permission checks, doom loop detection, and execution mode routing SHALL apply to compact_context_tool

#### Scenario: P3 lazy load
- **WHEN** compact_context_tool is invoked
- **THEN** the tool SHALL receive only the messages within the compactor's input range
- **AND** other messages SHALL NOT be loaded (P3 lazy)

### Requirement: load_skill SHALL be a Tool with is_hidden=true and is_user_invocable=true

The `synthia_skill::implicit_tools::load_skill` SHALL be migrated to the `Tool` trait with `is_hidden=true` and `is_user_invocable=true`.

#### Scenario: load_skill visible to LLM
- **WHEN** the materialized tool set is sent to the LLM
- **THEN** `load_skill` SHALL be present in the `tools` field
- **AND** `is_hidden=true` SHALL mean it does NOT appear in user-facing help

#### Scenario: load_skill triggers skill activation
- **WHEN** the LLM calls `load_skill(skill_name)`
- **THEN** the tool SHALL activate the named skill via `SkillRegistry::activate_skill`
- **AND** return a `ToolOutput` containing the skill's instructions

### Requirement: subagent::AgentTool SHALL use ToolRegistry

The `synthia_agent::subagent::AgentTool` SHALL be migrated to use the standard `ToolRegistry::register_scoped()` mechanism, removing the dual `agent_tools.rs` path.

#### Scenario: subagent registered as session-scoped Tool
- **WHEN** a subagent is spawned
- **THEN** the subagent's tool SHALL be registered in `Session` scope with a unique `token`
- **AND** when the subagent session ends, the `ScopeGuard` SHALL auto-deregister the tool

#### Scenario: main_loop routes via registry
- **WHEN** the main loop encounters a subagent call
- **THEN** it SHALL call `registry.run_with_context("subagent", input)`
- **AND** the call SHALL be subject to standard permission checks
- **AND** the `_subagent_session_factory` field in `main_loop.rs:124-162` SHALL be replaced by registry-based injection

### Requirement: guardian self_reflect tool SHALL self-identify

The `synthia_guardian::SELF_REFLECT_TOOL_NAME` SHALL be a `const` field on a Tool impl, removing string literal comparison in the main loop.

#### Scenario: self_reflect lookup via Tool trait
- **WHEN** the main loop checks for self_reflect tool
- **THEN** it SHALL iterate `ToolRegistry` and find the tool whose `name()` returns `SELF_REFLECT_TOOL_NAME`
- **AND** the `main_loop.rs:543-546` string literal comparison SHALL be replaced

### Requirement: MonitorTool SHALL be migrated to Tool trait

The `synthia_tool_bash::MonitorTool` SHALL be migrated from a static style to a `Tool` trait implementation.

#### Scenario: MonitorTool registered
- **WHEN** the bash tool family is initialized
- **THEN** `MonitorTool` SHALL be registered in the `ToolRegistry`
- **AND** it SHALL be subject to permission checks and doom loop detection

### Requirement: Each MCP server SHALL be registered as McpTool

Each `McpProxy` server SHALL have its tools registered as `McpTool { server: Arc<McpProxy>, name: String }` instances in the `ToolRegistry`.

#### Scenario: McpTool provenance
- **WHEN** an `McpTool` is registered
- **THEN** the registration SHALL include `ToolPluginProvenance { source: "mcp", server: "..." }`
- **AND** the provenance SHALL be visible in the P9 event log and OTel span

#### Scenario: McpTool delegates to server
- **WHEN** the LLM calls an `McpTool`
- **THEN** the call SHALL be delegated to the underlying `McpProxy::invoke_tool(name, args)`
- **AND** the response SHALL be wrapped in a `ToolOutput`

### Requirement: HookRunner external subprocess SHALL be ExternalHookTool

The `synthia_plugin::HookRunner` external subprocess hooks SHALL be migrated to `ExternalHookTool` instances registered in the `ToolRegistry`.

#### Scenario: ExternalHookTool token budget
- **WHEN** an `ExternalHookTool` is registered
- **THEN** it SHALL have an associated `token_budget: u32`
- **AND** the orchestrator SHALL count the subprocess output bytes against the budget
- **AND** when the budget is exceeded, the tool SHALL return `ToolError::Truncated`

#### Scenario: ExternalHookTool permission check
- **WHEN** an `ExternalHookTool` is invoked
- **THEN** the standard permission check SHALL apply
- **AND** the user SHALL be able to allow/deny the subprocess command before execution

### Requirement: Skill usage tracker SHALL be queryable via Tool

The `synthia_skill::usage` tracker SHALL be exposed as `QuerySkillUsageTool`, allowing the LLM to query skill usage statistics.

#### Scenario: QuerySkillUsageTool returns JSON
- **WHEN** the LLM calls `QuerySkillUsageTool(skill_name: Option<String>)`
- **THEN** the tool SHALL return a `ToolOutput` with JSON containing usage statistics
- **AND** the response SHALL include: `call_count`, `last_used`, `success_rate`, `average_tokens`

#### Scenario: P1 hash includes usage queries
- **WHEN** the LLM calls `QuerySkillUsageTool` and the result is added to the message history
- **THEN** the result SHALL participate in the P1 prefix hash (per design decision D5)

### Requirement: Plugin CLI entries SHALL be Tools

Plugin manifest entries with `kind: Tool` SHALL be registered as `Tool` instances in the `ToolRegistry`.

#### Scenario: Plugin Tool registration scope
- **WHEN** a plugin loads with `kind: Tool` hooks
- **THEN** the tools SHALL be registered in `Global` scope by default
- **AND** a plugin can request `User` scope via manifest metadata

#### Scenario: Plugin Tool subject to all orchestrator features
- **WHEN** a plugin Tool is invoked
- **THEN** the orchestrator SHALL apply: permission check, doom loop detection, execution mode routing, cancellation, P1 prefix caching
- **AND** the plugin Tool SHALL behave identically to a built-in tool

