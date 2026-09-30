# tool-retry Specification

## Purpose
Automatic retry of failed tool executions with exponential backoff for temporary/transient errors.

## ADDED Requirements

### Requirement: Retryable errors SHALL trigger automatic retry

When a tool execution fails with a retryable error (timeout, connection reset, rate limit, or 5xx HTTP status), the system SHALL retry the execution up to 2 additional times before propagating the failure.

#### Scenario: Timeout error triggers retry
- **WHEN** a tool execution fails with "connection timed out"
- **THEN** the system SHALL retry the same tool execution
- **AND** the first retry SHALL occur after 2 seconds

#### Scenario: Rate limit error triggers retry
- **WHEN** a tool execution fails with HTTP 429 (Too Many Requests)
- **THEN** the system SHALL retry the same tool execution
- **AND** the first retry SHALL occur after 2 seconds

#### Scenario: Server error triggers retry
- **WHEN** a tool execution fails with HTTP 503 (Service Unavailable)
- **THEN** the system SHALL retry the same tool execution
- **AND** the first retry SHALL occur after 2 seconds

#### Scenario: Non-retryable error does not trigger retry
- **WHEN** a tool execution fails with "file not found"
- **THEN** the system SHALL NOT retry
- **AND** the failure SHALL be propagated immediately

---

### Requirement: Retry attempts SHALL use exponential backoff

Retry delays SHALL increase exponentially: 2 seconds, then 4 seconds, then 8 seconds.

#### Scenario: Second retry uses longer delay
- **WHEN** a retryable error occurs and the first retry also fails
- **THEN** the second retry SHALL occur after 4 seconds (not 2 seconds)

#### Scenario: Third attempt uses maximum delay
- **WHEN** a retryable error occurs, the first retry fails, and the second retry also fails
- **THEN** the third attempt SHALL occur after 8 seconds (not 4 seconds)

---

### Requirement: Maximum retry attempts SHALL be 2

The system SHALL make at most 2 retry attempts (3 total attempts including the initial call).

#### Scenario: Success on second attempt stops retries
- **WHEN** initial execution fails but second attempt succeeds
- **THEN** no further retries SHALL occur
- **AND** the tool result SHALL be returned as normal

#### Scenario: All retries exhausted propagates failure
- **WHEN** all 3 attempts (initial + 2 retries) fail
- **THEN** the failure SHALL be propagated to the recovery cascade
- **AND** the error SHALL include the total attempt count
