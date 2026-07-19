## Context

synthia 经多轮 P0/P1 改进后，Subagent 框架与 Tool 系统仍有 8 项真实债。这些债经代码级验证（非归档文档），通过三方对抗性分析（性能/可靠性/架构三视角）确认必须修复。

**当前状态**:
- Subagent 框架：`SubagentManager` 有 `max_depth=3` 配置但 `current_depth()` 是 stub；配额 `try_acquire_slot` 返回 bool，6 处手动 release；background 模式结果丢弃；无递归子树取消
- Tool 系统：`ToolAdapter::execute` 无输入校验；`PermissionChecker` 无 always 持久化；中断无显式清理；bash 输出 30KB 偏低

**约束**:
- 不引入新抽象（YAGNI）
- 不引入 Effect 框架（prior memory 明确不做）
- 借鉴 codex `SpawnReservation::Drop` 和 opencode `failInterruptedTools` 等具体实现
- 保留 synthia 已有优势（4 层循环检测、5 层错误恢复、head+tail UTF-8 截断）

**利益相关者**:
- Agent 开发者：配额泄漏 / panic 风险影响生产稳定性
- 终端用户：重复权限确认 / bash 输出截断影响体验
- SaaS 多租户：max_depth 失效是安全风险

## Goals / Non-Goals

**Goals:**
- 关闭 8 项真实债（F6/F7/F8/F11/F15/F19/F20/F23）
- 配额管理 RAII 化，防泄漏
- 工具输入 schema 校验，防 panic
- max_depth 安全防线接通
- background 模式最小可用（事件通知）
- 递归子树取消，防 zombie
- always 权限持久化，改善体验
- failInterruptedTools 批量清理，防僵尸状态
- bash 输出上限对齐业界标准

**Non-Goals:**
- 不做 task_id 恢复（并入 `p0-subagent-execution-session-persistence`）
- 不做沙箱升级重试（并入 `production-tool-execution-sandbox`）
- 不做 team/coordinator 接通（YAGNI，多 agent 协作非 synthia 场景）
- 不做 ToolLifecycleContributor（synthia hook_registry 已覆盖）
- 不做 background 完整注入父 session input queue（留 P2）
- 不引入 SQLite / 向量记忆 / Effect 框架

## Decisions

### D1：配额 RAII 化用 SlotGuard + commit() 模式

- **选择**：`try_acquire_slot()` 返回 `Option<SlotGuard>`，guard 持 `manager: Arc<SubagentManager>` + `released: bool`，Drop 时 `if !released { release_slot() }`。`commit()` 标记 `released = true` 防止 double-release。
- **理由**：Rust 惯用法，drop 即释放，不可能泄漏。`commit()` 让成功路径显式标记，错误路径自动释放。
- **已考虑 alternative**：
  - 手动 release + clippy lint 检查 → 拒绝：未来加错误路径仍会漏
  - `tokio::spawn` + JoinHandle abort → 拒绝：abort 不释放配额计数
  - 借鉴 codex `SpawnReservation`（带 reserved_agent_path）→ 拒绝：synthia 无 nickname pool，简化版即可

### D2：工具输入校验用 serde_json::from_value + DeserializeOwned trait bound

- **选择**：`ToolAdapter<T: Tool>` 增强 trait bound `T::Input: DeserializeOwned`，`execute` 中 `serde_json::from_value::<T::Input>(request.arguments.clone())`，失败返回 `ToolOutput::error(format!("Invalid input: {err}"))`。
- **理由**：Rust serde 是标准方案，无需新依赖。`request.arguments.clone()` 因为 `from_value` 消费参数，但 arguments 后续可能用于日志。
- **已考虑 alternative**：
  - jsonschema crate 运行时校验 → 拒绝：增加依赖 + 性能开销，且 synthia 工具数量少
  - opencode Effect Schema 双向校验 → 拒绝：需引入 Effect 框架（prior memory 明确不做）
  - 每个工具自己校验 → 拒绝：散布重复代码，易遗漏

