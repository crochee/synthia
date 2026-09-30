# Synthia 接口契约并集表

> 生成时间: 2026-08-22T09:48:57.555Z
> 版本: 1

## 统计

- Total: 89
- Paired (双侧一致): 69
- Backend-only (后端提供, 前端未调用): 17
- Frontend-only (前端调用, 后端未注册): 3

## 端点

### DELETE `/api/commands/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:152`

### DELETE `/api/jobs/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:122`

### DELETE `/api/mcp/servers/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:140`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/McpPage.tsx:63`

### DELETE `/api/providers/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:100`

### DELETE `/api/skills/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:110`

### DELETE `/api/tools/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:148`

### DELETE `/api/v1/agents/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:169`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/AgentsPage.tsx:144`

### DELETE `/api/v1/commands/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:147`

### DELETE `/api/v1/jobs/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:119`

### DELETE `/api/v1/mcp/servers/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:130`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/McpPage.tsx:61`

### DELETE `/api/v1/skills/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:174`

### DELETE `/api/v1/tools/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:190`

### GET `/.well-known/agent-card.json`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:192`

### GET `/api/approvals`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:164`

### GET `/api/commands`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:150`

### GET `/api/commands/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:151`

### GET `/api/jobs`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:118`

### GET `/api/mcp/servers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:127`

### GET `/api/mcp/servers/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:132`

### GET `/api/memory/search`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:116`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/MemoryPage.tsx:21`

### GET `/api/models`

- **状态**: frontend
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:296`

### GET `/api/providers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:95`

### GET `/api/providers/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:100`

### GET `/api/settings`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:157`

### GET `/api/skills`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:106`

### GET `/api/skills/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:110`

### GET `/api/tools`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:145`

### GET `/api/tools/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:147`

### GET `/api/v1/agents`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:160`

### GET `/api/v1/agents/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:169`

### GET `/api/v1/agents/default`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:165`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/ChatPage.tsx:303`

### GET `/api/v1/approvals`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:159`

### GET `/api/v1/chat/agents/default`

- **状态**: frontend
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:178`

### GET `/api/v1/chat/sessions`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:115`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:202`

### GET `/api/v1/chat/sessions/{key}/history`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:120`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:191`

### GET `/api/v1/chat/sessions/{key}/messages/stream`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:128`

### GET `/api/v1/chat/usage`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:114`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:274`

### GET `/api/v1/commands`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:145`

### GET `/api/v1/commands/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:146`

### GET `/api/v1/jobs`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:115`

### GET `/api/v1/mcp/servers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:125`

### GET `/api/v1/mcp/servers/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:130`

### GET `/api/v1/memory/search`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:180`

### GET `/api/v1/models`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:99`

### GET `/api/v1/providers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:98`

### GET `/api/v1/providers/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:99`

### GET `/api/v1/sessions`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:101`

### GET `/api/v1/sessions/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:102`

### GET `/api/v1/settings`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:152`

### GET `/api/v1/skills`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:154`

### GET `/api/v1/skills/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:174`

### GET `/api/v1/tools`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:186`

### GET `/api/v1/tools/{key}`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:190`

### GET `/livez`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:210`

### GET `/messages/stream`

- **状态**: frontend
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:375`

### GET `/readyz`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:211`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/hooks/useServerHealth.ts:96`

### GET `/ws/approvals`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:172`

### PATCH `/api/v1/chat/sessions/{key}/messages/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:140`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:242`

### POST `/api/approvals/{key}/resolve`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:165`

### POST `/api/jobs`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:118`

### POST `/api/jobs/{key}/execute`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:123`

### POST `/api/jobs/{key}/pause`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:124`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/JobsPage.tsx:36`

### POST `/api/mcp`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:126`

### POST `/api/mcp/servers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:128`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/McpPage.tsx:46`

### POST `/api/mcp/servers/{key}/discover`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:136`

### POST `/api/providers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:95`

### POST `/api/skills`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:106`

### POST `/api/skills/reload`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:114`

### POST `/api/tools`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:146`

### POST `/api/v1/agents`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:160`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/AgentsPage.tsx:126`

### POST `/api/v1/approvals/{key}/resolve`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:160`

### POST `/api/v1/chat/messages/{key}/feedback`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:144`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:262`

### POST `/api/v1/chat/sessions`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:115`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:163`

### POST `/api/v1/chat/sessions/{key}/cancel`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:132`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:212`

### POST `/api/v1/chat/sessions/{key}/messages`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:124`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:351`

### POST `/api/v1/chat/sessions/{key}/regenerate`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:136`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/api/chat-stream.ts:224`

### POST `/api/v1/jobs`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:115`

### POST `/api/v1/jobs/{key}/execute`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:120`

### POST `/api/v1/jobs/{key}/pause`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:121`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/JobsPage.tsx:28`

### POST `/api/v1/jobs/{key}/resume`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:122`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/JobsPage.tsx:44`

### POST `/api/v1/mcp/rpc`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:124`

### POST `/api/v1/mcp/servers`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:125`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/McpPage.tsx:42`

### POST `/api/v1/mcp/servers/{key}/discover`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:135`

### POST `/api/v1/skills`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:154`

### POST `/api/v1/skills/reload`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:158`

### POST `/api/v1/tools`

- **状态**: backend
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:186`

### PUT `/api/settings`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/crates/synthia-server/src/server/router.rs:157`
- **前端来源**: `/home/crochee/workspace/synthia/.worktrees/synthia-interface-contract-closure-cycle-2/synthia-web/src/pages/SettingsPage.tsx:34`

### PUT `/api/v1/settings`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:152`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/SettingsPage.tsx:34`

### PUT `/api/v1/skills/{key}`

- **状态**: both
- **后端来源**: `/home/crochee/workspace/synthia/crates/synthia-server/src/server/router.rs:105`
- **前端来源**: `/home/crochee/workspace/synthia/synthia-web/src/pages/SkillsPage.tsx:27`
