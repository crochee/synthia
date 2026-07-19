## ADDED Requirements

### Requirement: Self-reflection shall be executed after main loop completion

Self-reflection SHALL be triggered after the main loop ends with a successful completion (end_reason = Completed and iteration > 0).

#### Scenario: Self-reflection trigger condition
- **WHEN** Main loop exits with end_reason = Completed
- **AND** iteration > 0
- **THEN** Self-reflection SHALL be executed

#### Scenario: Self-reflection skipped on early exit
- **WHEN** Main loop exits with Cancelled, LoopDetected, or Error
- **THEN** Self-reflection SHALL be skipped
- **AND** No reflection artifact SHALL be generated

---

### Requirement: Self-reflection shall generate structured reflection data

The reflection SHALL produce a structured output containing summary, issues, and suggestions.

#### Scenario: Reflection generation
- **WHEN** Self-reflection is triggered
- **THEN** It SHALL analyze the complete message history
- **AND** Generate a Reflection with fields: iteration, summary (String), issues (Vec<String>), suggestions (Vec<String>)

#### Scenario: Reflection prompt
- **WHEN** Reflection is generated
- **THEN** It SHALL use a prompt instructing the LLM to analyze execution patterns
- **AND** Return structured JSON with summary/issues/suggestions

---

### Requirement: Self-reflection result shall be stored in HotMemory

The generated reflection SHALL be stored in HotMemory with a specific key pattern for later retrieval.

#### Scenario: Reflection storage key format
- **WHEN** Reflection is stored
- **THEN** The key SHALL be `reflection/{session_id}/{iteration}`
- **AND** Value SHALL be the serialized Reflection struct

#### Scenario: Reflection storage integration
- **WHEN** Reflection is generated and stored
- **THEN** It SHALL use the memory_event_sender to send MemoryEvent::reflection_stored
- **AND** HotMemory SHALL be updated with the reflection data

---

### Requirement: Reflection shall be accessible for future sessions

Reflections stored in HotMemory SHALL be retrievable by subsequent sessions for context injection.

#### Scenario: Reflection retrieval
- **WHEN** A new session needs context
- **THEN** It SHALL be able to retrieve past reflections from HotMemory
- **AND** Include them in the context_assembler for prompt construction

---

### Requirement: Self-reflection step shall emit events

Self-reflection execution SHALL emit events for observability.

#### Scenario: Self-reflection progress events
- **WHEN** Self-reflection is in progress
- **THEN** AgentEvent::Thinking with "Performing self-reflection..." SHALL be emitted
- **AND** Upon completion, AgentEvent::SelfReflection SHALL be emitted with the reflection data
- **AND** On failure, AgentEvent::Warning with error message SHALL be emitted