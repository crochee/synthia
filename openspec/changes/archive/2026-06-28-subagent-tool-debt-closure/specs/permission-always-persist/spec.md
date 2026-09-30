<!--
Delta spec for new capability: permission-always-persist
-->

## ADDED Requirements

### Requirement: PermissionChecker SHALL persist "always" approval rules in a concurrent set

The `PermissionChecker` SHALL maintain a `saved_rules: Arc<DashSet<(String, String)>>` storing `(action, resource)` pairs that have been approved with "always" semantics. The set SHALL be checked before evaluating the configured policy.

#### Scenario: Saved rule matches request
- **WHEN** `PermissionChecker::check` receives a request with `(action, resource)` that exists in `saved_rules`
- **THEN** the result SHALL be `Permission::AutoApprove`
- **AND** the policy SHALL NOT be evaluated

#### Scenario: Saved rule does not match request
- **WHEN** `PermissionChecker::check` receives a request with `(action, resource)` not in `saved_rules`
- **THEN** the policy SHALL be evaluated as before
- **AND** the result SHALL follow existing policy logic

#### Scenario: Saved rules are session-scoped
- **WHEN** a `PermissionChecker` is dropped (session ends)
- **THEN** the `saved_rules` SHALL NOT persist to disk
- **AND** a new session SHALL start with an empty `saved_rules`

---

### Requirement: PermissionChecker SHALL expose remember_always API

The `PermissionChecker` SHALL expose `pub fn remember_always(&self, action: String, resource: String)` that inserts `(action, resource)` into `saved_rules`. This API SHALL be callable by approval services when the user replies with "always".

#### Scenario: User replies "always" to a permission request
- **WHEN** the user replies "always" to a request for `(action="bash", resource="cargo build")`
- **THEN** `remember_always("bash", "cargo build")` SHALL be called
- **AND** subsequent requests for `(action="bash", resource="cargo build")` SHALL be auto-approved

#### Scenario: Resource pattern matching is exact (no wildcard)
- **WHEN** `remember_always("bash", "cargo build")` is saved
- **AND** a request for `(action="bash", resource="cargo test")` arrives
- **THEN** the saved rule SHALL NOT match
- **AND** the policy SHALL be evaluated normally

---

### Requirement: PermissionChecker SHALL expose forget_always API for revocation

The `PermissionChecker` SHALL expose `pub fn forget_always(&self, action: &str, resource: &str)` that removes `(action, resource)` from `saved_rules`. This allows users to revoke a previously saved "always" rule.

#### Scenario: User revokes a saved rule
- **WHEN** `forget_always("bash", "cargo build")` is called
- **THEN** `(bash, cargo build)` SHALL be removed from `saved_rules`
- **AND** subsequent requests for that pair SHALL be evaluated by the policy

#### Scenario: Forget non-existent rule is a no-op
- **WHEN** `forget_always` is called for a pair not in `saved_rules`
- **THEN** no error SHALL be returned
- **AND** `saved_rules` SHALL remain unchanged
