# Capability: event-v2-system

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: opencode `packages/core/src/event.ts:680` + `truncate.ts:158`

## Purpose

为 Synthia 提供一个持久化可选的 Event Sourcing 系统，替代现有 stub `synthia-event::bus` (39 行)，并保留 Synthia 独有优势（PrefixTracker 三段 hash + JSONL 事件流 + TURN_* 三态机 + gRPC message-proxy）。

## ADDED Requirements

### Requirement: EventV2 dual-layer bus

The `synthia-event-v2` system MUST provide a dual-layer event abstraction with an `EventBus` trait and an `EventSink` enum, supporting `InMemory` (default) and `Sqlite` (feature-gated) implementations.

#### Scenario: default in-memory sink

- **WHEN** the consumer crates a `default event-v2` feature build
- **THEN** the system MUST instantiate `EventSink::InMemory` (bounded ring 1024) without any external dependency
- **AND** MUST NOT require `rusqlite` to be present at compile time

#### Scenario: sqlite sink optional

- **WHEN** the consumer enables `--features event-v2,sqlite`
- **THEN** the system MUST instantiate `EventSink::Sqlite` using `rusqlite` 0.32+
- **AND** MUST persist events to dual tables (`events` + `projections`)
- **AND** MUST survive process restart

### Requirement: EventEnvelope with prefix-version metadata

Every dispatched event MUST carry an `EventEnvelope<T>` containing `EventVersion`, `EventMeta`, and the payload `T`.

#### Scenario: envelope carries prefix hash

- **WHEN** any event is emitted via `EventBus::emit`
- **THEN** the resulting `EventEnvelope<T>` MUST contain `EventMeta { prefix_hash: [u8; 32], sequence: u64, source: EventSource }`
- **AND** the `prefix_hash` MUST be derived via the existing PrefixTracker three-segment rolling hash (Synthia 保留)
- **AND** MUST be deterministic given the same source payload

#### Scenario: envelope version increment

- **WHEN** multiple events are emitted in sequence
- **THEN** `EventVersion` MUST monotonically increase per source
- **AND** MUST be persisted in sqlite sink
- **AND** MUST be dropped with the process in in-memory sink

### Requirement: Projector + CommitGuard facade

The system MUST expose a `Projector` trait and a `CommitGuard` that together form an `aggregate_events<EventType>()` facade for downstream consumers.

#### Scenario: projector projection

- **WHEN** a consumer calls `aggregate_events::<MyEventType>()`
- **THEN** the system MUST replay all matching events from `EventBus` sink
- **AND** MUST invoke the registered `Projector::project` for each
- **AND** MUST skip events rejected by `CommitGuard::validate`

#### Scenario: commit guard rejection

- **WHEN** an event fails `CommitGuard::validate`
- **THEN** the system MUST log a warning with `event_id` + reason
- **AND** MUST NOT invoke downstream projectors
- **AND** MUST increment a metrics counter `event_v2_commit_guard_rejected_total`

### Requirement: CleanupTask with 7-day retention

The system MUST run a background `CleanupTask` that enforces a 7-day retention policy on persisted events.

#### Scenario: cleanup runs every 1h

- **WHEN** the runtime is started with `event-v2` feature
- **THEN** a `CleanupTask` MUST be spawned every 3600 seconds
- **AND** MUST delete events whose `created_at` is older than `now() - 7days`
- **AND** MUST delete orphan projection rows referencing deleted events

#### Scenario: cleanup disabled

- **WHEN** the consumer sets `SYNTHIA_EVENT_V2_RETENTION_DAYS=0`
- **THEN** the `CleanupTask` MUST be a no-op
- **AND** no events MUST be deleted automatically

### Requirement: gRPC message-proxy bridge

The system MUST bridge `EventBus` events to the existing gRPC message-proxy without behavioral change.

#### Scenario: grpc downstream fanout

- **WHEN** an event is emitted via `EventBus::emit`
- **AND** a gRPC subscriber is registered via `message_proxy::subscribe`
- **THEN** the bridge MUST forward the `EventEnvelope<T>` as a gRPC stream message
- **AND** MUST maintain the existing `TURN_*` three-state machine semantics (Synthia 保留)
