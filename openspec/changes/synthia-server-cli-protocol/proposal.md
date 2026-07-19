# Proposal: synthia-server-cli-protocol

> Change #4 — Server/CLI/Protocol/MCP: gRPC streaming, server backpressure, wire protocol completion, MCP tools endpoint

## Why

Changes #1-#3 delivered infrastructure, main_loop integration, and tool business logic. The **server/transport layer** remains incomplete: `POST /submission` is a stub, there's no gRPC streaming, slow SSE/WS consumers lose events, the wire CLI is read-only, and synthia doesn't expose an MCP server endpoint for IDE integration.

## What Changes

1. **synthia-grpc crate** — New crate with `SynthiaService` gRPC proto (SubmitOp → stream EventMsg, SubscribeSession → stream EventMsg); REST handles CRUD, gRPC handles streaming
2. **Server backpressure** — Per-subscriber bounded mpsc channels (default 256) with overflow policy (drop oldest); replaces broadcast channel for SSE/WS subscribers
3. **Op → SessionOp adapter** — Direct mapping function in `synthia-protocol`; `POST /submission` handler routes to `SessionController::submit()`
4. **MCP tools endpoint** — HTTP+SSE MCP server exposing `tools/list` and `tools/call`; enables IDE/tool integration
5. **Interactive wire CLI** — stdin→Submission pipeline for interrupt/approval/steering; events remain JSON lines
6. **GrpcEventBridge implementation** — Upgrade from no-op stub to real tonic transport; connect EventBus to gRPC streaming

## Capabilities

### New Capabilities

| Capability | Description |
|------------|-------------|
| `grpc-streaming-service` | synthia-grpc crate + SynthiaService proto + SubmitOp/SubscribeSession RPCs + GrpcEventBridge implementation |
| `server-backpressure` | Per-subscriber bounded mpsc + overflow policy + SubscriberRegistry |
| `op-session-adapter` | Op → SessionOp mapping function + POST /submission routing to SessionController |
| `mcp-tools-endpoint` | MCP server tools/list + tools/call via HTTP+SSE transport |
| `interactive-wire-cli` | stdin→Submission pipeline + interrupt/approval/steering from wire client |

## Impact

- **New crate**: `synthia-grpc` (proto + tonic service + GrpcEventBridge impl)
- **Code**: `EventBroadcaster` refactor, `POST /submission` handler, wire.rs bidirectional, MCP server handler
- **Dependencies**: `tonic`, `prost` (via workspace), rmcp server features
- **API**: New gRPC endpoint alongside existing REST; MCP endpoint at `/mcp`
