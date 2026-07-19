# event-bus

## ADDED Requirements

### Requirement: Event Bus scope SHALL expose 4 extension points

The Event Bus scope SHALL expose: `event.subscribe`, `event.publish`, `event.aggregate`, `event.replay`.

#### Scenario: event.subscribe registers a handler
- **WHEN** `event.subscribe` is fired with `SubscribeRequest { topic: String, handler_id: String, handler: EventHandler }`
- **THEN** the handler SHALL be registered for the given topic
- **AND** subsequent `event.publish` calls for that topic SHALL invoke the handler in registration order
- **AND** the handler SHALL be invoked synchronously in the publishing thread

#### Scenario: event.publish fires all subscribers
- **WHEN** `event.publish` is fired with `PublishRequest { topic: String, payload: serde_json::Value }`
- **THEN** all handlers registered for the topic SHALL be invoked in registration order
- **AND** the payload SHALL be passed by reference (no clone)
- **AND** handler panics SHALL be caught and logged (consistent with Phase 3 pattern)

#### Scenario: event.aggregate groups events
- **WHEN** `event.aggregate` is fired with `AggregateRequest { topic: String, window: Duration }`
- **THEN** the extension MAY collect events for the window duration
- **AND** the extension SHALL return `Option<AggregatedEvent>` (None = no aggregation desired)
- **AND** the orchestrator SHALL emit the aggregated event once the window closes

#### Scenario: event.replay is for session restore
- **WHEN** `event.replay` is fired with `ReplayRequest { from_event_id: u64, to_event_id: Option<u64> }`
- **THEN** the extension MAY replay the events in order
- **AND** the replayed events SHALL be tagged with `replay=true` in the OTel attributes

### Requirement: Event Bus scope SHALL guarantee within-topic ordering, NOT cross-topic ordering

The Event Bus SHALL guarantee that handlers are invoked in
registration order **within a single topic**. Cross-topic ordering is
NOT guaranteed. If a handler subscribes to multiple topics, the
ordering of events across those topics is undefined.

#### Scenario: within-topic ordering
- **WHEN** two handlers `h1` and `h2` subscribe to topic `T` (h1 first, then h2)
- **AND** `event.publish(T, payload)` is called
- **THEN** `h1(payload)` SHALL be invoked before `h2(payload)`

#### Scenario: cross-topic ordering is not guaranteed
- **WHEN** a handler subscribes to topics `T1` and `T2`
- **AND** `event.publish(T1, p1)` and `event.publish(T2, p2)` are called concurrently
- **THEN** the handler MAY receive p1 before p2 OR p2 before p1
- **AND** the orchestrator SHALL NOT promise any specific order

### Requirement: Event Bus used-by matrix SHALL be maintained per point

The Event Bus scope SHALL maintain a "Used by / Reserved for" matrix for every extension point. The matrix SHALL be the single source of truth documenting which points are exercised by current code vs. reserved for future use.

| Extension point | Used by | Reserved for |
|---|---|---|
| `event.subscribe` | — (reserved) | Plugin authors subscribing to agent events |
| `event.publish` | — (reserved) | Custom event sources (e.g., external webhooks) |
| `event.aggregate` | — (reserved) | Metrics aggregation (e.g., "how many tool errors in the last 5 minutes") |
| `event.replay` | — (reserved) | Session restore, debugging, audit |

#### Scenario: used-by matrix SHALL be the source of truth for current consumers
- **WHEN** a developer checks which Event Bus extension points are exercised by current code
- **THEN** the "Used by" column SHALL accurately list every internal call site
- **AND** the "Reserved for" column SHALL list at least one concrete future use case per point
- **AND** any discrepancy SHALL be reported as a documentation bug
