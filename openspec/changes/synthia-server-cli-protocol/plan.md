# Implementation Plan: synthia-server-cli-protocol

## Overview

**Change**: synthia-server-cli-protocol (Change #4)
**Goal**: gRPC streaming, server backpressure, wire protocol completion, MCP tools endpoint
**Total PRs**: 13 (across 7 groups)
**Estimated effort**: 3-4 sessions

## Execution Order

### Session 1: Backpressure + Op adapter (PRs 1.1-1.2, 2.1-2.2) — Mostly parallel
- PR 1.1: Per-subscriber mpsc + SubscriberRegistry
- PR 2.1: op_to_session_op mapping (independent)
- PR 1.2: SSE/WS use SubscriberRegistry (depends on 1.1)
- PR 2.2: POST /submission routing (depends on 2.1)

### Session 2: gRPC streaming (PRs 3.1-3.3) — Sequential
- PR 3.1: synthia-grpc crate skeleton + proto
- PR 3.2: SynthiaService tonic implementation (depends on 3.1)
- PR 3.3: GrpcEventBridge real impl (depends on 3.2)

### Session 3: MCP + wire CLI (PRs 4.1-4.2, 5.1-5.2) — Parallel tracks
- Track A: PR 4.1 → 4.2 (MCP tools endpoint)
- Track B: PR 5.1 → 5.2 (interactive wire CLI)

### Session 4: Quality gates + retrospective

## Risk Mitigation

1. **Proto version drift**: Generate proto from EventMsg/Op serde types; single source of truth
2. **gRPC server port conflict**: Configure via `SYNTHIA_GRPC_PORT` env var; default 50051
3. **MCP security**: Reuse existing PermissionChecker; MCP calls go through same approval flow
