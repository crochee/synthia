## ADDED Requirements

### Requirement: "always" permission choice SHALL auto-resolve matching pending requests in same session

When the user selects "always allow" for a permission request, the system MUST scan all other pending permission requests in the same session. For each pending request, if every resource in the request matches an "always allow" rule just established, the request MUST be auto-resolved as allowed without displaying a second prompt.

#### Scenario: Two pending requests with overlapping resources

- **WHEN** user selects "always allow" for `bash` tool with resources `["ls"]`
- **AND** another pending request exists in the same session for `bash` with resources `["ls", "pwd"]`
- **THEN** the second request is NOT auto-resolved (resources do not fully match)
- **AND** the second request is displayed to the user

#### Scenario: Two pending requests with identical resources

- **WHEN** user selects "always allow" for `bash` tool with resources `["ls"]`
- **AND** another pending request exists in the same session for `bash` with resources `["ls"]`
- **THEN** the second request is auto-resolved as allowed
- **AND** no second prompt is displayed to the user

#### Scenario: Propagation does not cross session boundary

- **WHEN** user selects "always allow" in session A
- **AND** a pending request exists in session B with identical resources
- **THEN** the session B request is NOT auto-resolved
- **AND** session B's request is displayed to its own user

---

### Requirement: "reject" permission choice SHALL cascade-terminate same-session pending requests

When the user selects "reject" (or "always reject") for a permission request, the system MUST terminate all other pending permission requests in the same session. Each terminated request MUST be marked as rejected with reason "cascade-from-session-reject".

#### Scenario: Reject cascades to all same-session pending

- **WHEN** user selects "reject" for one permission request
- **AND** three other pending requests exist in the same session
- **THEN** all three pending requests are terminated with status rejected
- **AND** the rejection reason is recorded as "cascade-from-session-reject"
- **AND** the corresponding tool calls return a permission-denied error

#### Scenario: Reject does not cascade across sessions

- **WHEN** user selects "reject" in session A
- **AND** pending requests exist in session B
- **THEN** session B's pending requests are unaffected
- **AND** session B continues normal permission flow
