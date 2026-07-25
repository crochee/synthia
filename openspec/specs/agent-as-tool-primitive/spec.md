# agent-as-tool-primitive Specification

## Purpose
TBD - created by archiving change synthia-agent-composition-a2a. Update Purpose after archive.
## Requirements
### Requirement: agent_as_tool pure function
`pub fn agent_as_tool(handle: Arc<AgentHandle>) -> AgentTool` SHALL be a pure conversion function with no side effects. It SHALL wrap an `AgentHandle` into a `Tool` impl with `name = handle.id` and `description = handle.config.system_prompt`.

#### Scenario: convert handle to tool
- **WHEN** `agent_as_tool(handle)` is called with a valid `AgentHandle`
- **THEN** the returned `AgentTool` has `name` equal to `handle.id` and `description` equal to `handle.config.system_prompt`

### Requirement: AgentTool call semantics
`AgentTool::call()` SHALL create a new `AgentSession`, invoke `handle.run(session, prompt)`, and return a `ToolOutput`. Each call SHALL create an independent session with no shared state.

#### Scenario: each call uses independent session
- **WHEN** `agent_tool.call(prompt)` is invoked twice
- **THEN** each invocation creates a separate `AgentSession` and the sessions share no state

### Requirement: AgentTool parameter schema
The tool parameters SHALL be `{ "prompt": string (required), "context": string (optional) }`. `prompt` is the task description sent to the agent; `context` is additional context.

#### Scenario: parameter schema defined
- **WHEN** the `AgentTool` parameter schema is inspected
- **THEN** it defines a required `prompt` string and an optional `context` string

### Requirement: SubagentManager removal
The system SHALL remove `SubagentManager`. Its responsibilities SHALL be redistributed:
- depth limiting to `ToolOrchestrator` execution policy
- concurrency control to `ToolOrchestrator` (already present)
- subtree cancellation to `CancellationToken` hierarchy
- parent config to `AgentHandle` (already included)
- session registration/unregistration to `AgentSession` self-management
`SlotGuard` SHALL also be removed.

#### Scenario: subagent manager removed
- **WHEN** the codebase is compiled after this change
- **THEN** `SubagentManager` and `SlotGuard` no longer exist and their responsibilities are handled by `ToolOrchestrator`, `CancellationToken`, `AgentHandle`, and `AgentSession`

