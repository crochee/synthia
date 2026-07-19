<!--
Raw capture of brainstorming output.

本檔原樣捕捉 brainstorming 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: Server V2 Session Controller

## Background

Synthia 当前已经具备一个 Axum-based server（`synthia-server`），支持 SSE/WebSocket 事件广播、Bearer token 认证、session 磁盘持久化。但与 OpenCode/Codex 等生产级 agent 相比，存在三个核心差距：

1. **多客户端无法共享同一个 session 控制器**：当前 `chat_handler` 和 `ws_handler` 每次请求都会新建一个 `Agent::run_stream`，导致同一个 session 可能同时运行多个 agent loop。
2. **steering / cancel 没有 HTTP 入口**：`AgentRunConfig` 已经有 `steering_channel`、`session_input_queue`、`cancel_token`，但 server 没有暴露对应的 REST 端点。
3. **用户隔离不完整**：`SessionManager::create_with_user` 已存在，但 server 仍用旧 `create()`；list/get/delete 未按 user_id 过滤。

同时，P0 子代理变更 `p0-subagent-execution-session-persistence` 正在补齐 session 持久化与 steering 队列，为本次变更提供了基础。

## Decision Chain

### Q1: Synthia 的目标形态是什么？
**Answer**: 走 OpenCode 的多端路线（TUI、Web、Desktop、VS Code 插件），因此 server API 必须成为多端共享的后端。

### Q2: 当前最痛的瓶颈是什么？
**Answer**: 能力（多代理/同步）。成本/缓存和安全也有差距，但先解决多客户端共享与控制能力才能支撑多端。

### Q3: 是否先停留在探索阶段？
**Answer**: 先探索，后收敛为变更 A（Server V2 Session Controller），变更 B（Subagent Event Streaming）在 A 之后。

### Q4: 选择哪种架构范式？
**Answer**: 采用“保守增量”方案（选项 1）：
- 不引入 OpenCode 的完整事件溯源 + SQLite 投影（与 Synthia P10 “文件即记忆”原则冲突，且改造成本高）。
- 不引入 Codex 的 JSON-RPC + app-server 控制平面（Synthia 已有 HTTP/SSE 骨架，重写不划算）。
- 在现有 `synthia-server` 上增加 `SessionController`，串行化 prompt/steer/cancel；新增 `events.jsonl` 用于事件持久化与重放；保留 `messages.jsonl` 作为 LLM 上下文来源。

## Design Trade-offs

### Approach A: 轻量 SessionController（推荐）
- **Pros**: 风险低，不改 agent core，复用现有 `SessionInputQueue` 和 `EventBroadcaster`。
- **Cons**: 仍是文件日志，未来扩展到跨 workspace 迁移时需要进一步升级。

### Approach B: 完整事件溯源
- **Pros**: 与 OpenCode 对齐，支持 workspace warp、回放、投影。
- **Cons**: 需要把 `messages.jsonl` 改成事件流，所有恢复路径重构，与 P0 变更冲突风险大。

### Approach C: Codex 式单循环 + ThreadStore
- **Pros**: 并发安全简单，存储抽象清晰。
- **Cons**: 需要重写 session/agent 交互边界，与现有 `Agent::run_stream` 模型差异大。

**Selected**: Approach A，作为下一步的合理简洁路径。

## Additional Constraints

- API 设计遵循 `api-design` 技能：资源命名、HTTP 方法语义、envelope 响应、游标分页、标准错误格式。
- 列表端点必须使用游标分页（cursor-based）。
- 所有 V2 端点必须校验 user_id 并实现 session 隔离。
- 变更 A 不实现子代理事件流，但接口和事件模型要为变更 B 预留扩展点。