### D3：max_depth 用 SubagentConfig.depth 字段 + spawn 时 +1

- **选择**：`SubagentConfig` 增加 `pub depth: usize`，`SubagentSessionFactory::create_child` 接受 `parent_depth: usize`，子 config depth = parent_depth + 1。`AgentTool::call` 在 spawn 前检查 `config.depth >= manager.max_depth()` 返回 `ToolOutput::error("Max sub-agent depth reached")`。`current_depth()` 从 stub 改为 `self.config.depth`。
- **理由**：最小改动接通安全防线。depth 字段在 config 中传递，不污染 session 持久化层。
- **已考虑 alternative**：
  - 用 `Arc<AtomicUsize>` 全局计数器 → 拒绝：无法区分不同 agent tree 的 depth
  - 借鉴 codex `next_thread_spawn_depth(session_source)` → 拒绝：synthia 无 SessionSource 抽象，简化版即可
  - 仅依赖 `derive_subagent_permission` forced Deny task → 拒绝：builtin config 显式 `allow_task: true` 时无防线

### D4：background 通知用 parent_event_sender 发 SubagentCompleted 事件

- **选择**：background 子 agent 完成时，通过 `ChildSessionHandle.parent_event_sender` 发 `AgentEvent::SubagentEvent { inner: SubagentCompleted { session_id, result_summary } }` 到父流。`result_summary` 是 `AgentResult` 的简短摘要（前 500 字符）。
- **理由**：最小修复，复用现有 event channel。父 agent 下一轮 LLM 能看到 background 完成通知。
- **已考虑 alternative**：
  - 完整注入父 session input queue（学 codex `inject_user_message_without_turn`）→ 拒绝：需改 session input 层，工程量大，留 P2
  - opencode `inject("completed"|"error", text)` synthetic message → 拒绝：synthia 无 synthetic message 抽象

### D5：递归子树取消用 child_sessions DashMap + cancel_session_tree DFS

- **选择**：`SubagentManager` 增加 `child_sessions: DashMap<SessionId, Vec<SessionId>>`（parent → children 映射）。`create_child` 时注册 `child_sessions.entry(parent_id).or_default().push(child_id)`。`cancel_session_tree(session_id)` 递归遍历 children，对每个 child 调用 `cancel_session(child_id)`，再递归 cancel 孙子。
- **理由**：显式递归取消，不依赖共享 token。支持"取消父不取消子"的精细控制（虽然当前不需要，但 API 预留）。
- **已考虑 alternative**：
  - 共享 CancellationToken（现有方案）→ 拒绝：无法做精细控制，且无法追踪 child 列表
  - 借鉴 codex `list_live_agent_subtree_thread_ids` 基于 SQLite 查询 → 拒绝：synthia 暂不引入 SQLite

### D6：always 持久化用 DashSet<(action, resource)> + remember_always API

- **选择**：`PermissionChecker` 增加 `saved_rules: Arc<DashSet<(String, String)>>`。`check()` 中对每个 request，先查 `saved_rules.contains(&(action, resource))`，命中则 AutoApprove。新增 `pub fn remember_always(&self, action: String, resource: String)` 供 approval service 调用。
- **理由**：DashSet 并发安全，O(1) 查找。不持久化到磁盘（session 级别，重启清空）。
- **已考虑 alternative**：
  - 持久化到 SQLite → 拒绝：prior memory 明确暂不引入 SQLite
  - opencode 级联批准同 session pending 请求 → 拒绝：synthia 无 pending 请求队列抽象，简化版即可
  - 用 HashMap + Mutex → 拒绝：DashSet 更适合高频读低频写场景

### D7：failInterruptedTools 用 active_calls 遍历 + 批量发 Failed 事件

