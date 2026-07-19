## 1. Project Setup

- [x] 1.1 Create `crates/synthia-message-proxy/` crate with Cargo.toml
- [x] 1.2 Add dependencies: `tonic`, `prost`, `tokio`, `tracing`, `serde`
- [x] 1.3 Configure proto code generation (tonic-build)

## 2. Proto Definition

- [x] 2.1 Define `message_proxy.proto` with Send, Broadcast, Register, Subscribe RPCs
- [x] 2.2 Generate Rust code from proto

## 3. MessageProxy Server Implementation

- [x] 3.1 Implement `MessageProxyService` gRPC service handler
- [x] 3.2 Implement agent registry (in-memory map of agent_id -> tx)
- [x] 3.3 Implement `Send` handler (Point-to-Point delivery)
- [x] 3.4 Implement `Broadcast` handler (1:N delivery)
- [x] 3.5 Implement `Register` handler
- [x] 3.6 Implement `Subscribe` handler (streaming messages to agent)
- [x] 3.7 Add Unix Domain Socket listener startup
- [x] 3.8 Add environment variable configuration for socket path

## 4. Agent Client Implementation

- [x] 4.1 Create `MessageBusProxy` client struct
- [x] 4.2 Implement `MessageBus` trait for `MessageBusProxy`
- [x] 4.3 Connect to MessageProxy via gRPC channel
- [x] 4.4 Implement async send via `Send` RPC
- [x] 4.5 Implement broadcast via `Broadcast` RPC
- [x] 4.6 Implement subscription via `Subscribe` streaming
- [x] 4.7 Handle reconnection on disconnect

## 5. Integration with synthia-agent

- [x] 5.1 Replace `InMemoryMessageBus` with `MessageBusProxy` in agent_tools.rs
- [x] 5.2 Add `MESSAGE_PROXY_ADDR` environment variable reading
- [x] 5.3 Implement graceful fallback if MessageProxy is unavailable

## 6. Testing

- [x] 6.1 Write unit tests for MessageProxyService handlers
- [x] 6.2 Write integration test with two agents communicating via MessageProxy
- [x] 6.3 Test broadcast to multiple agents
- [x] 6.4 Test reconnection handling
