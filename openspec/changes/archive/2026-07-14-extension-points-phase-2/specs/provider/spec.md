# provider

## ADDED Requirements

### Requirement: Provider scope SHALL expose 4 extension points

The Provider scope SHALL expose: `provider.register`, `provider.unregister`, `provider.auth`, `provider.fallback`.

#### Scenario: provider.register is idempotent
- **WHEN** `provider.register` is fired with a `ProviderConfig { name, kind, config: serde_json::Value }`
- **AND** a provider with the same name is already registered
- **THEN** the new provider SHALL replace the old one
- **AND** a `provider.replaced` OTel event SHALL be emitted

#### Scenario: provider.unregister removes provider
- **WHEN** `provider.unregister` is fired with `name: String`
- **AND** a provider with that name is registered
- **THEN** the provider SHALL be removed
- **AND** subsequent calls to the provider SHALL fail with `ProviderNotFound`
- **AND** in-flight calls SHALL complete (cancellation is separate)

#### Scenario: provider.auth is fired before each request
- **WHEN** the provider is about to make an LLM API call
- **THEN** `provider.auth` SHALL be fired with `AuthRequest { provider_name: String, current_token: Option<String> }` by mutable reference
- **AND** the extension MAY rotate the token
- **AND** the modified token SHALL be the one used in the actual request

#### Scenario: provider.fallback selects a fallback chain
- **WHEN** the primary provider returns an error
- **THEN** `provider.fallback` SHALL be fired with `FallbackContext { primary: String, error: String }` by mutable reference
- **AND** the extension MAY set `fallback_chain: Vec<String>` (ordered list of fallback providers)
- **AND** the orchestrator SHALL attempt the fallback chain in order

### Requirement: Provider extension points SHALL be thread-safe

The Provider scope's registry SHALL be thread-safe. Provider
registration, unregistration, and fallback chain lookup MAY be
called concurrently from multiple threads. The registry SHALL use
the same `DashMap` + `AtomicU64 cache_version` pattern as
`ExtensionManager` (see Round 3 for the pattern).

#### Scenario: concurrent register and resolve
- **WHEN** two threads call `provider.register` simultaneously
- **THEN** both registrations SHALL complete without panic
- **AND** the cache version SHALL be incremented atomically
- **AND** subsequent `provider.resolve(name)` calls SHALL return the most recently registered provider

### Requirement: Provider used-by matrix SHALL be maintained per point

The Provider scope SHALL maintain a "Used by / Reserved for" matrix for every extension point. The matrix SHALL be the single source of truth documenting which points are exercised by current code vs. reserved for future use.

| Extension point | Used by | Reserved for |
|---|---|---|
| `provider.register` | — (reserved) | Late-bound provider loading (e.g., enterprise-specific provider) |
| `provider.unregister` | — (reserved) | Provider hot-swap during config reload |
| `provider.auth` | — (reserved) | Token rotation, OAuth refresh, multi-tenant auth |
| `provider.fallback` | — (reserved) | Multi-provider fallback chains (e.g., primary → secondary → tertiary) |

#### Scenario: used-by matrix SHALL be the source of truth for current consumers
- **WHEN** a developer checks which Provider extension points are exercised by current code
- **THEN** the "Used by" column SHALL accurately list every internal call site
- **AND** the "Reserved for" column SHALL list at least one concrete future use case per point
- **AND** any discrepancy SHALL be reported as a documentation bug
