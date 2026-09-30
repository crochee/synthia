## ADDED Requirements

### Requirement: GoalService Trait
`GoalService` SHALL provide `current() -> Option<Goal>`, `set(goal) -> Result`, `status() -> GoalStatus`, and `budget() -> GoalBudget`. `GoalStatus` SHALL be an enum: `Active`, `Blocked`, `BudgetLimited`, `UsageLimited`.

#### Scenario: Goal status blocks loop
- **WHEN** `GoalService::status()` returns `GoalStatus::Blocked`
- **THEN** the ReAct loop SHALL break, awaiting user intervention

#### Scenario: Token budget limits turn
- **WHEN** `GoalService::budget()` returns a `token_budget` and the turn exceeds it
- **THEN** the loop SHALL truncate the turn and set `GoalStatus::BudgetLimited`

---

### Requirement: GoalService Optional with No-Op Default
`GoalService` SHALL be an optional service in `LoopServices`. When absent, `LoopServices::bootstrap` SHALL substitute `NoopGoalService` which returns `None` from `current()` and `Active` from `status()`.

#### Scenario: GoalService not configured
- **WHEN** `ServiceRegistry` does not contain a `GoalService`
- **THEN** `LoopServices::bootstrap` SHALL use `NoopGoalService` with a warning log

#### Scenario: NoopGoalService never blocks
- **WHEN** `NoopGoalService::status()` is called
- **THEN** it SHALL always return `GoalStatus::Active` — the loop never breaks due to goal
