## ADDED Requirements

### Requirement: apply_cache_policy SHALL short-circuit on Arc reference equality

When all three of `tools`, `system`, and `messages` are wrapped in `Arc` and `Arc::ptr_eq` returns true for all three against the previously cached request, `apply_cache_policy` MUST return the original request reference without allocating a new request or invalidating the provider cache. This zero-allocation fast path aligns with opencode `applyCachePolicy` reference equality semantics.

#### Scenario: All three fields unchanged by reference

- **WHEN** `apply_cache_policy` is called with `tools`, `system`, `messages` Arc references identical to the previous call
- **THEN** the function returns the original cached request reference
- **AND** no new allocation occurs
- **AND** the provider's prompt cache prefix remains valid (no cache invalidation)

#### Scenario: Any one field changed by reference

- **WHEN** `apply_cache_policy` is called with `system` Arc unchanged but `tools` Arc replaced (e.g., new tool registered)
- **THEN** the function performs full cache policy evaluation
- **AND** a new request is allocated with updated cache control hints
- **AND** the provider cache prefix is invalidated as expected

#### Scenario: First call has no previous reference

- **WHEN** `apply_cache_policy` is called for the first time in a session (no prior request)
- **THEN** the function performs full cache policy evaluation
- **AND** a new request is allocated and stored as the cached reference for subsequent calls
