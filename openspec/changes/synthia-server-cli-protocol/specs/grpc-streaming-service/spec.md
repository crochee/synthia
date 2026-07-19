# Spec: grpc-streaming-service

## ADDED Requirements

### Requirement: SynthiaService gRPC proto definition

The system SHALL define a `SynthiaService` gRPC proto with:
- `SubmitOp(Submission) → stream EventMsg` — submit an operation and receive streaming events
- `SubscribeSession(SubscribeRequest) → stream EventMsg` — subscribe to session events

#### Scenario: SubmitOp RPC streams events

WHEN a client calls `SubmitOp` with `Submission { op: Op::UserInput { content: "hello" } }`
THEN the server SHALL submit the operation to `SessionController`
AND stream `EventMsg` events back to the client until the turn completes

#### Scenario: SubscribeSession RPC receives live events

WHEN a client calls `SubscribeSession` with `{ session_id: "abc" }`
THEN the server SHALL subscribe to the session's event stream
AND stream `EventMsg` events as they occur

### Requirement: GrpcEventBridge real implementation

`GrpcEventBridge::forward()` SHALL forward `StoredEventSnapshot` to the gRPC streaming service.

#### Scenario: GrpcEventBridge forwards snapshot

WHEN `GrpcEventBridge::forward(snapshot)` is called
THEN the snapshot SHALL be converted to `EventMsg` and sent to subscribed gRPC clients

### Requirement: synthia-grpc crate

A new `synthia-grpc` crate SHALL contain the proto definition, tonic service implementation, and server bootstrap.

#### Scenario: synthia-grpc compiles with tonic

WHEN `cargo check -p synthia-grpc` is run
THEN the crate SHALL compile successfully with tonic and prost dependencies
