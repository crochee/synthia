# subagent-tree-cancellation Specification

## Purpose
TBD - created by archiving change subagent-tool-debt-closure. Update Purpose after archive.
## Requirements
### Requirement: SubagentManager SHALL track child sessions for recursive cancellation

The `SubagentManager` SHALL maintain a `child_sessions: DashMap<SessionId, Vec<SessionId>>` mapping parent session IDs to their direct child session IDs. When `create_child` is called, the manager SHALL register the parent-child relationship by inserting the child ID into the parent's entry.

#### Scenario: Child session registered on creation
- **WHEN** `create_child(parent_id, ...)` successfully creates a child session
- **THEN** the child session ID SHALL be appended to `child_sessions[parent_id]`

#### Scenario: Child registration cleaned up on session removal
- **WHEN** a session is removed via `remove_session(session_id)`
- **THEN** the manager SHALL remove the session from its parent's child list
- **AND** the manager SHALL remove the session's own child list entry

---

### Requirement: SubagentManager SHALL provide cancel_session_tree for recursive cancellation

The `SubagentManager` SHALL expose a `cancel_session_tree(session_id: &SessionId)` method that recursively cancels the session and all its descendants. The method SHALL perform a depth-first traversal of `child_sessions`, canceling each descendant's cancellation token before canceling the target session.

#### Scenario: Cancel parent cancels all descendants
- **WHEN** `cancel_session_tree(parent_id)` is called
- **THEN** all direct children of `parent_id` SHALL be canceled
- **AND** all grandchildren SHALL be canceled recursively
- **AND** the parent itself SHALL be canceled last

#### Scenario: Cancel with no children
- **WHEN** `cancel_session_tree(session_id)` is called for a session with no children
- **THEN** only the target session SHALL be canceled

#### Scenario: Cancel handles concurrent child removal
- **WHEN** `cancel_session_tree` is traversing and a child is concurrently removed
- **THEN** the traversal SHALL skip the removed child without panic
- **AND** remaining children SHALL still be canceled

---

### Requirement: Per-session child cancellation token SHALL coexist with shared parent token

Each child session SHALL have a `child_cancel_token: CancellationToken` derived from `parent_cancel_token.child_token()`. Canceling the parent's shared token SHALL propagate to all children (existing behavior). Canceling a specific child's token via `cancel_session_tree` SHALL cancel only that subtree.

#### Scenario: Parent cancel propagates to all children (existing behavior preserved)
- **WHEN** the parent's shared `cancel_token` is canceled
- **THEN** all child sessions SHALL be canceled (via child_token derivation)

#### Scenario: Subtree cancel does not affect siblings
- **WHEN** `cancel_session_tree(child_a_id)` is called
- **THEN** child_a and its descendants SHALL be canceled
- **AND** sibling child_b SHALL NOT be canceled

