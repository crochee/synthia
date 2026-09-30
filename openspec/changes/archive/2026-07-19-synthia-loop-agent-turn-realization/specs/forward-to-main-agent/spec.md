# Spec: forward-to-main-agent

## ADDED Requirements

### Requirement: SteeringPriority Forwarded variant

The system SHALL add a `Forwarded` variant to `SteeringPriority` with priority lower than `User` but higher than `System`.

#### Scenario: Forwarded priority is below User

WHEN a `SteeringMessage` with `SteeringPriority::Forwarded` and a `SteeringMessage` with `SteeringPriority::User` are both in the steering channel
THEN the `User` message SHALL be drained before the `Forwarded` message

### Requirement: ForwardToMainAgent MUST be consumed in main_loop

The system SHALL inject `ForwardToMainAgent { hint }` outcomes into the
parent agent's steering channel as `SteeringMessage { priority: Forwarded,
content: hint }`.

WHEN a sub-agent's hook or extension returns `HookOutcome::ForwardToMainAgent { hint }`
THEN the main_loop SHALL create a `SteeringMessage` with `SteeringPriority::Forwarded` and content from the `hint` field
AND inject it into the parent agent's `SteeringChannel`

#### Scenario: Sub-agent forwards message to parent

WHEN a sub-agent hook returns `ForwardToMainAgent { hint: "review file X" }`
THEN the parent agent's steering channel SHALL receive a `SteeringMessage` with `priority: Forwarded` and `content: "review file X"`
AND the parent agent SHALL process it on the next `drain_steering()` call

#### Scenario: ForwardToMainAgent with empty hint

WHEN a sub-agent hook returns `ForwardToMainAgent { hint: "" }`
THEN the main_loop SHALL still inject a `SteeringMessage` with `content: ""` and `priority: Forwarded`

### Requirement: Forwarded message rate limiting

The system SHALL limit forwarded messages to a maximum of 5 per turn. Messages exceeding this limit SHALL be silently dropped.

#### Scenario: Rate limit enforcement

WHEN 7 `ForwardToMainAgent` outcomes are produced in a single turn
THEN only the first 5 SHALL be injected into the steering channel
AND the remaining 2 SHALL be dropped with a `warn!` log

#### Scenario: Rate limit resets each turn

WHEN a new turn begins
THEN the forwarded message counter SHALL reset to 0
