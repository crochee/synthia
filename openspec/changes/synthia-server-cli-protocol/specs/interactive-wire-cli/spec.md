# Spec: interactive-wire-cli

## ADDED Requirements

### Requirement: stdin→Submission pipeline

The wire CLI SHALL support sending Submissions from stdin.

#### Scenario: Send UserInput from stdin

WHEN the user types a message in the wire CLI and presses Enter
THEN the CLI SHALL send `Submission { op: Op::UserInput { content: "<input>" } }` to the server
AND display the resulting events

### Requirement: Interrupt from keyboard

The wire CLI SHALL support sending `Op::Interrupt` when the user presses Ctrl+C.

#### Scenario: Ctrl+C sends interrupt

WHEN the user presses Ctrl+C in the wire CLI
THEN the CLI SHALL send `Submission { op: Op::Interrupt }` to the server
AND display "Interrupt sent" confirmation

### Requirement: Approval response from stdin

The wire CLI SHALL support sending `Op::ApprovalResponse` when an approval request is received.

#### Scenario: Approve a tool execution

WHEN the wire CLI receives an `EventMsg::ApprovalRequest`
THEN the CLI SHALL prompt the user with "Approve? [y/n/a]: "
AND send `Submission { op: Op::ApprovalResponse { approval_id, decision: Approved } }` if 'y' is entered

#### Scenario: Deny a tool execution

WHEN 'n' is entered for an approval prompt
THEN the CLI SHALL send `Op::ApprovalResponse { decision: Denied }`

### Requirement: Steering from stdin

The wire CLI SHALL support sending `Op::UserInput` as steering messages.

#### Scenario: Steering message with / prefix

WHEN the user types `/steer check file X` in the wire CLI
THEN the CLI SHALL send `Submission { op: Op::UserInput { content: "check file X" } }` as a steering message
