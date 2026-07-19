## ADDED Requirements

### Requirement: ScopedToolRegistry SHALL support scoped registration with token

`ScopedToolRegistry` SHALL provide a `register_scoped(tools, token)` method that registers tools within a scope identified by a unique token. All registrations associated with a token SHALL be automatically removed when the corresponding `ScopeGuard` is dropped.

#### Scenario: Scoped registration associates tools with token
- **WHEN** `ScopedToolRegistry::register_scoped(tools, token)` is called
- **THEN** each tool in `tools` SHALL be associated with the given `token`
- **AND** the tools SHALL be stored in the registry's local map

#### Scenario: ScopeGuard drop triggers deregistration
- **WHEN** the `ScopeGuard` for a scope is dropped
- **THEN** all tools registered with that scope's token SHALL be automatically deregistered
- **AND** subsequent calls to `materialize()` SHALL not include those tools

#### Scenario: Scoped registration overrides global for same name
- **WHEN** a tool with name "foo" is registered globally
- **AND** the same name "foo" is registered with a scoped registration
- **THEN** `materialize()` SHALL return the scoped version (last-wins semantics)

---

### Requirement: ScopedToolRegistry SHALL provide ScopeGuard for RAII cleanup

`ScopedToolRegistry::create_scope()` SHALL return a tuple of `(Arc<ScopedToolRegistry>, ScopeGuard)` where `ScopeGuard` implements `Drop` to automatically deregister all tools when dropped.

#### Scenario: Create scope returns registry and guard
- **WHEN** `ScopedToolRegistry::create_scope(global_registry)` is called
- **THEN** a tuple `(registry, guard)` SHALL be returned
- **AND** `registry` SHALL reference the same global registry
- **AND** `guard` SHALL hold a unique token for this scope

#### Scenario: Guard drop deregisters all scoped tools
- **WHEN** `guard` is dropped (explicitly or via scope end)
- **THEN** all tools registered with `guard.token()` SHALL be removed from `registry.local`
- **AND** `registry.materialize()` SHALL no longer include those tools

#### Scenario: Multiple concurrent scopes coexist
- **WHEN** scope A and scope B are created
- **THEN** both scopes SHALL coexist independently
- **AND** each scope's tools SHALL be isolated from the other

---

### Requirement: ScopedToolRegistry::materialize SHALL use last-wins semantics

When materializing tools for execution, `ScopedToolRegistry::materialize()` SHALL return the most recently registered version of each tool. Scoped registrations override global registrations.

#### Scenario: Materialize returns scoped over global
- **WHEN** "tool_a" is registered globally
- **AND** "tool_a" is registered with an active scope
- **THEN** `materialize()` SHALL return the scoped "tool_a" version

#### Scenario: Materialize after scope end returns global
- **WHEN** "tool_a" is registered globally
- **AND** "tool_a" is registered with scope S
- **AND** scope S ends (ScopeGuard dropped)
- **THEN** `materialize()` SHALL return the global "tool_a" version

#### Scenario: Multiple scopes for same name uses most recent
- **WHEN** scope S1 registers "tool_a"
- **AND** scope S2 registers "tool_a" (different implementation)
- **THEN** `materialize()` SHALL return S2's version (last registered)

---

### Requirement: ScopedToolRegistry SHALL be thread-safe

`ScopedToolRegistry` SHALL support concurrent access from multiple threads. Registration and materialization SHALL use appropriate synchronization.

#### Scenario: Concurrent registration from multiple threads
- **WHEN** thread A calls `register_scoped(tools_a, token_a)`
- **AND** thread B calls `register_scoped(tools_b, token_b)` simultaneously
- **THEN** both registrations SHALL succeed without data races
- **AND** both scopes SHALL coexist correctly

#### Scenario: Concurrent materialize during registration
- **WHEN** thread A is registering tools
- **AND** thread B calls `materialize()` simultaneously
- **THEN** `materialize()` SHALL return a consistent snapshot
- **AND** no registration SHALL be partially visible

---

### Requirement: ScopeGuard SHALL support manual deregistration

`ScopeGuard` SHALL provide a `deregister_all()` method for explicit cleanup before drop.

#### Scenario: Manual deregistration empties scope
- **WHEN** `guard.deregister_all()` is called
- **THEN** all tools registered with `guard.token()` SHALL be removed
- **AND** subsequent `materialize()` calls SHALL not include those tools

#### Scenario: Manual deregistration invalidates guard
- **WHEN** `guard.deregister_all()` is called
- **THEN** calling `deregister_all()` again SHALL be a no-op
- **AND** dropping the guard SHALL not attempt to deregister again
