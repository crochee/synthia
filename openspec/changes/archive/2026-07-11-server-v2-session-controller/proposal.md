## Why

Synthia 的 server 已经具备 HTTP/SSE 骨架，但当前 `synthia-server` 无法成为多端共享的 agent 后端：每次 chat 或 WebSocket 连接都会新建一个 `Agent::run_stream`，导致同一个 session 可能同时运行多个执行循环；steering 和 cancel 能力虽已在 agent 内部实现，却未暴露给 HTTP 客户端；session 创建也未绑定 user_id，多用户隔离不完整。随着 P0 子代理变更补齐 `SessionInputQueue` 的持久化，现在是把 server 升级为生产级多客户端控制平面的合适时机。

## What Changes

**Session 生命周期与控制**
- From: V1 `POST /api/v1/sessions` 调用 `SessionManager::create()`，不绑定 user_id；`POST /api/v1/sessions/{id}/messages` 仅返回确认，不入队也不触发 agent。
- To: 新增 V2 `/api/v2/sessions` 系列端点，创建时绑定 user_id；`/prompts` 真正写入 `SessionInputQueue` 并触发 `SessionController`；`/steering` 发送高优先级干预；`/cancel` 中断当前 run。
- Reason: 让多个客户端（TUI/CLI/Web/IDE）共享同一个 session 执行器，避免并行 run 冲突。
- Impact: 新增 V2 API，V1 保持兼容但标记废弃；agent core 不改动。

**SessionController 执行模型**
- From: 每个 HTTP/WS 请求独立 spawn `Agent::run_stream`。
- To: 每个活跃 session 有一个 `SessionController`，通过 `mpsc` 串行处理 `Prompt`/`Steer`/`Cancel`/`Shutdown`，保证同 session 单 run。
- Reason: 统一执行入口，支持多客户端观察与接力。
- Impact: 新增 `synthia-server` 内部模块，复用 `synthia-session` 和 `synthia-agent` 已有能力。

**事件持久化与重放**
- From: `EventBroadcaster` 仅向当前连接客户端推送实时事件，断线即丢失历史。
- To: 新增 append-only `events.jsonl`，SSE `/events?last_seq=N` 支持从任意 seq 重放历史事件再切换实时流。
- Reason: 支持断线重连与多设备接力。
- Impact: 新增磁盘文件，不影响已有 `messages.jsonl`。

**用户隔离**
- From: `list_sessions` 返回全局 session；`get_session` 未校验 user_id。
- To: 所有 V2 端点按 token 派生的 user_id 过滤；session 磁盘路径与内存索引均按用户隔离。
- Reason: 生产级多用户安全。
- Impact: V2 路由独占，V1 行为不变。

## Capabilities

### New Capabilities

- `v2-session-api`: 新增 V2 REST 端点，用于 session 创建、列表、详情、删除、prompt、steering、cancel、events SSE、messages 查询。
- `session-controller`: 每个活跃 session 的唯一执行控制器，串行化 prompt/steer/cancel，保证单 run。
- `event-persistence`: 将 `AgentEvent` 持久化到 `events.jsonl`，并支持基于 `last_seq` 的 SSE 重放。
- `user-session-isolation`: 基于 token 派生 user_id，实现 session 创建、列表、读取、删除的用户隔离。
- `cursor-pagination`: 为 `GET /api/v2/sessions` 和 `GET /api/v2/sessions/{id}/messages` 提供游标分页。

### Modified Capabilities

- 无现有 spec 的 REQUIREMENTS 需要修改。V1 路由行为保持不变。

## Impact

- **代码**: 主要影响 `crates/synthia-server`；少量扩展 `crates/synthia-session`（`SessionMetadata` 字段、`list_for_user` 等）。
- **API**: 新增 `/api/v2/*` 端点；V1 增加 `Deprecation: true` 响应头。
- **数据**: 每个 session 目录新增 `events.jsonl`；`metadata.json` 新增可选字段。
- **依赖**: 无新增外部 crate；复用现有 `tokio::sync`、`tokio_util::sync::CancellationToken`、Axum。
