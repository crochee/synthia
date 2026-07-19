# Brainstorm: synthia-server-cli-protocol (Change #4)

> Raw capture of brainstorming output for change #4 — server/CLI/protocol/MCP.

---

## Background

Changes #1-#3 delivered infrastructure, main_loop integration, and tool business logic. Change #4 focuses on the **server/transport layer** — gRPC streaming, MCP server endpoints, server backpressure, and wire protocol completion.

### Key Integration Gaps

| Gap | Description |
|-----|-------------|
| G1 | `POST /submission` is a dispatch stub — receives `SubmissionEnvelope` but only logs; doesn't route to `SessionController::submit()` |
| G2 | `EventBroadcaster` uses `tokio::sync::broadcast` (capacity 128) — slow consumers get Lagged, no backpressure |
| G3 | No gRPC streaming — only HTTP REST + SSE + WS; `GrpcEventBridge` is a no-op stub |
| G4 | `Op → SessionOp` adapter missing — `Op::UserInput` → `SessionOp::Prompt` mapping not implemented |
| G5 | Wire CLI is read-only — cannot send interactive Submissions (interrupt, approval, steering) |
| G6 | No MCP server endpoint — synthia only connects to external MCP servers as client; doesn't expose tools/resources to external consumers |
| G7 | `ForwardToMainAgent` only works in-process — cross-process sub→parent forwarding not available |
| G8 | `EventBus::emit` and `EventBroadcaster::send` are independent pipelines, not bridged |

---

## Decision Chain

### Q1: gRPC streaming — extend message-proxy or new service?

**Options**:

1. **Extend message-proxy**: Add `SubmitOp(Submission) → stream EventMsg` RPC to the existing proto.
   - ✅ Reuses existing UDS infrastructure
   - ❌ message-proxy is designed for inter-agent messaging, not client→server submission

2. **New SynthiaServer gRPC service**: Separate proto with `SynthiaService` RPCs alongside existing HTTP server.
   - ✅ Clean separation of concerns
   - ❌ Two transport layers to maintain; proto code generation overhead

3. **Hybrid**: HTTP REST for CRUD, gRPC for streaming only. New `synthia-grpc` crate.
   - ✅ Best of both worlds — REST for tools, gRPC for streams
   - ✅ Matches opencode's architecture (REST + gRPC dual transport)

**Decision (D1)**: **Option 3** — New `synthia-grpc` crate. HTTP REST handles CRUD (sessions, providers, skills, approvals). gRPC handles streaming (submit op → stream events, subscribe to session events). The `GrpcEventBridge` connects to this service.

### Q2: Server backpressure mechanism?

**Options**:

1. **Bounded mpsc per subscriber**: Replace broadcast with per-subscriber mpsc channels with configurable buffer. When buffer full, apply backpressure (pause event production).
   - ✅ No event loss
   - ❌ One slow subscriber blocks the whole pipeline

2. **Bounded mpsc + per-subscriber overflow policy**: Each subscriber gets a bounded channel. When full, either drop oldest (ring buffer) or apply per-subscriber backpressure without blocking others.
   - ✅ Isolates slow subscribers
   - ❌ More complex than broadcast

3. **gRPC flow control only**: Drop broadcast; use gRPC's built-in flow control for streaming clients. SSE/WS clients get broadcast with Lagged semantics (acceptable for monitoring).
   - ✅ Leverages gRPC's built-in mechanism
   - ❌ SSE/WS clients still lose events under load

**Decision (D2)**: **Option 2** — Per-subscriber bounded mpsc + overflow policy. Default buffer 256 events per subscriber. When full, drop oldest (ring buffer). gRPC streaming uses its own flow control. SSE/WS clients use the same per-subscriber channels.

### Q3: Op → SessionOp adapter?

**Options**:

1. **Direct mapping function**: `fn op_to_session_op(Op) -> Result<SessionOp, ProtocolError>` in `synthia-protocol`.
   - ✅ Simple, testable
   - ❌ Bidirectional mapping needed (SessionOp → EventMsg)

2. **ProtocolAdapter trait**: Abstract bidirectional adapter with `op_to_session_op()` and `event_to_event_msg()`.
   - ✅ Extensible for future protocol versions
   - ❌ Over-engineering for current needs

