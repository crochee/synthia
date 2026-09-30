## ADDED Requirements

### Requirement: Steering channel SHALL be drained at each iteration start

At the beginning of each iteration in `run_with_steps`, the system SHALL call `steering_channel.try_recv()` and if a message is available, the system SHALL yield `AgentEvent::SteeringReceived { session_id, message }` and inject the steering content as a `Message::User` at the front of `ctx.messages`.

#### Scenario: Steering message received at iteration start
- **WHEN** a steering message is pending on the channel at the start of iteration 5
- **THEN** the system SHALL yield `AgentEvent::SteeringReceived` with the steering message
- **AND** inject the steering content as a user message at the front of context

### Requirement: Steering messages SHALL respect priority ordering

The `MpscSteeringChannel` already orders messages by priority. The wiring implementation SHALL NOT alter this ordering.

#### Scenario: High-priority steering delivered first
- **WHEN** two steering messages are pending (priority 1 and priority 5)
- **THEN** the message with priority 1 SHALL be delivered first

### Requirement: Empty steering channel SHALL not block iteration

Calling `try_recv()` on an empty channel SHALL return immediately without blocking, allowing the iteration to proceed normally.

#### Scenario: No steering message pending
- **WHEN** `steering_channel.try_recv()` returns `None`
- **THEN** the iteration SHALL continue without waiting or yielding any steering event