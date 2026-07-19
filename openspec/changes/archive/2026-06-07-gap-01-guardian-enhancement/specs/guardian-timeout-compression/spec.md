## ADDED Requirements

### Requirement: GuardianReviewer shall implement 30-second timeout

The GuardianReviewer SHALL enforce a 30-second timeout on all LLM review calls. If the LLM call does not complete within 30 seconds, the review SHALL be considered failed and fail-closed.

#### Scenario: LLM completes within timeout
- **WHEN** LLM review call completes in under 30 seconds
- **THEN** GuardianReviewer SHALL return the LLM's decision

#### Scenario: LLM exceeds timeout
- **WHEN** LLM review call exceeds 30 seconds
- **THEN** GuardianReviewer SHALL cancel the call and return GuardianDecision::Deny with reason "LLM review timeout - fail closed"

---

### Requirement: GuardianReviewer shall compress conversation transcript

The GuardianReviewer SHALL compress the conversation transcript before sending to LLM to prevent context window overflow and reduce latency.

Compression strategy:
- Preserve system prompt and ruleset summary
- Preserve most recent 10 message rounds
- Summarize older messages into a brief context summary

#### Scenario: Long conversation compressed
- **WHEN** GuardianReviewer receives a conversation with 50+ messages
- **THEN** GuardianReviewer SHALL compress to include system prompt + ruleset + recent 10 rounds + summary of older content

#### Scenario: Short conversation passed through
- **WHEN** GuardianReviewer receives a conversation with fewer than 10 rounds
- **THEN** GuardianReviewer SHALL pass the full transcript to LLM without compression

---

### Requirement: GuardianReviewer shall build review prompt from compressed transcript

The GuardianReviewer SHALL build a structured review prompt that includes:
- Task context (what the agent is trying to do)
- The specific approval request being reviewed
- Compressed conversation history
- Risk assessment criteria

#### Scenario: Build review prompt
- **WHEN** GuardianReviewer needs to evaluate an ApprovalRequest
- **THEN** Guardian SHALL build a prompt containing: task context, approval request details, compressed history, and risk criteria

#### Scenario: Risk criteria in prompt
- **WHEN** Building review prompt
- **THEN** Guardian SHALL include explicit risk criteria: destructive operations, credential access, network transmission, data exfiltration