# subagent-quota-raii Specification

## Purpose
TBD - created by archiving change subagent-tool-debt-closure. Update Purpose after archive.
## Requirements
### Requirement: SubagentManager SHALL provide RAII-based quota management via SlotGuard

The `SubagentManager::try_acquire_slot` method SHALL return `Option<SlotGuard>` instead of `bool`. The `SlotGuard` struct SHALL hold an `Arc<SubagentManager>` reference and a `released: bool` flag. When `SlotGuard` is dropped without calling `commit()`, the quota slot SHALL be automatically released back to the manager. The `commit()` method SHALL mark the guard as released to prevent double-release.

#### Scenario: Successful slot acquisition and commit
- **WHEN** `try_acquire_slot()` is called and a slot is available
- **THEN** it SHALL return `Some(SlotGuard)`
- **AND WHEN** `commit()` is called on the guard
- **THEN** the slot SHALL remain consumed and `released` SHALL be `true`

#### Scenario: Guard dropped without commit releases slot
- **WHEN** a `SlotGuard` is obtained but dropped without calling `commit()`
- **THEN** the `Drop` implementation SHALL call `release_slot()` on the manager
- **AND** the slot SHALL be available for future acquisitions

#### Scenario: Quota exhausted returns None
- **WHEN** `try_acquire_slot()` is called when `current_concurrent >= max_concurrent`
- **THEN** it SHALL return `None`
- **AND** no slot SHALL be consumed

#### Scenario: Double-release is prevented
- **WHEN** `commit()` is called on a guard, then the guard is dropped
- **THEN** the `Drop` implementation SHALL NOT call `release_slot()` again
- **AND** the slot count SHALL remain consistent

---

### Requirement: SlotGuard SHALL not be Send across await points

The `SlotGuard` SHALL be created and consumed (via `commit()` or drop) within the same synchronous execution scope of `AgentTool::call`. The guard SHALL NOT be held across `.await` points to ensure deterministic drop timing.

#### Scenario: Guard used in synchronous scope
- **WHEN** `AgentTool::call` acquires a guard and calls `commit()` before any `.await`
- **THEN** the slot SHALL be correctly marked as committed
- **AND** no leak SHALL occur even if subsequent `.await` fails

