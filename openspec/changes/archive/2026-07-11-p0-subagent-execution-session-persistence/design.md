## Context

Synthia 当前缺少两个关键生产级能力，与 OpenCode 和 Codex 存在显著架构差距：

1. **子智能体执行循环未实现** — `AgentTool::call()` 创建 `AgentInstance` 并注册到 message bus，但从未实际启动子智能体的 ReAct 执行循环。存在两套并行的 `AgentInstance` 类型（`registry/` 和 `tools/agent_tools/`），`AgentControl` 在主循环中被忽略，`Mailbox::send_message()` 是显式存根。

2. **会话状态仅内存** — `LoopContext` 有 7 个关键字段（`end_reason`、`cumulative_tokens`、`context_token_limit` 等）完全不持久化。转向 channel 是纯内存 `tokio::mpsc`，进程重启后丢失所有未消费的转向消息。

约束：
- 必须遵循现有 Rust 编码规范（`cargo +nightly fmt --all`、`cargo clippy --all-targets --all-features --tests --all`）
- 新增 `Message` 字段必须使用 `serde(default)` + `..Default::default()` 模式保证向后兼容
- 不可引入新的外部依赖（如 SQLite/sqlx）— 仅使用现有 JSON/JSONL 文件系统存储
- 不破坏现有 2300+ 测试

## Goals / Non-Goals

**Goals:**
- 实现子智能体的完整执行循环：spawn → run → collect result
- 支持前台（阻塞等待）和后台（fire-and-forget）两种执行模式
- 统一两套并行的 `AgentInstance` 类型
- 接线 `AgentControl` 和 `Mailbox` 到主执行循环
- 持久化 `LoopContext` 中恢复执行所需的关键字段
- 新增 `SessionInputQueue` 持久化转向消息队列

**Non-Goals:**
- 不实现子智能体间的消息传递（send_message 工具）
- 不实现 Team 管理（create_team/delete_team）
- 不引入 SQLite 或向量数据库
- 不改变现有工具注册和执行机制
- 不修改 Session 的 JSONL 消息格式
- 不实现操作系统级沙箱

## Decisions

### D1：子智能体执行模式 — 采用 OpenCode 的 foreground/background 二分

- **选择**：`AgentTool::call()` 支持 `background: bool` 参数。默认 `false`（前台），spawn 后 await 结果；`true`（后台），spawn 后立即返回 "running" 状态，结果通过 Mailbox 异步注入。
- **理由**：OpenCode 的 `task` 工具已证明此模式在生产中有效。前台模式简单直观（RPC 风格），后台模式适合长时间任务。Synthia 的 `ForkPolicy` 设计可在此基础上提供更丰富的上下文继承。
- **已考虑 alternative**：Codex 的 actor 模式（spawn + parent poll/wait）。拒绝了，因为更复杂，父 agent 需要显式管理子 agent 生命周期。可通过 `send_message` 工具后续添加。

### D2：配置继承 — 采用 Codex 的配置快照继承

- **选择**：子智能体从父 `AgentRunConfig` 继承 model、provider、token_budget、permission_policy（降级为 User 层）。应用 `ForkPolicy` 过滤历史消息。覆盖 subagent_type 指定的 tools/denied_tools。
- **理由**：Codex 的 `build_agent_shared_config()` 从活跃 Turn 继承配置快照（而非从持久化配置），确保子智能体使用父智能体当前运行时的设置。Synthia 已有 `ForkPolicy` 设计（6 种上下文分叉策略），与配置继承互补。
- **已考虑 alternative**：OpenCode 的干净会话（子智能体仅获得 task prompt）。拒绝了，因为 Synthia 的 `ForkPolicy` 已设计好更丰富的上下文继承，不应浪费。

### D3：AgentInstance 统一 — 合并到单一类型

- **选择**：将 `registry::instance::AgentInstance` 和 `tools::agent_tools::coordinator::AgentInstance` 合并为单一的 `AgentInstance` 类型，包含执行所需全部字段（definition, session, token_budget, state, parent_id, fork_policy, result_tx）。
- **理由**：两套类型各有部分字段，都不完整。统一后消除歧义，减少维护成本。`registry/` 类型有更多结构化字段，`coordinator/` 类型更简单。合并后取两者的并集。
- **已考虑 alternative**：保留两套类型，通过 trait 统一。拒绝了，因为两套类型的用途重叠，trait 抽象增加复杂度而无实际收益。

### D4：结果通道 — tokio oneshot channel

