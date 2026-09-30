## ADDED Requirements

### Requirement: Unified Tool Trait Contract
Every LLM-invokable capability SHALL implement a single `Tool` trait with exactly 3 methods: `name(&self) -> &str`, `execute(&self, input, ctx) -> Result<ToolOutput, ToolError>`, and `descriptor(&self) -> &ToolDescriptor`. The trait SHALL be `#[async_trait]` with `Send + Sync` bounds.

#### Scenario: Tool implements unified trait
- **WHEN** a new tool is defined implementing the `Tool` trait
- **THEN** it SHALL provide `name()`, `execute()`, and `descriptor()` without implementing any other trait

#### Scenario: Legacy Tool trait deprecated
- **WHEN** code references the legacy `Tool` trait with 11 methods
- **THEN** the compiler SHALL emit a deprecation warning and the legacy trait SHALL remain available for 1 release cycle

---

### Requirement: Tool Provider Registration Contract
Every source of tools SHALL implement `ToolProvider` trait with `id()`, `list_tools()`, `get_tool()`, `on_tool_event()`, `before_execute()`, and `after_execute()`. `pre_check` SHALL NOT exist on `ToolProvider` — permission evaluation is centralized in `PermissionService`.

#### Scenario: Provider registers tools
- **WHEN** a `ToolProvider` is registered with the `ToolRegistry`
- **THEN** all tools from `list_tools()` SHALL be available for materialization

#### Scenario: Provider pre_check removed
- **WHEN** a tool provider previously overrode `pre_check` for permission gating
- **THEN** it MUST register a `PermissionRule` via `PermissionService::add_rule` at init time instead

---

### Requirement: Tool Registry Materialization
The `ToolRegistry` SHALL support `materialize(session_id, permissions)` which captures an immutable snapshot of tool identities for stale detection. `resolve(mat, name)` SHALL return `Err(Stale)` when the tool's `ToolGeneration` has changed since snapshot.

#### Scenario: Stale detection on plugin reload
- **WHEN** a materialization snapshot is taken at step T, and a plugin reloads at step T+1
- **THEN** `resolve()` SHALL return `Stale` error for tools from the reloaded provider

#### Scenario: Fresh snapshot resolves successfully
- **WHEN** a materialization snapshot is taken and no registration changes occur
- **THEN** `resolve()` SHALL return the tool `Arc<dyn Tool>` successfully

---

### Requirement: Builtin Tool Provider Dual Map
`BuiltinToolProvider` SHALL maintain two maps: `applications` (built-in, immutable) and `local` (runtime additions, mutable). Core tool names SHALL be refused re-registration. Local tools MAY be added/removed freely.

#### Scenario: Core tool re-registration refused
- **WHEN** a tool with the same name as an existing `Core` provenance tool is registered
- **THEN** the registry SHALL return `RegistrationError::CoreNameTaken`

#### Scenario: Local tool override allowed
- **WHEN** a local tool is added with the same name as a core tool
- **THEN** the local tool SHALL shadow the core tool at materialization time (LIFO)

---

### Requirement: Output Bound with Async Spill
`ToolRegistry::bound_output` SHALL be `async fn` and SHALL truncate tool output to per-call limits (50 KiB / 2000 lines by default). Overflow SHALL spill to managed files via `tokio::fs` without blocking worker threads. Retention SHALL default to 7 days with 1-hour cleanup interval.

#### Scenario: Output exceeds per-call limit
- **WHEN** a tool output exceeds 50 KiB or 2000 lines
- **THEN** the output SHALL be truncated (head/tail preserved) and excess SHALL spill to a managed file

#### Scenario: Managed file cleanup
- **WHEN** a managed file is older than 7 days
- **THEN** the background cleanup task SHALL delete it

---

### Requirement: Output Sanitization Policy
`OutputBound` SHALL apply a `SanitizationPolicy` before returning output to LLM. Default SHALL be `StripControlChars`. The policy SHALL strip ASCII control characters (except \\n, \\r, \\t) and wrap output in isolation tags when `WrapUntrusted` is selected.

#### Scenario: Default sanitization strips control chars
- **WHEN** tool output contains NUL bytes or escape sequences
- **THEN** `StripControlChars` SHALL remove them before LLM delivery

---

### Requirement: Tool Descriptor Provenance Visibility
`ToolDescriptor` SHALL include `prompt_visible_provenance: bool` (default true for plugin tools). When true, plugin tools SHALL appear to the LLM as `plugin:<id>:<tool>`. The descriptor SHALL also include `is_hidden: bool` (default false) and `is_user_invocable: bool` (default true).

#### Scenario: Plugin tool namespace visible to LLM
- **WHEN** a plugin tool with `prompt_visible_provenance: true` is materialized
- **THEN** the LLM SHALL see the tool name as `plugin:<plugin_id>:<raw_name>`

#### Scenario: Hidden tool not in help listing
- **WHEN** a tool has `is_hidden: true`
- **THEN** it SHALL NOT appear in help listings but SHALL remain callable by the LLM
