# Spec: server-backpressure

## ADDED Requirements

### Requirement: Per-subscriber bounded mpsc channels

`EventBroadcaster` SHALL use per-subscriber bounded mpsc channels instead of a single broadcast channel.

#### Scenario: Subscriber receives events

WHEN an `AgentEvent` is produced
THEN each subscriber's mpsc channel SHALL receive a copy of the event

#### Scenario: Slow subscriber overflow

WHEN a subscriber's channel is full (buffer capacity reached)
THEN the oldest event SHALL be dropped (ring buffer overflow policy)
AND no other subscriber SHALL be affected

#### Scenario: Fast subscriber unaffected by slow subscriber

WHEN subscriber A's channel is full and dropping events
AND subscriber B's channel has capacity
THEN subscriber B SHALL still receive all events without loss

### Requirement: Configurable buffer size

The per-subscriber buffer size SHALL be configurable via `EventBroadcasterConfig::buffer_size` (default: 256).

#### Scenario: Custom buffer size

WHEN `EventBroadcaster` is configured with `buffer_size: 512`
THEN each subscriber's channel SHALL have capacity 512

### Requirement: SubscriberRegistry

A `SubscriberRegistry` SHALL manage subscriptions with `subscribe(session_id) -> SubscriberHandle` and `unsubscribe(handle)`.

#### Scenario: Subscribe and unsubscribe

WHEN `subscribe("session-1")` is called
THEN a `SubscriberHandle` with an mpsc `Receiver` SHALL be returned
AND events for "session-1" SHALL be routed to the receiver

WHEN `unsubscribe(handle)` is called
THEN the subscriber SHALL stop receiving events
AND the channel SHALL be closed