- **选择**：每个子智能体实例携带 `tokio::sync::oneshot::Sender<AgentResult>`。前台模式 `AgentTool::call()` await 此 receiver。后台模式 receiver 传给 Mailbox，在子智能体完成时将结果作为合成用户消息注入父会话。
- **理由**：oneshot 是最轻量的结果传递机制，语义精确（一次发送，一次接收）。不需要 tokio watch channel 的持续订阅（当前阶段不需要中途向子智能体发消息）。
- **已考虑 alternative**：tokio watch channel（Codex 模式）。拒绝，因为当前不需要中途交互，oneshot 更简单。

### D5：持久化存储 — 扩展 SessionMetadata + 新增 SessionInput JSONL

- **选择**：扩展 `SessionMetadata` 新增 `end_reason`、`iteration`、`cumulative_tokens`、`context_token_limit` 字段。新增 `session_input.jsonl` 持久化转向消息队列。所有新增字段使用 `serde(default)` 保证向后兼容。
- **理由**：Synthia 已有 `messages.jsonl` + `metadata.json` 持久化基础。引入 SQLite 增加依赖复杂度。JSONL append-only 与现有架构一致。
- **已考虑 alternative**：SQLite（OpenCode 模式）。拒绝，因为增加 sqlx 依赖，且 Synthia 的数据量不需要 SQL 查询能力。JSON 文件已足够。

### D6：转向队列 — 从内存 mpsc 迁移到持久化 JSONL

- **选择**：新增 `SessionInputQueue` 结构体，底层使用 `session_input.jsonl`（append-only）。`drain_steering()` 改为从 JSONL 文件读取未消费的输入。消费后标记 `promoted: true`。
- **理由**：OpenCode 的 `session_input` 表（SQLite）已证明持久化输入队列的价值 — 转向消息在进程重启后仍然存在。JSONL 实现与现有消息存储一致。
- **已考虑 alternative**：保持内存 mpsc + 新增重启标记。拒绝，因为仍然丢失转向消息内容，仅知道"有消息被丢弃"。

## Risks / Trade-offs

- **[Risk] 子智能体执行可能耗尽系统资源** — 如果 agent 无限制地 spawn 子智能体 → Mitigation: 深度限制（默认 1）+ 并发限制（默认 6），通过 `AgentRegistry` 原子计数强制执行。
- **[Risk] 统一的 AgentInstance 可能破坏现有代码** — 两套类型有不同调用方 → Mitigation: 先合并类型定义，再逐步迁移调用方。保留旧类型路径作为 `pub use` shim 直到所有调用方迁移完成。
- **[Risk] SessionMetadata 字段新增可能破坏旧 session 文件读取** — 反序列化时缺少新字段 → Mitigation: `serde(default)` + `..Default::default()` 确保旧文件自动使用默认值。
- **[Trade-off] SessionInput 使用 JSONL 而非 SQLite** — 牺牲了查询能力（如"列出所有待处理转向"），但避免了引入 sqlx 依赖。接受理由：转向消息量小，顺序扫描 JSONL 足够。
- **[Trade-off] 使用 oneshot 而非 watch channel** — 牺牲了中途向子智能体发送消息的能力。接受理由：当前阶段仅需 spawn-and-wait 模式，后续可通过 Mailbox 升级。

## Migration Plan

1. **Phase 1 — 持久化**：扩展 `SessionMetadata`，新增 `SessionInputQueue`。旧 session 文件自动兼容（`serde(default)`）。无需数据迁移。
2. **Phase 2 — 类型统一**：合并两套 `AgentInstance`。保留旧路径作为 `pub use` shim。更新所有调用方。
3. **Phase 3 — 执行桥接**：实现 `run_subagent()`，接线 `AgentTool::call()` 的前台模式。后台模式依赖 Mailbox 接线。
4. **Phase 4 — Mailbox 接线**：将 `AgentControl` 和 `Mailbox` 接入主执行循环。移除 `agent_control: _` 忽略。
5. **Rollback**：每个 phase 独立可回滚。持久化字段新增不影响现有行为（新字段有默认值）。类型统一通过 shim 保证向后兼容。

## Open Questions

- 子智能体的 `system_prompt` 如何构建？是否需要注入父智能体的任务上下文？
- 后台模式的结果注入时机 — 在父智能体下一次 LLM 调用前还是立即中断当前工具执行？
- `ForkPolicy::ByTag` 和 `ForkPolicy::SinceStep` 的语义在当前消息模型中如何实现？