3. **Direct mapping + trait**: Function for simple mappings, trait for version negotiation.

**Decision (D3)**: **Option 1** — Direct mapping function. Simple and testable. The `POST /submission` handler calls `op_to_session_op()` then `SessionController::submit()`. Version negotiation is handled by `PROTOCOL_VERSION` header.

### Q4: MCP server endpoint?

**Options**:

1. **Full MCP server**: Implement complete MCP server spec (tools/list, tools/call, resources/list, prompts/list, sampling, etc.)
   - ❌ Very large scope; MCP server spec is extensive

2. **MCP tools endpoint only**: Expose synthia's tool list and tool execution via MCP protocol. No resources, prompts, or sampling.
   - ✅ Focused, useful for IDE/tool integration
   - ✅ Matches the primary use case (external tools calling synthia)

3. **Defer**: Don't implement MCP server in change #4; focus on gRPC + backpressure first.

**Decision (D4)**: **Option 2** — MCP tools endpoint only. Implement `tools/list` and `tools/call` via HTTP+SSE transport (matching MCP spec). This enables IDE integration (VS Code, Cursor) to call synthia tools directly. Full MCP server (resources, prompts, sampling) deferred.

### Q5: Wire CLI bidirectional interaction?

**Options**:

1. **Full TUI over wire**: Implement the same REPL experience over wire (stdin→Submission, events→formatted output).
   - ✅ Complete experience
   - ❌ Large scope; TUI rendering over wire is complex

2. **Interactive wire mode**: Add interrupt/approval/steering submission support. Events remain JSON lines.
   - ✅ Focused on interaction gaps
   - ✅ Matches opencode's `--wire` mode

3. **Defer**: Keep wire mode read-only for change #4.

**Decision (D5)**: **Option 2** — Interactive wire mode. Add `Op::Interrupt`, `Op::ApprovalResponse`, `Op::UserInput` submission from stdin. Events remain JSON lines for simplicity. TUI rendering over wire is future work.

### Q6: EventBus → EventBroadcaster bridge?

**Options**:

1. **EventBusBridge → EventBroadcaster**: `MpscEventBridge` forwards to `EventBroadcaster::send()`.
   - ✅ Single event pipeline
   - ❌ EventBus is typed (`EventEnvelope<T>`), EventBroadcaster is untyped (`AgentEvent`)

2. **Unified event pipeline**: Replace `EventBroadcaster` with `EventBus` as the single source of truth.
   - ✅ Clean architecture
   - ❌ Large refactor; EventBus is async, EventBroadcaster is sync broadcast

3. **Keep separate, document relationship**: EventBus for durability/projection; EventBroadcaster for real-time SSE/WS. EventBus → EventBroadcaster forwarding is optional.

**Decision (D6)**: **Option 3** — Keep separate, document relationship. EventBus handles durability (SQLite sink) and projection. EventBroadcaster handles real-time SSE/WS. A future change can unify them when EventBus is mature enough to replace broadcast. For now, the `GrpcEventBridge` connects EventBus to the gRPC streaming layer.

---

## Design Trade-offs Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| D1 | New synthia-grpc crate (hybrid REST+gRPC) | Clean separation, matches opencode architecture |
| D2 | Per-subscriber mpsc + overflow policy | Isolates slow subscribers, no event loss for gRPC |
| D3 | Direct Op → SessionOp mapping function | Simple, testable |
| D4 | MCP tools endpoint only (tools/list + tools/call) | Focused, enables IDE integration |
| D5 | Interactive wire mode (interrupt/approval/steering) | Closes interaction gap without TUI |
| D6 | Keep EventBus + EventBroadcaster separate | Avoid large refactor, document relationship |

---

## Out of Scope

- Full MCP server (resources, prompts, sampling) — future change
- TUI rendering over wire — future change
- gRPC authentication (mTLS, JWT) — future change after basic streaming works
- EventBus replacing EventBroadcaster — evaluate after EventBus has SQLite durability
- Deprecated `AgentHook`/`HookRunner` removal — 6-month window from change #2
