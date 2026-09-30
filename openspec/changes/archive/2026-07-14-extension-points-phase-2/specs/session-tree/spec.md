# session-tree

## ADDED Requirements

### Requirement: Session Tree scope SHALL expose 5 extension points

The Session Tree scope SHALL expose: `session.entry.append`, `session.entry.tree_walk`, `session.branch.create`, `session.version.migrate`, `session.compaction.preserve`.

#### Scenario: session.entry.append fires before persistence
- **WHEN** a new entry is about to be appended to a session
- **THEN** the extension SHALL receive `EntryAppendInput { session_id: SessionId, entry: SessionEntry, parent_entry_id: Option<EntryId> }` by mutable reference
- **AND** the extension MAY modify `entry.metadata`, `entry.tags`, or `entry.annotations`
- **AND** the modified entry SHALL be the one persisted to storage
- **AND** the extension MAY return `Action::Skip { reason }` to reject the entry (caller surfaces the reason to the user)

#### Scenario: session.branch.create forks session
- **WHEN** an extension fires `session.branch.create` with `BranchCreateRequest { parent_session_id: SessionId, branch_name: String }`
- **THEN** a new branch SHALL be created with the given parent
- **AND** subsequent entries SHALL be appended to the new branch
- **AND** the original branch SHALL remain immutable
- **AND** the extension SHALL receive the new `SessionId` via the `Action<BranchCreateOutput>` return

#### Scenario: session.entry.tree_walk enumerates branches
- **WHEN** an extension fires `session.entry.tree_walk` with `TreeWalkRequest { root_session_id: SessionId, max_depth: u32 }`
- **THEN** the extension SHALL receive `Vec<BranchNode { session_id, parent_id, depth, entry_count }>` in pre-order traversal
- **AND** nodes exceeding `max_depth` SHALL be omitted
- **AND** the orchestrator SHALL use the same ordering as `pi-mono session-manager.ts:48-61`

#### Scenario: session.version.migrate upgrades old payloads
- **WHEN** the session loader encounters an entry with a schema version older than the current one
- **THEN** `session.version.migrate` SHALL be fired with `MigrateRequest { session_id, from_version: u32, to_version: u32, payload: serde_json::Value }`
- **AND** the extension SHALL return `Option<serde_json::Value>` (None = use default migration chain)
- **AND** if multiple migrations are needed, the hook SHALL be fired once per version step

#### Scenario: session.compaction.preserve retains extension summaries
- **WHEN** a compaction is triggered
- **THEN** the orchestrator SHALL preserve `from_hook=true` CompactionEntry details (per `pi-mono session-manager.ts:48-61`)
- **AND** subsequent re-compactions SHALL preserve core-generated details
- **AND** extension-generated details SHALL be discarded after a re-compaction unless `from_hook=true`

### Requirement: Session Tree scope SHALL guarantee write ordering and parent-link integrity

The Session Tree scope SHALL guarantee that entries are appended in
the order they are submitted to `session.entry.append`, and that the
`parent_entry_id` chain is preserved across branch creation.

#### Scenario: entries append in submission order
- **WHEN** two entries `e1` and `e2` are submitted to `session.entry.append` for the same session
- **THEN** `e1` SHALL be persisted before `e2`
- **AND** `e2.parent_entry_id` SHALL equal `e1.entry_id`

#### Scenario: branch creation freezes parent
- **WHEN** `session.branch.create` fires for parent `P`
- **THEN** `P` SHALL be marked immutable
- **AND** any subsequent `session.entry.append` for `P` SHALL return `Err(BranchFrozenError)`
- **AND** new appends SHALL go to the new branch by default

#### Scenario: tree walk is read-only
- **WHEN** `session.entry.tree_walk` fires
- **THEN** the handler SHALL NOT mutate any session state
- **AND** any mutation attempt SHALL panic with `ReadOnlyError` (debug assertions only — release builds log a warning)

### Requirement: Session Tree used-by matrix SHALL be maintained per point

The Session Tree scope SHALL maintain a "Used by / Reserved for" matrix for every extension point. The matrix SHALL be the single source of truth documenting which points are exercised by current code vs. reserved for future use.

| Extension point | Used by | Reserved for |
|---|---|---|
| `session.entry.append` | — (reserved) | Audit log injection, metadata enrichment, redaction |
| `session.entry.tree_walk` | — (reserved) | Branch visualization, cross-session analytics |
| `session.branch.create` | — (reserved) | User-initiated forks (e.g., "explore alternative") |
| `session.version.migrate` | — (reserved) | Schema upgrades for stored sessions |
| `session.compaction.preserve` | — (reserved) | Extension-generated summaries that should survive re-compaction |

#### Scenario: used-by matrix SHALL be the source of truth for current consumers
- **WHEN** a developer checks which Session Tree extension points are exercised by current code
- **THEN** the "Used by" column SHALL accurately list every internal call site
- **AND** the "Reserved for" column SHALL list at least one concrete future use case per point
- **AND** any discrepancy SHALL be reported as a documentation bug
