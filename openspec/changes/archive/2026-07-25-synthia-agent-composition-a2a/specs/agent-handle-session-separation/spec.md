# Spec: agent-handle-session-separation

## ADDED Requirements

### Requirement: AgentHandle struct
`AgentHandle` SHALL be a stateless inference handle reusable across N `AgentSession` instances.
- It SHALL hold: `id`, `config`, `provider`, `tool_registry`, `hook_registry`, `context_assembler`, `model_router`, `interceptor_chain`, `a2a_card`
- It SHALL NOT hold: `session`, `loop_state`, `history`, `token_budget`, `compaction_state`
- It SHALL implement `Clone` (via shared `Arc` fields)
- It SHALL implement `RegistryItem` (`name = id`, `description = config.system_prompt`)

#### Scenario: handle is stateless and cloneable
- **WHEN** an `AgentHandle` is cloned and used across multiple sessions
- **THEN** each session operates independently and the handle holds no mutable session state

### Requirement: AgentSession struct
`AgentSession` SHALL be a private session state, independent per run.
- It SHALL hold: `id`, `agent_id`, `history`, `token_budget`, `loop_state`, `compaction_state`
- `agent_id` SHALL reference the owning `AgentHandle`
- It SHALL provide `push_message()`, `get_history()`, and `compact()` methods

#### Scenario: session holds per-run state
- **WHEN** two runs are executed with the same `AgentHandle` but different `AgentSession` instances
- **THEN** each session maintains its own independent history and state

### Requirement: AgentInstance deprecation
`AgentInstance` SHALL be preserved as a type alias `type AgentInstance = AgentHandle` for Phase 1 transition, and SHALL be removed in Phase 6. All existing `AgentInstance` usage points SHALL be migrated to `AgentHandle + AgentSession`.

#### Scenario: agent instance is type alias
- **WHEN** existing code references `AgentInstance`
- **THEN** it resolves to `AgentHandle` via the type alias and compiles without error

### Requirement: AgentRunConfig simplification
`AgentRunConfig` SHALL NOT duplicate `tool_registry`, `hook_registry`, or `session_store`. These SHALL be obtained from `AgentHandle`. `AgentRunConfig` SHALL retain only runtime parameters (`session_id`, `user_id`, `cancel_token`, etc.).

#### Scenario: run config does not duplicate handle fields
- **WHEN** `AgentRunConfig` is constructed for a run
- **THEN** it contains only runtime parameters and references `tool_registry` / `hook_registry` / `session_store` from `AgentHandle`
