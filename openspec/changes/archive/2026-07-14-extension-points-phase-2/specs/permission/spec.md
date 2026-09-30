# permission

## ADDED Requirements

### Requirement: Permission scope SHALL expose 5 extension points

The Permission scope SHALL expose: `permission.ask`, `permission.notify`, `doom_loop.detected`, `blacklist.match`, `permission.persist`.

#### Scenario: permission.ask may only ADD to the deny list
- **WHEN** `permission.ask` is fired before asking the user
- **THEN** the extension SHALL receive `PermissionDecision { outcome: AskUser | Allow | Deny, reason: String }` by mutable reference
- **AND** the extension MAY transition `AskUser` → `Deny` (more restrictive)
- **AND** the extension MAY NOT transition `Deny` → `Allow` or `Deny` → `AskUser` (less restrictive)
- **AND** if the extension attempts a less-restrictive transition, the `PermissionExtensibilityGuard` SHALL downgrade the final decision to `AskUser` (P6 fail-closed)
- **AND** a `permission.weakening_attempt` OTel event SHALL be emitted

#### Scenario: permission.notify is observe-only
- **WHEN** `permission.notify` is fired after a permission decision is made
- **THEN** the extension SHALL receive `PermissionDecision` (immutable)
- **AND** the extension MAY log, audit, or forward the decision
- **AND** the decision SHALL NOT be modified

#### Scenario: doom_loop.detected triggers fail-closed response
- **WHEN** the doom loop detector fires
- **THEN** `doom_loop.detected` SHALL be fired with `DoomLoopInfo { tool_name: String, repetition_count: u32 }`
- **AND** the extension MAY return `DoomLoopAction::{AllowOneMore, DenyNow, AskUser}`
- **AND** the orchestrator SHALL execute the action (P6 fail-closed: `DenyNow` is the safe default if no handler is registered)

#### Scenario: blacklist.match is hot-path
- **WHEN** a permission check is about to ask the user
- **THEN** `blacklist.match` SHALL be fired first (before `permission.ask`)
- **AND** if the extension returns `Some(BlacklistEntry)`, the check SHALL skip the user prompt and apply the entry's verdict
- **AND** the extension SHALL run in O(1) (no LLM call)

### Requirement: Permission extension points SHALL preserve P6 fail-closed semantics

All Permission scope extension points SHALL be constrained to NOT
weaken the existing permission decision. The `PermissionExtensibilityGuard`
is a runtime wrapper that enforces this constraint. The guard is
implemented in the Permission scope's registry, not in the underlying
`ApprovalService`.

#### Scenario: PermissionExtensibilityGuard test
- **WHEN** a test registers a `permission.ask` handler that returns `Allow` (weakening) for a `Deny` decision
- **THEN** the final decision SHALL be `AskUser` (the guard downgrades)
- **AND** the test SHALL verify the `permission.weakening_attempt` OTel event was emitted

#### Scenario: legitimate use case (deny blacklist)
- **WHEN** a security plugin registers a `blacklist.match` handler that returns `Some(BlacklistEntry { verdict: Deny, reason: "known-bad-pattern" })`
- **THEN** the user SHALL NOT be prompted
- **AND** the tool call SHALL be denied immediately
- **AND** the decision SHALL be logged with the `blacklist` source

### Requirement: Permission used-by matrix SHALL be maintained per point

The Permission scope SHALL maintain a "Used by / Reserved for" matrix for every extension point. The matrix SHALL be the single source of truth documenting which points are exercised by current code vs. reserved for future use.

| Extension point | Used by | Reserved for |
|---|---|---|
| `permission.ask` | — (reserved) | Security plugins that want to add to the deny list |
| `permission.notify` | — (reserved) | Audit logging, security incident response |
| `doom_loop.detected` | — (reserved) | Custom doom loop policies (e.g., allow 3 instead of 2) |
| `blacklist.match` | — (reserved) | Hot-path blacklist (regex, known-bad commands) |
| `permission.persist` | — (reserved) | Save decisions across sessions (today this is in `ApprovalService` directly) |

#### Scenario: used-by matrix SHALL be the source of truth for current consumers
- **WHEN** a developer checks which Permission extension points are exercised by current code
- **THEN** the "Used by" column SHALL accurately list every internal call site
- **AND** the "Reserved for" column SHALL list at least one concrete future use case per point
- **AND** any discrepancy SHALL be reported as a documentation bug