- **选择**：agent 主循环检测到中断时，调用 `tool_orchestrator.fail_interrupted_tools()`。该方法遍历 `active_calls: DashMap<String, CancellationToken>`，对每个 entry：1) `entry.value().cancel()` 2) 从 map 移除 3) 通过 event sender 发 `AgentEvent::ToolCallCompleted { tool_name, output: "Tool execution interrupted", is_error: true }`。
- **理由**：批量清理确保无僵尸状态。复用现有 active_calls + event channel。
- **已考虑 alternative**：
  - 仅依赖 CancellationToken → 拒绝：不显式发 Failed 事件，外部观察者无法感知
  - opencode `failInterruptedTools` 遍历 session content → 拒绝：synthia 无 session content 抽象，用 active_calls 更直接

### D8：bash 上限提升到 1MB

- **选择**：`MAX_OUTPUT_BYTES` 从 30_000 提升到 1_048_576（1MB）。head+tail 截断逻辑不变（已有 UTF-8 安全边界检查 `find_safe_boundary`）。
- **理由**：对齐 opencode `MAX_CAPTURE_BYTES = 1MB` 和 codex `EXEC_OUTPUT_MAX_BYTES = 1MB`。30KB 对 `cargo build` 等长输出场景太小。
- **已考虑 alternative**：
  - 可配置化 → 拒绝：YAGNI，1MB 是业界共识
  - 分级上限（bash 1MB / 其他工具 30KB）→ 拒绝：增加复杂度，统一 1MB 即可

## Risks / Trade-offs

- [Risk] RAII SlotGuard 跨 await 点 drop 时机不确定 → Mitigation: guard 不跨 await 点，在 `AgentTool::call` 的同步段使用，`commit()` 在 spawn 成功后立即调用
- [Risk] schema 校验失败的工具输出可能让 LLM 陷入重试循环 → Mitigation: 错误消息包含具体字段信息，引导 LLM 修正；复用现有 4 层循环检测兜底
- [Risk] max_depth 接通后，旧 builtin config 的 `allow_task: true` 可能意外触发 depth 限制 → Mitigation: 当前所有 builtin 都 `allow_task: false`，无实际影响；depth=3 足够深
- [Risk] background 通知的 result_summary 可能泄漏敏感信息到父 LLM → Mitigation: summary 只取前 500 字符，且 background 子 agent 本身受权限约束
- [Risk] child_sessions DashMap 在 session 结束时未清理可能内存泄漏 → Mitigation: `remove_session(session_id)` 时级联删除 child_sessions 中的记录
- [Risk] always 持久化规则可能被误用（用户 always 后后悔）→ Mitigation: 提供 `forget_always(action, resource)` API；session 结束时 saved_rules 自动清空
- [Trade-off] bash 1MB 上限增加内存占用 → 接受理由：现代机器内存充足，LLM 上下文价值 > 内存成本

## Migration Plan

本变更是纯代码改进，不涉及部署变更（无 endpoint / DB schema / 配置文件格式变更）。

**部署顺序**:
1. 先合入 P0 项（F8 配额 RAII + F15 schema 校验），因 trait bound 变更可能影响下游工具实现
2. 再合入 P1 项（F6/F7/F11/F19/F20/F23）

**Rollback 策略**: git revert 即可，无数据迁移。

**验收条件**:
- `cargo test --all` 通过
- `cargo clippy --all-targets --all-features --tests --all` 无警告
- 新增单元测试覆盖每项变更的 happy path + 边界
- `cargo +nightly fmt --all` 格式化通过

## Open Questions

- **OQ1**: `SubagentConfig.depth` 是否需要持久化到 session JSONL？当前决策是不持久化（depth 是运行时计算的，不跨 session 恢复）。如果未来支持 task_id 恢复，需要重新评估。
- **OQ2**: `saved_rules` 是否需要跨 session 持久化？当前决策是 session 级别（重启清空）。如果用户反馈希望跨 session 保留，需要引入磁盘持久化（但 prior memory 明确暂不引入 SQLite）。
- **OQ3**: `fail_interrupted_tools()` 的触发时机是在 `cancel_token.cancelled()` 时还是 steering 中断时？当前决策是两者都触发（任何中断都清理）。
