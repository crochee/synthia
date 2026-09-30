# permission-fail-closed Specification

## Purpose

Unify 4 separate `PermissionPolicy` implementations (synthia-permission/policy, synthia-permission/merged_policy, synthia-tool/exec/permission, synthia-agent/fork_policy) into a single `MergedPolicy`. Change the `MergedPolicy::evaluate` default from `Allow` (fail-open) to `Ask` (fail-closed) to fix CVE-level security bug C2 (per security review R2).

## ADDED Requirements

### Requirement: MergedPolicy.evaluate shall return Ask for unknown patterns (fail-closed)

`MergedPolicy::evaluate(pattern: &str) -> PermissionAction` SHALL return `PermissionAction::Ask` when the pattern is not registered in any rule layer.

The `Allow` return path SHALL be reachable only when an explicit `Allow` rule exists for the pattern in the user/agent/default ruleset.

`Allow` from the default ruleset SHALL NOT override an explicit `Deny` from a higher-priority layer.

#### Scenario: Unknown pattern asks
- **WHEN** `MergedPolicy::default().evaluate("nonexistent_tool")` is called
- **THEN** the return value SHALL be `PermissionAction::Ask` (NOT `PermissionAction::Allow`)

#### Scenario: Explicit Allow rule is honored
- **WHEN** a rule `{ pattern: "bash", action: Allow }` is registered in the user ruleset
- **THEN** `evaluate("bash")` SHALL return `PermissionAction::Allow`

#### Scenario: Explicit Deny rule is honored
- **WHEN** a rule `{ pattern: "evil_tool", action: Deny, forced: true }` is registered
- **THEN** `evaluate("evil_tool")` SHALL return `PermissionAction::Deny` regardless of lower-layer Allow rules

#### Scenario: Three-layer merge respects priority
- **WHEN** user rule says Allow and default rule says Deny
- **THEN** the user rule SHALL win (higher priority)
- **WHEN** user rule says Deny and default rule says Allow
- **THEN** the user rule SHALL win (higher priority)

---

### Requirement: The legacy PermissionPolicy struct shall be removed

`crates/synthia-permission/src/policy.rs` SHALL contain only the new `MergedPolicy` (and any internal helpers). The old `PermissionPolicy` struct, `Permission` enum, and `RuleSet` compat adapter SHALL be deleted.

`synthia-permission/src/lib.rs` SHALL re-export `MergedPolicy` as the primary public type. (The name `PermissionPolicy` MAY remain as a type alias for 1 release, then be removed.)

#### Scenario: Old PermissionPolicy struct is gone
- **WHEN** `grep -r "pub struct PermissionPolicy" crates/synthia-permission/` is run
- **THEN** the search SHALL return 0 results (the old struct is deleted)
- **AND** the only `Permission`-related types SHALL be `MergedPolicy` and its dependencies

#### Scenario: RuleSet compat adapter is gone
- **WHEN** `grep -r "RuleSet" crates/synthia-permission/` is run
- **THEN** the search SHALL return 0 results (the compat adapter is deleted)

#### Scenario: Tests use MergedPolicy
- **WHEN** `cargo test -p synthia-permission` is run
- **THEN** all 18+ previously-failing tests (using old `PermissionPolicy`) SHALL pass using `MergedPolicy`

---

### Requirement: synthia-tool::exec::permission shall use synthia_permission::Permission

`crates/synthia-tool/src/exec/permission.rs` SHALL NOT define a local `PermissionPolicy` struct. It SHALL import `synthia_permission::MergedPolicy` (or a `Permission` type alias) and use it.

The previously-broken `use crate::types::PermissionLevel;` import (which referenced a non-existent type) SHALL be removed.

#### Scenario: synthia-tool compiles with all features
- **WHEN** `cargo check -p synthia-tool --all-features` is run
- **THEN** the build SHALL succeed
- **THEN** the error "cannot find type `PermissionLevel` in this scope" SHALL NOT appear

#### Scenario: Local PermissionPolicy is gone
- **WHEN** `grep -r "pub struct PermissionPolicy" crates/synthia-tool/` is run
- **THEN** the search SHALL return 0 results

---

### Requirement: MergedPolicy default shall be aligned with PermissionPolicy::default()

`MergedPolicy::default()` and the new behavior SHALL be consistent with `PermissionPolicy::default()` semantics (which already uses `RequireConfirm` / `Ask`).

Documentation in `MergedPolicy` SHALL explicitly state: "Unknown patterns require explicit user confirmation. This is intentional fail-closed behavior."

#### Scenario: Default state is fail-closed
- **WHEN** `MergedPolicy::default()` is constructed with no rules
- **THEN** `evaluate("any_pattern")` SHALL return `PermissionAction::Ask`

#### Scenario: CHANGELOG documents the breaking change
- **WHEN** the breaking change is released
- **THEN** `CHANGELOG.md` SHALL contain an entry under the version with title "BREAKING: `MergedPolicy::evaluate` default changed from `Allow` to `Ask`"
- **THEN** the entry SHALL include migration instructions: "Add explicit `Allow` rules for all tools you want to silently allow"
