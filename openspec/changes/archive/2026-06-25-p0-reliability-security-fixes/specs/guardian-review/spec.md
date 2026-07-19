<!--
Delta spec for guardian-review capability.
-->

## ADDED Requirements

### Requirement: Guardian Check Conversation Context

`GuardianReviewer::check` SHALL accept a `conversation: &[Message]` parameter and pass it to `build_review_prompt` via `collect_transcript_entries`, ensuring the review has access to dialog context for detecting cross-turn prompt injection.

#### Scenario: check with conversation context

- **WHEN** `GuardianReviewer::check` is called with a non-empty conversation
- **THEN** the review prompt SHALL include transcript entries from the conversation
- **AND** the Guardian SHALL be able to detect instructions injected in earlier turns

#### Scenario: check with empty conversation

- **WHEN** `GuardianReviewer::check` is called with an empty conversation slice
- **THEN** the review prompt SHALL include an empty transcript
- **AND** the Guardian SHALL still evaluate the action based on action JSON alone

---

### Requirement: Guardian Check Request Pass-Through

`GuardianReviewer::check` SHALL pass the actual `ApprovalRequest` to `make_guardian_decision`, not a placeholder request, ensuring user confirmation prompts reference the correct action.

#### Scenario: NeedUserConfirm uses actual request

- **WHEN** `make_guardian_decision` returns `NeedUserConfirm`
- **THEN** the `request` field SHALL contain the actual `ApprovalRequest` passed to `check`
- **AND** the user confirmation prompt SHALL reference the correct action details

---

### Requirement: Guardian Check Signature

`GuardianReviewer::check` SHALL have the signature `async fn check(&self, request: &ApprovalRequest, conversation: &[Message], router: &Arc<dyn ModelRouter>) -> GuardianDecision`.

#### Scenario: check signature includes conversation

- **WHEN** a caller invokes `GuardianReviewer::check`
- **THEN** the caller SHALL provide a `conversation` parameter
- **AND** the compiler SHALL enforce this at all call sites
