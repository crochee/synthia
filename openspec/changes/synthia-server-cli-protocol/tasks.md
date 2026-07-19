# Tasks — synthia-server-cli-protocol (Change #4)

> **Scope**: change #4 — server/CLI/protocol/MCP
> **Pre-condition**: changes #1-#3 completed

---

## 0. Pre-flight

### Task 0.1: cargo baseline check

---

## 1. server-backpressure (PR-1.1 ~ PR-1.2)

### Task 1.1: PR-1.1 — Per-subscriber mpsc channels + SubscriberRegistry

- **WHERE**: `crates/synthia-server/src/event_stream.rs`
- **HOW**: replace `broadcast` with `SubscriberRegistry` managing per-subscriber `mpsc::Sender` channels; configurable buffer size (default 256); overflow policy: drop oldest
- **EXPECTED**: subscribe/unsubscribe test + slow-subscriber isolation test pass

### Task 1.2: PR-1.2 — SSE/WS routes use SubscriberRegistry

- **WHERE**: `crates/synthia-server/src/routes/` (SSE + WS handlers)
- **HOW**: update SSE stream and WS event handlers to use `SubscriberRegistry::subscribe()` instead of `broadcast::subscribe()`
- **EXPECTED**: existing SSE/WS tests pass with new subscriber model

---

## 2. op-session-adapter (PR-2.1 ~ PR-2.2)

### Task 2.1: PR-2.1 — op_to_session_op mapping function

- **WHERE**: `crates/synthia-protocol/src/adapter.rs` (new)
- **HOW**: `fn op_to_session_op(op: Op) -> Result<SessionOp, ProtocolError>` with mapping for UserInput→Prompt, Interrupt→Cancel, ApprovalResponse→ApprovalDecision, Compact→Compact, RefreshTools→RefreshTools; UnsupportedOp error for unmapped Ops
- **EXPECTED**: 5 mapping tests + 1 unsupported error test pass

### Task 2.2: PR-2.2 — POST /submission routes to SessionController

- **WHERE**: `crates/synthia-server/src/routes/submission.rs`
- **HOW**: replace stub with `op_to_session_op()` + `SessionController::submit()` call; return 202 on success, 400 on mapping error, 500 on controller error
- **EXPECTED**: valid submission test + unsupported op test + controller error test pass

---

## 3. grpc-streaming-service (PR-3.1 ~ PR-3.3)

### Task 3.1: PR-3.1 — synthia-grpc crate skeleton + proto

- **WHERE**: `crates/synthia-grpc/` (new)
- **HOW**: scaffold crate with `synthia.proto` defining `SynthiaService` (SubmitOp, SubscribeSession RPCs) + message types mirroring `EventMsg`/`Op`; tonic + prost dependencies; build.rs for proto compilation
- **EXPECTED**: `cargo check -p synthia-grpc` exits 0

### Task 3.2: PR-3.2 — SynthiaService tonic implementation

- **WHERE**: `crates/synthia-grpc/src/service.rs`
- **HOW**: implement `SynthiaService` trait; `SubmitOp` calls `op_to_session_op()` + `SessionController::submit()` + subscribes to event stream; `SubscribeSession` subscribes to session events; stream `EventMsg` via tonic `Streaming`
- **EXPECTED**: SubmitOp streaming test + SubscribeSession test pass

### Task 3.3: PR-3.3 — GrpcEventBridge real implementation

- **WHERE**: `crates/synthia-event-v2/src/bridge.rs`
- **HOW**: upgrade `GrpcEventBridge` from no-op to real tonic transport; `forward()` sends `StoredEventSnapshot` (converted to `EventMsg`) to the gRPC service's event stream
- **EXPECTED**: GrpcEventBridge integration test (emit → bridge → gRPC → client receives) pass

---

## 4. mcp-tools-endpoint (PR-4.1 ~ PR-4.2)

### Task 4.1: PR-4.1 — MCP tools/list handler

- **WHERE**: `crates/synthia-server/src/routes/mcp.rs` (new)
- **HOW**: implement JSON-RPC `tools/list` handler; query `ScopedToolRegistry` for available tools; return MCP format (name, description, inputSchema)
- **EXPECTED**: tools/list returns correct tool list test pass

### Task 4.2: PR-4.2 — MCP tools/call handler

- **WHERE**: `crates/synthia-server/src/routes/mcp.rs`
- **HOW**: implement JSON-RPC `tools/call` handler; route through `ToolOrchestrator` + `PermissionChecker`; return tool result or approval-required error; HTTP+SSE transport (POST + SSE)
- **EXPECTED**: tools/call execution test + approval-required test pass

---

## 5. interactive-wire-cli (PR-5.1 ~ PR-5.2)

### Task 5.1: PR-5.1 — stdin→Submission pipeline

- **WHERE**: `crates/synthia-cli/src/wire.rs`
- **HOW**: add stdin reader task that sends `Submission { op: Op::UserInput { content } }` on Enter; Ctrl+C sends `Op::Interrupt`; display events as JSON lines
- **EXPECTED**: stdin submission test + interrupt test pass

### Task 5.2: PR-5.2 — Approval + steering from wire CLI

- **WHERE**: `crates/synthia-cli/src/wire.rs`
- **HOW**: on `ApprovalRequest` event, prompt `[y/n/a]: ` and send `Op::ApprovalResponse`; `/steer` prefix sends steering message
- **EXPECTED**: approval response test + steering test pass

---

## 6. Quality gates

### Task 6.1: cargo fmt + clippy
### Task 6.2: cargo test split per-module
### Task 6.3: OpenSpec validation

---

## 7. Docs + retrospective

### Task 7.1: retrospective.md
