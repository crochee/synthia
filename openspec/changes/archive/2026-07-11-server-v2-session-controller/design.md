## Context

Synthia 已经具备一个基于 Axum 的 HTTP server（`synthia-server`），支持 SSE/WebSocket 事件广播、Bearer token 认证、以及按 `user_id/session_id` 组织的磁盘持久化。但当前 server 无法作为多端共享的 agent 后端，因为：

1. **每个请求独立启动 agent run**：`POST /api/v1/chat` 和 WebSocket handler 每次都会创建新的 `Agent::run_stream`，同一个 session 可能被多个客户端同时触发多个并行的执行循环。
2. **控制入口缺失**：`steering`（用户干预）和 `cancel`（中断）能力已经存在于 `synthia-agent` 内部（`AgentRunConfig.steering_channel`、`session_input_queue`、`cancel_token`），但 server 没有暴露对应的 REST 端点。
3. **用户隔离不完整**：`synthia-session::SessionManager` 已经提供 `create_with_user(user_id)`，但 server 路由仍调用旧的 `create()`，且 list/get/delete 未按 user_id 过滤。
4. **事件不可重放**：现有 `EventBroadcaster` 只能向已连接的客户端推送实时事件，断线后无法从持久化日志恢复。

同时，P0 子代理变更 `p0-subagent-execution-session-persistence` 正在补齐 `SessionInputQueue` 的持久化与 steering 队列，为本次变更提供了直接基础。

本设计选择**保守增量路径**：在现有 server 骨架上引入 `SessionController`，新增 `events.jsonl` 持久化事件流，先解决多客户端共享与控制问题，再支撑后续子代理事件流（变更 B）。

## Goals / Non-Goals

**Goals:**

- 让 `synthia-server` 成为 TUI/CLI/Web/IDE 多端可共享的 session 后端。
- 保证同一会话同一时刻最多只有一个 `Agent::run_stream` 在执行。
- 通过 REST 暴露 prompt、steering、cancel 能力。
- 实现按 user_id 的完整 session 隔离。
- 实现基于游标的事件重放，支持客户端断线重连与多设备接力。
- 遵循 `api-design` 技能的 REST 规范：资源命名、HTTP 语义、envelope 响应、游标分页、标准错误格式。

**Non-Goals:**

- 不实现完整事件溯源 + SQLite 投影（保留 Synthia P10 “文件即记忆”原则）。
- 不重写 agent core 或 ReAct 循环。
- 不实现子代理事件流（留给变更 B）。
- 不实现跨 workspace / 跨机器的 session 迁移（如 OpenCode 的 sessionWarp）。
- 不引入新的前端 SDK 或 IDE 插件。

## Decisions

### D1: 引入 SessionController 作为每个活跃 session 的唯一执行控制器

- **选择**：在 `synthia-server` 中新增 `SessionController` 结构，每个活跃 session 持有一个实例，内部通过 `mpsc::Receiver<SessionOp>` 串行处理 `Prompt` / `Steer` / `Cancel` / `Shutdown` 等操作。
- **理由**：
  - 复用现有 `SessionInputQueue` 和 `EventBroadcaster`。
  - 最小化对 `synthia-agent` 的侵入。
  - 与 P0 子代理变更的 `session_input.jsonl` 持久化天然契合。
- **已考虑 alternative**：
  - Codex 式 `submission_loop`：需要重写 session/agent 交互边界，改造成本高。
  - OpenCode 式事件溯源 + 投影：与 Synthia 当前文件日志架构冲突，且会延迟多端能力的交付。

### D2: 新增 `events.jsonl`，保留现有 `messages.jsonl`

- **选择**：新增 append-only `events.jsonl` 记录所有 `AgentEvent`，`messages.jsonl` 保持原有格式作为 LLM 上下文来源。
- **理由**：
  - 不改现有恢复路径，兼容已存在的 session 数据。
  - `events.jsonl` 支持断线重放与多设备同步；`messages.jsonl` 继续直接服务 agent loop。
- **已考虑 alternative**：
  - 把 `messages.jsonl` 直接改成事件 envelope：需要重写所有读取/恢复逻辑。
  - 完全事件溯源：超出本次变更范围。

### D3: 列表端点使用游标分页

- **选择**：`GET /api/v2/sessions` 和 `GET /api/v2/sessions/{id}/messages` 使用 cursor-based 分页。
- **理由**：
  - `api-design` 技能推荐大数据集/流式场景使用游标。
  - sessions 和 messages 都是 append-only 或时间有序数据，游标性能稳定且不受并发插入影响。
