# Synthia 接口契约并集表

> 生成时间: 2026-07-25T14:47:24.971Z
> 版本: 1

## 统计

- Total: 38
- Paired (双侧一致): 7
- Backend-only (后端提供, 前端未调用): 31
- Frontend-only (前端调用, 后端未注册): 0

## 端点

### DELETE `/api/commands/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:152`

### DELETE `/api/jobs/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:122`

### DELETE `/api/mcp/servers/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:140`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/McpPage.tsx:63`

### DELETE `/api/providers/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:100`

### DELETE `/api/skills/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:110`

### DELETE `/api/tools/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:148`

### GET `/.well-known/agent-card.json`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:197`

### GET `/api/approvals`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:164`

### GET `/api/commands`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:150`

### GET `/api/commands/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:151`

### GET `/api/jobs`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:118`

### GET `/api/mcp/servers`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:127`

### GET `/api/mcp/servers/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:132`

### GET `/api/memory/search`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:116`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/MemoryPage.tsx:21`

### GET `/api/models`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:91`

### GET `/api/providers`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:95`

### GET `/api/providers/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:100`

### GET `/api/settings`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:157`

### GET `/api/skills`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:106`

### GET `/api/skills/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:110`

### GET `/api/tasks`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:93`

### GET `/api/tools`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:145`

### GET `/api/tools/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:147`

### GET `/health`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:196`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/hooks/useServerHealth.ts:23`

### GET `/ws/approvals`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:177`

### message:send `/a2a/message:send`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:208`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/a2a-client.ts:23`, `/home/crochee/workspace/synthia/synthia-web/src/api/a2a-stream.ts:117`
- **Note**: Fix card #002. The REST endpoint is served by the A2A JSON-RPC
router mounted under `/a2a` via `nest_service` in
`crates/synthia-server/src/server/router.rs`; the scanner
cannot see it directly because the route table lives in the
`a2a-server-lf` crate. Frontend payload is built by
`synthia-web/src/api/a2a-client.ts` and `a2a-stream.ts` via
`Message.fromJSON({messageId, contextId, role, parts})`, all
camelCase per `@a2a-js/sdk@1.0.0`. Per ARBITRATION.md
priority 2 (SDK types > Synthia stable spec) the wire shape
is fixed; backend accepts both `messageId` and `message_id`
at deserialise time (see a2a-pb protojson serde).


### POST `/api/approvals/{key}/resolve`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:165`

### POST `/api/jobs`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:118`

### POST `/api/jobs/{key}/execute`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:123`

### POST `/api/jobs/{key}/pause`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:124`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/JobsPage.tsx:36`

### POST `/api/mcp`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:126`

### POST `/api/mcp/servers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:128`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/McpPage.tsx:46`

### POST `/api/mcp/servers/{key}/discover`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:136`

### POST `/api/providers`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:95`

### POST `/api/skills`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:106`

### POST `/api/skills/reload`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:114`

### POST `/api/tools`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:146`

### PUT `/api/settings`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:157`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/SettingsPage.tsx:34`
