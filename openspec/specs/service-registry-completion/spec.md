# Capability: service-registry-completion

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: opencode `serviceRegistry.ts` + codex `codex-rs/core/src/service.rs`

## Purpose

完善 `synthia-service::registry` (286 行 + 4 处 TODO)，新增 `OutputBound::Service` trait 反向依赖切断、typed `Capability<T>` contract、peer-source 标识（CapsuleId/StreamId）、reverse-dependency resolution（change #1 阶段不引入 CapabilityBroker，推迟到 change #3）。

## ADDED Requirements

### Requirement: OutputBound::Service trait

The `ServiceRegistry` MUST expose `bound_service::<T>()` returning a typed `Arc<T>` handle via the `OutputBoundService` trait.

#### Scenario: bind to a typed service

- **WHEN** a consumer calls `registry.bound_service::<dyn MyCapability>()`
- **THEN** the registry MUST return `Arc<dyn MyCapability>` if the service implements `OutputBoundService<Service = dyn MyCapability>`
- **AND** MUST return `ServiceRegistryError::NotBound` if the bound type does not match

#### Scenario: bind to dyn-incompatible type rejected

- **WHEN** a consumer calls `registry.bound_service::<NotSend>()`
- **AND** `NotSend` does not implement `Send + Sync + 'static`
- **THEN** the compiler MUST reject the call at compile time (no panic at runtime)

### Requirement: Capability typed contract

The `ServiceRegistry` MUST allow services to declare a typed `Capability<T>` contract describing what they expose.

#### Scenario: capability declaration

- **WHEN** a service is registered via `registry.register_with_capability(svc, Capability::of::<MyCapability>())`
- **THEN** consumers MUST be able to query `registry.capabilities_provided::<dyn MyCapability>()`
- **AND** MUST receive `&[ProviderId]` listing all providers of that capability

#### Scenario: capability mismatch on register

- **WHEN** the service's actual type does NOT match the declared capability
- **THEN** the registration MUST fail with `ServiceRegistryError::CapabilityMismatch { expected, found }`

### Requirement: peer-source identification

The `ServiceRegistry` MUST support peer-source tagged services (e.g. `CapsuleId`, `StreamId`).

#### Scenario: register with peer source

- **WHEN** `registry.register_with_source(svc, Source::Capsule(id))` is called
- **THEN** the service MUST be retrievable via `registry.get_by_capsule::<MyCapability>(id)`
- **AND** MUST be evicted when the capsule ends

#### Scenario: source mismatch

- **WHEN** a consumer calls `get_by_capsule::<NotExposed>(id)`
- **THEN** the registry MUST return `ServiceRegistryError::SourceNotFound { source, capability }`

### Requirement: reverse-dependency resolution (no broker)

The `ServiceRegistry` MUST track reverse-dependency edges (which services depend on which) without introducing a runtime broker. (Broker is deferred to change #3.)

#### Scenario: dependency tracking on bind

- **WHEN** service A calls `registry.bound_service::<B>()`
- **THEN** the registry MUST record edge `A → B` in a `DashMap<ServiceId, BTreeSet<ServiceId>>`
- **AND** MUST expose `registry.reverse_dependents_of::<B>() -> Vec<ServiceId>` for tooling/diagnostics

#### Scenario: cycle detection

- **WHEN** a call would introduce a cycle in the dependency graph
- **THEN** the bind MUST fail with `ServiceRegistryError::Cycle { path }`
- **AND** no edge MUST be inserted
