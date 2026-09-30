## ADDED Requirements

### Requirement: self_reflect SHALL be exposed as an LLM-callable tool

The Guardian's `self_reflect` capability MUST be exposed as a tool registered with the tool orchestrator. The tool description MUST explain that it triggers an independent context review and returns structured feedback. The LLM MUST be able to call this tool at any time during a turn, subject to normal permission policies.

#### Scenario: LLM calls self_reflect tool

- **WHEN** the LLM emits a `tool_use` for `self_reflect`
- **THEN** the tool orchestrator dispatches the call to the Guardian
- **AND** the Guardian performs an independent context review
- **AND** the structured feedback is returned as `tool_result`

#### Scenario: Tool registered with descriptive name

- **WHEN** the tool registry is queried
- **THEN** a tool named `self_reflect` exists
- **AND** its description mentions "independent context review" and "structured feedback"
- **AND** the tool schema declares no required parameters

---

### Requirement: self_reflect tool SHALL retain every-5-rounds fallback

In addition to LLM-initiated calls, the runtime MUST automatically trigger `self_reflect` every 5 iterations of the main loop, regardless of whether the LLM has called the tool. This fallback ensures self-reflection occurs even if the LLM neglects to call the tool (per P6 "do not trust LLM" principle).

#### Scenario: Auto-trigger at iteration 5

- **WHEN** the main loop reaches iteration 5 and the LLM has not called `self_reflect`
- **THEN** the runtime injects a synthetic `self_reflect` tool call
- **AND** the Guardian performs the review
- **AND** the result is added to context as if the LLM had called it

#### Scenario: LLM call resets auto-trigger counter

- **WHEN** the LLM calls `self_reflect` at iteration 3
- **THEN** the auto-trigger counter is reset
- **AND** the next auto-trigger is scheduled for iteration 8 (3 + 5)
- **AND** no duplicate auto-trigger occurs at iteration 5

#### Scenario: Both LLM call and auto-trigger at same iteration

- **WHEN** the LLM calls `self_reflect` at iteration 5
- **AND** the auto-trigger is also scheduled for iteration 5
- **THEN** only the LLM-initiated call runs
- **AND** no duplicate review is performed
- **AND** the counter resets to schedule the next auto-trigger at iteration 10