- **已考虑 alternative**：
  - offset 分页：实现简单，但大 offset 性能差，且并发插入会导致页间重复或遗漏。

### D4: 异步控制操作返回 `202 Accepted`

- **选择**：`POST /prompts`、`POST /steering` 等触发或影响 agent run 的操作返回 `202`，不等待执行结果。
- **理由**：
  - agent 执行是长时间异步过程，同步阻塞 HTTP 连接不现实。
  - 客户端通过 `GET /events` SSE 获取结果。
- **已考虑 alternative**：
  - 长轮询：复杂度更高，且与已有 SSE 能力重复。

### D5: WebSocket 仅保留事件订阅，不直接启动 run

- **选择**：改造现有 WebSocket handler，使其只订阅 broadcaster，不再在收到文本消息时启动新的 `Agent::run_stream`。
- **理由**：
  - 避免多客户端通过 WebSocket 重复触发 run。
  - 控制命令统一走 REST，语义更清晰。
- **已考虑 alternative**：
  - 保持 WS 双向控制：会导致同一 session 存在多个控制入口，增加竞态风险。

### D6: V1 路由保持兼容但标记废弃

- **选择**：新增 V2 路由，V1 路由保持现有行为，响应头增加 `Deprecation: true`。
- **理由**：
  - 不破坏现有客户端。
  - 为后续移除 V1 的 `/chat` 一次性入口做准备。

## Risks / Trade-offs

- **[Risk] SessionController 进程内单点故障** → Mitigation: Controller 状态完全由磁盘 `metadata.json` + `session_input.jsonl` + `events.jsonl` 决定，崩溃后重启 server 可通过 `restore` 重建。
- **[Risk] 多个 server 实例同时访问同一份 session 文件** → Mitigation: 本次变更假设单 server 实例；多实例场景需要后续引入文件锁或分布式 ownership。
- **[Risk] events.jsonl 无限增长** → Mitigation: 未来（变更 B 之后）可加入 compaction/archiving，将旧事件压缩到 `events.{seq}.jsonl.gz`。
- **[Trade-off] 双写 messages.jsonl 和 events.jsonl** → 接受理由：messages 是 agent 的必需输入，events 是 UI/同步的观测数据，两者职责不同，双写带来的冗余换取了清晰的边界和兼容性。
- **[Trade-off] 不引入 SQLite 投影** → 接受理由：符合 Synthia “文件即记忆”原则，降低架构复杂度；当未来需要复杂查询时再引入索引层。

## Migration Plan

1. **Phase 1: 数据模型扩展**
   - 在 `SessionMetadata` 中新增 `title`、`controller_version` 字段（使用 `serde(default)` 保持兼容）。
   - 新增 `events.jsonl` 写入逻辑。

2. **Phase 2: SessionManager 用户隔离**
   - 将 `SessionManager::create` 调用改为 `create_with_user`。
   - 为 list/get/delete 增加 user_id 过滤。

3. **Phase 3: SessionController 实现**
   - 实现 `SessionController` 与 `SessionOp`。
   - 集成 `SessionInputQueue`、`EventBroadcaster`、`CancellationToken`。

4. **Phase 4: V2 REST API**
   - 实现 `/api/v2/sessions`、`/prompts`、`/steering`、`/cancel`、`/events`、`/messages` 端点。
   - 实现游标分页与标准错误响应。

5. **Phase 5: WebSocket 改造与废弃 V1**
   - 改造 WebSocket handler 为纯事件订阅。
   - 为 V1 路由增加 `Deprecation` 头。

6. **Phase 6: 测试与验证**
   - 单元测试：SessionController 状态机、游标编码、user_id 隔离。
   - 集成测试：多客户端 SSE 观察、断线重放、steering/cancel 闭环。

**Rollback**: 由于新增 V2 路由不改变 V1 行为，回滚只需停止暴露 V2 端点并回退 `synthia-server` 代码；`events.jsonl` 是新增文件，不影响已有 `messages.jsonl` 读取。

## Open Questions

1. `SessionController` 空闲超时时间应设为多少？（默认考虑 30 分钟无订阅且无 run 时 shutdown）
2. 是否需要在 `events.jsonl` 中持久化 `SyncCaughtUp` 这类 meta 事件？
3. V1 的 `POST /api/v1/chat` 是否需要在变更 A 中立即废弃，还是保留到变更 B 之后？
