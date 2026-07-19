# Design: synthia-server-cli-protocol (Change #4)

## Context

The server layer has HTTP REST + SSE + WS but no gRPC streaming. `POST /submission` is a stub. EventBroadcaster uses broadcast channel with Lagged semantics (slow consumers lose events). Wire CLI is read-only. No MCP server endpoint.

### Constraints from Changes #1-#3

- `GrpcEventBridge` stub exists (PR-1.5) — change #4 must implement the real transport
- `EventBus` trait exists with `emit()` — can forward to EventBroadcaster or gRPC
- `AgentEvent::Custom` and `project_custom_event` exist (PR-7.1/7.3) — wire protocol must project Custom events
- `ForwardToMainAgent` works in-process (change #2) — cross-process forwarding needs message-proxy or gRPC
- `ToolId` on `ToolCallRequest`/`ToolCallResult` (change #3) — wire protocol must carry ToolId

---

## Goals / Non-Goals

### Goals

1. Implement `GrpcEventBridge` with real tonic transport
2. Replace broadcast channel with per-subscriber mpsc + overflow policy
3. Implement Op → SessionOp adapter and wire POST /submission to SessionController
4. Add MCP tools endpoint (tools/list + tools/call) via HTTP+SSE
5. Add interactive wire CLI (interrupt/approval/steering from stdin)

### Non-Goals

- Full MCP server (resources, prompts, sampling)
- TUI rendering over wire
- gRPC authentication (mTLS, JWT)
- EventBus replacing EventBroadcaster
- Removing deprecated AgentHook/HookRunner (6-month window)

---

## Decisions

### D1: synthia-grpc crate (hybrid REST+gRPC)

REST handles CRUD (sessions, providers, skills, approvals). gRPC handles streaming (SubmitOp → stream EventMsg, SubscribeSession → stream EventMsg). New `synthia-grpc` crate with proto definition and tonic service implementation.

### D2: Per-subscriber bounded mpsc + overflow policy

Each SSE/WS subscriber gets a bounded mpsc channel (default 256 events). When full, drop oldest (ring buffer). gRPC uses its own flow control. `SubscriberRegistry` manages subscriptions.

### D3: Direct Op → SessionOp mapping function

`fn op_to_session_op(Op) -> Result<SessionOp, ProtocolError>` in `synthia-protocol`. The POST /submission handler calls this then routes to SessionController::submit().

### D4: MCP tools endpoint only

Implement `tools/list` and `tools/call` via HTTP+SSE transport matching MCP spec. Enables IDE integration. Full MCP server deferred.

### D5: Interactive wire CLI

Add interrupt/approval/steering submission from stdin. Events remain JSON lines. TUI rendering deferred.

### D6: EventBus + EventBroadcaster separate

EventBus for durability/projection, EventBroadcaster for real-time SSE/WS. Document relationship. GrpcEventBridge connects EventBus to gRPC streaming.

---

## Risks / Trade-offs

| Risk | Severity | Mitigation |
|------|----------|------------|
| R1: gRPC proto version drift with protocol crate | High | Generate proto from `EventMsg`/`Op` serde types; single source of truth in protocol crate |
| R2: Per-subscriber mpsc memory overhead | Medium | Default 256 * N_subscribers; monitor; configurable buffer size |
| R3: MCP tools endpoint security | Medium | Same permission checks as REST API; MCP calls go through PermissionChecker |
| R4: Wire CLI interrupt timing | Low | Interrupt is best-effort (sends Cancel to SessionController); may miss if session already ended |

---

## Migration Plan

1. **Phase 1 (PRs 1-3)**: Backpressure + Op adapter — EventBroadcaster refactor, op_to_session_op, POST /submission routing
2. **Phase 2 (PRs 4-6)**: gRPC streaming — synthia-grpc crate, proto, SynthiaService, GrpcEventBridge impl
3. **Phase 3 (PRs 7-8)**: MCP + wire CLI — tools/list+call endpoint, interactive wire mode
4. **Phase 4 (PRs 9-10)**: Quality gates + retrospective
