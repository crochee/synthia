# Synthia 接口契约并集表

> 生成时间: 2026-07-25T13:21:58.803Z
> 版本: 1

## 统计

- Total: 39
- Paired (双侧一致): 4
- Backend-only (后端提供, 前端未调用): 33
- Frontend-only (前端调用, 后端未注册): 2

## 端点

### DELETE `/api/commands/{name}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:152`

### DELETE `/api/jobs/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:122`

### DELETE `/api/mcp/servers/`

- **状态**: frontend
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/synthia-web/src/pages/McpPage.tsx:63`

### DELETE `/api/mcp/servers/{id}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:140`

### DELETE `/api/providers/{name}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:100`

### DELETE `/api/skills/{name}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:110`

### DELETE `/api/tools/{name}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:148`

### GET `/.well-known/agent-card.json`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:197`

### GET `/api/approvals`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:164`

### GET `/api/commands`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:150`

### GET `/api/commands/{name}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:151`

### GET `/api/jobs`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:118`

### GET `/api/mcp/servers`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:127`

### GET `/api/mcp/servers/{id}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:132`

### GET `/api/memory/search`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:116`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/synthia-web/src/pages/MemoryPage.tsx:21`

### GET `/api/models`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:91`

### GET `/api/providers`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:95`

### GET `/api/providers/{name}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:100`

### GET `/api/settings`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:157`

### GET `/api/skills`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:106`

### GET `/api/skills/{name}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:110`

### GET `/api/tasks`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:93`

### GET `/api/tools`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:145`

### GET `/api/tools/{name}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:147`

### GET `/health`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:196`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/synthia-web/src/hooks/useServerHealth.ts:23`

### GET `/ws/approvals`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:177`

### POST `/api/approvals/{id}/resolve`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:165`

### POST `/api/jobs`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:118`

### POST `/api/jobs/{key}/execute`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:123`

### POST `/api/jobs/{key}/pause`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:124`

### POST `/api/mcp`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:126`

### POST `/api/mcp/servers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:128`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/synthia-web/src/pages/McpPage.tsx:46`

### POST `/api/mcp/servers/{id}/discover`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:136`

### POST `/api/providers`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:95`

### POST `/api/skills`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:106`

### POST `/api/skills/reload`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:114`

### POST `/api/tools`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:146`

### POST `/pause`

- **状态**: frontend
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/synthia-web/src/pages/JobsPage.tsx:36`

### PUT `/api/settings`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/crates/synthia-server/src/server/router.rs:157`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure/synthia-web/src/pages/SettingsPage.tsx:34`
