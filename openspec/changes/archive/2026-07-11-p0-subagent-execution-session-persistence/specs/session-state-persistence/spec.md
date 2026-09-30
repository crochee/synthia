## ADDED Requirements

### Requirement: SessionMetadata SHALL persist loop recovery fields

The SessionMetadata stored in metadata.json SHALL include fields required to correctly resume agent execution after a process restart: end_reason, iteration, cumulative_tokens, and context_token_limit.

#### Scenario: Resume after crash
- **WHEN** an agent session is resumed after a process crash
- **THEN** the system SHALL restore the iteration counter, end reason, cumulative token count, and context token limit from SessionMetadata, allowing the agent to continue from where it left off

#### Scenario: Old session file compatibility
- **WHEN** an existing session metadata.json file without the new fields is loaded
- **THEN** the system SHALL use default values (iteration=0, end_reason=None, cumulative_tokens=0, context_token_limit=None) via serde(default), without error

#### Scenario: Metadata update on tool call
- **WHEN** a tool call completes during agent execution
- **THEN** the system SHALL write updated SessionMetadata to disk, including the current iteration, cumulative token count, and context token limit

### Requirement: SessionInputQueue SHALL persist steering messages

The system SHALL maintain a persistent queue of user steering messages (session_input.jsonl) that survives process restarts, replacing the in-memory tokio::mpsc channel for steering message delivery.

#### Scenario: Steering message persists across restart
- **WHEN** a user sends a steering message during agent execution and the process crashes before the message is consumed
- **THEN** the steering message SHALL be available in the SessionInputQueue after process restart and SHALL be delivered to the agent on the next drain_steering() call

#### Scenario: Drain pending steering
- **WHEN** drain_steering() is called at the start of an agent loop iteration
- **THEN** the system SHALL read all un-promoted SessionInput entries for the current session from session_input.jsonl and return them as steering messages

#### Scenario: Promote consumed input
- **WHEN** a steering message has been processed by the agent
- **THEN** the system SHALL mark the SessionInput entry as promoted, preventing it from being re-delivered on subsequent drain_steering() calls

#### Scenario: Queue delivery for new turns
- **WHEN** a user sends a new message (not a steering message) to a session
- **THEN** the system SHALL store it in the SessionInputQueue with delivery type "queue", to be consumed when the agent starts a new turn

### Requirement: Agent resume SHALL restore from SessionMetadata

The agent resume() method SHALL read SessionMetadata to restore the iteration counter and end reason, in addition to the existing message history restoration from JSONL and checkpoints.

#### Scenario: Resume with iteration state
- **WHEN** resume() is called for a session that was previously at iteration 15
- **THEN** the LoopContext SHALL be initialized with iteration=15, not 0

#### Scenario: Resume with end reason
- **WHEN** resume() is called for a session that was previously terminated with MaxIterationsReached
- **THEN** the LoopContext SHALL be initialized with end_reason=Some(MaxIterationsReached), and the agent SHALL not re-execute the already-completed session