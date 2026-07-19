# Spec: op-session-adapter

## ADDED Requirements

### Requirement: Op → SessionOp mapping function

The system SHALL provide `fn op_to_session_op(op: Op) -> Result<SessionOp, ProtocolError>` in `synthia-protocol`.

#### Scenario: UserInput maps to Prompt

WHEN `op_to_session_op(Op::UserInput { content: "hello" })` is called
THEN the result SHALL be `Ok(SessionOp::Prompt { content: "hello" })`

#### Scenario: Interrupt maps to Cancel

WHEN `op_to_session_op(Op::Interrupt)` is called
THEN the result SHALL be `Ok(SessionOp::Cancel)`

#### Scenario: ApprovalResponse maps to ApprovalDecision

WHEN `op_to_session_op(Op::ApprovalResponse { approval_id, decision })` is called
THEN the result SHALL be `Ok(SessionOp::ApprovalDecision { approval_id, decision })`

#### Scenario: Unknown Op returns error

WHEN `op_to_session_op` is called with an Op that has no SessionOp equivalent
THEN the result SHALL be `Err(ProtocolError::UnsupportedOp { op: "..." })`

### Requirement: POST /submission routes to SessionController

WHEN `POST /submission` receives a `SubmissionEnvelope`
THEN the handler SHALL call `op_to_session_op(envelope.op)`
AND submit the resulting `SessionOp` to `SessionController::submit()`

#### Scenario: Valid submission routes successfully

WHEN `POST /submission` receives a valid `Submission { op: Op::UserInput { content: "test" } }`
THEN the handler SHALL return 202 Accepted
AND `SessionController::submit()` SHALL be called with `SessionOp::Prompt { content: "test" }`

#### Scenario: Invalid submission returns error

WHEN `POST /submission` receives an unsupported `Op`
THEN the handler SHALL return 400 Bad Request with error details
