# scope-isolation Specification

## Purpose
TBD - created by archiving change tool-abstraction-and-extensibility. Update Purpose after archive.
## Requirements
### Requirement: ToolRegistry SHALL support 4 isolation scopes

The synthia tool registry SHALL support 4 isolation scopes: `Global`, `Session`, `User`, `Project`. Each tool registration SHALL be tagged with a scope, and `materialize()` SHALL resolve conflicts by priority.

#### Scenario: ToolScope enum
- **WHEN** a tool is registered
- **THEN** the registration SHALL be tagged with one of `ToolScope::{Global, Session, User, Project}`
- **AND** the scope SHALL be immutable for the registration lifetime

#### Scenario: Materialize priority is Project > User > Session > Global
- **WHEN** `materialize(session_id: &str)` is called
- **THEN** the resolved tool set SHALL prioritize Project-scope tools over User-scope, which take precedence over Session-scope, which take precedence over Global-scope
- **AND** within a single scope, last-wins semantics SHALL apply (most recent registration wins)

#### Scenario: Same tool name in different scopes
- **WHEN** the same tool name (e.g., `read`) is registered in both `Project` and `Global` scopes
- **THEN** the materialized set SHALL contain only the `Project`-scoped version
- **AND** the orchestrator SHALL log a P9 event: `tool_shadowed { name, shadowed_scope: "Global", shadowing_scope: "Project" }`

### Requirement: Project scope tools SHALL be loaded from `.synthia/tools.toml`

The Project scope SHALL be populated from the file `.synthia/tools.toml` in the workspace root.

#### Scenario: Loading project tools on session start
- **WHEN** a new session starts
- **THEN** the registry SHALL read `.synthia/tools.toml` from the workspace root
- **AND** each tool entry SHALL be loaded as an `Arc<dyn Tool>` and registered in `Project` scope

#### Scenario: Missing project tools file
- **WHEN** `.synthia/tools.toml` does not exist
- **THEN** the registry SHALL treat the Project scope as empty
- **AND** no error SHALL be raised
- **AND** a P9 event `project_tools_loaded { count: 0 }` SHALL be logged

#### Scenario: Invalid project tools file
- **WHEN** `.synthia/tools.toml` is malformed
- **THEN** the registry SHALL log an error and skip the malformed entries
- **AND** the session SHALL proceed with valid entries only
- **AND** a P9 event `project_tools_parse_error { path, error }` SHALL be logged

### Requirement: User scope tools SHALL be loaded from user config

The User scope SHALL be populated from `~/.config/synthia/tools.toml` (or platform equivalent).

#### Scenario: Loading user tools on session start
- **WHEN** a new session starts
- **THEN** the registry SHALL read user-level config
- **AND** tools SHALL be registered in `User` scope

### Requirement: Global scope tools SHALL be the built-in defaults

The Global scope SHALL contain all built-in tools (Read, Write, Glob, Grep, MultiEdit, ApplyPatch, WebFetch) plus any tool registered via `ToolRegistry::register_global()`.

#### Scenario: Built-in tools are always in Global
- **WHEN** a session starts with no overrides
- **THEN** all 7 built-in tools SHALL be present in the materialized set via `Global` scope

### Requirement: Session scope tools SHALL be created via `register_scoped()`

The Session scope SHALL be populated by calls to `ToolRegistry::register_scoped(tools, token)`. Tools in Session scope SHALL auto-deregister when the session ends (via `ScopeGuard` RAII).

#### Scenario: Session-scoped tool auto-cleanup
- **WHEN** a `ScopeGuard` is dropped (session end or panic)
- **THEN** all tools registered with the same `token` SHALL be removed from `Session` scope
- **AND** the next `materialize()` call SHALL NOT include them

#### Scenario: Per-session tool isolation
- **WHEN** two sessions each register a tool named `custom` via `register_scoped`
- **THEN** each session's `materialize()` SHALL return its own `custom` tool
- **AND** the two registrations SHALL NOT conflict

### Requirement: Materialization SHALL emit P9 observability events

Every `materialize()` call SHALL emit a P9 event containing the resolved tool set, source scopes, and a content hash for cache invalidation.

#### Scenario: materialize event payload
- **WHEN** `materialize(session_id)` is called
- **THEN** a P9 event SHALL be emitted with:
  ```json
  {
    "event": "tools.materialized",
    "session_id": "...",
    "scope_distribution": { "Project": 2, "User": 1, "Session": 0, "Global": 7 },
    "tool_count": 10,
    "tools": ["read", "write", "glob", "grep", "multi_edit", "apply_patch", "web_fetch", "project_tool_a", "project_tool_b", "user_tool_x"],
    "shadowed": [],
    "content_hash": "a3f7c2..."
  }
  ```

#### Scenario: P1 prefix cache invalidation
- **WHEN** the materialized tool set changes (e.g., a new project tool is added)
- **THEN** `content_hash` SHALL change
- **AND** the orchestrator SHALL invalidate the P1 prefix cache for the session
- **AND** the LLM SHALL re-receive the new tool definitions on the next call

