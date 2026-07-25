# workflow-pattern Specification

## Purpose
TBD - created by archiving change synthia-agent-composition-a2a. Update Purpose after archive.
## Requirements
### Requirement: Workflow struct
`Workflow` SHALL compose multiple `AgentHandle` instances for sequential execution:
- `stages: Vec<Arc<AgentHandle>>` — ordered stage list

#### Scenario: workflow holds ordered stages
- **WHEN** a `Workflow` is created with a list of agent handles
- **THEN** it holds the stages in the given order for sequential execution

### Requirement: Workflow.run
`Workflow.run(input)` SHALL:
1. Set `current = input`
2. For each `stage` in `stages`:
   a. `tool = agent_as_tool(stage)`
   b. `output = tool.call(current)`
   c. `current = output.text()`
3. Return `Ok(current)`

#### Scenario: sequential stage execution
- **WHEN** `workflow.run(input)` is called with 3 stages
- **THEN** each stage processes the previous stage's output in order and the final result is returned

### Requirement: Workflow supports mixed agents
The `stages` list SHALL support mixing local `AgentHandle` instances and remote agents (via `SendMessage` Tool).

#### Scenario: mixed local and remote stages
- **WHEN** a `Workflow` is configured with both local and remote agent stages
- **THEN** the workflow executes all stages sequentially regardless of whether each stage is local or remote

