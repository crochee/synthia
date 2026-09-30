## 1. P0: 配额管理 RAII 化 (F8)

- [x] 1.1 在 `crates/synthia-agent/src/subagent/team.rs` 定义 `SlotGuard` struct（`manager: Arc<SubagentManager>`, `released: bool`），实现 `Drop` 调用 `release_slot()`，实现 `commit()` 标记 `released = true`
- [x] 1.2 修改 `SubagentManager::try_acquire_slot` 返回类型从 `bool` 为 `Option<SlotGuard>`
- [x] 1.3 在 `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs` 更新 6 处调用点（L191,199,232,251,276,294）：成功路径调 `guard.commit()`，错误路径 drop guard 自动释放，移除手动 `release_slot()` 调用
- [x] 1.4 新增单元测试：`test_slot_guard_drop_releases_slot`、`test_slot_guard_commit_prevents_double_release`、`test_try_acquire_slot_returns_none_when_exhausted`
- [x] 1.5 运行 `cargo test -p synthia-agent` 验证通过

## 2. P0: 工具输入 schema 校验 (F15)

- [x] 2.1 在 `crates/synthia-tool-orchestrator/src/lib.rs` 的 `ToolAdapter<T: Tool>` impl 增强 trait bound：`T::Input: serde::de::DeserializeOwned`
- [x] 2.2 修改 `ToolAdapter::execute`：在调用 `tool.call(input)` 前，先 `serde_json::from_value::<T::Input>(request.arguments.clone())`，失败返回 `ToolOutput::error(format!("Invalid input: {err}"))`
- [x] 2.3 确认所有 `Tool` impl 的 `Input` 类型已有 `DeserializeOwned`（serde 默认 derive）
- [x] 2.4 新增单元测试：`test_valid_input_passes`、`test_invalid_type_rejected`、`test_missing_field_rejected`、`test_error_visible_to_llm`
- [x] 2.5 运行 `cargo test -p synthia-tool-orchestrator` 验证通过

## 3. P1: max_depth 接通 (F6/F14)

- [x] 3.1 在 `crates/synthia-agent/src/subagent/config.rs` 的 `SubagentConfig` 增加 `pub depth: usize` 字段
- [x] 3.2 修改 `SubagentSessionFactory::create_child` 签名增加 `parent_depth: usize` 参数，子 config depth = parent_depth + 1
- [x] 3.3 修改 `crates/synthia-server/src/state/subagent_factory.rs` 的 `AppStateSubagentFactory::create_child` 传递 parent_depth
- [x] 3.4 修改 `crates/synthia-agent/src/subagent/team.rs` 的 `current_depth()` 从 stub 返回 0 改为 `self.config.depth`
- [x] 3.5 在 `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs` 的 `AgentTool::call` spawn 前检查 `config.depth >= manager.max_depth()`，超限返回 `ToolOutput::error("Max sub-agent depth reached")`
- [x] 3.6 新增单元测试：`test_root_spawn_depth_1`、`test_depth_limit_exceeded`、`test_depth_limit_not_exceeded`、`test_current_depth_returns_config_depth`
- [x] 3.7 运行 `cargo test -p synthia-agent` 验证通过

## 4. P1: background 完成通知 (F7)

- [x] 4.1 在 `crates/synthia-agent/src/events/` 新增 `SubagentCompleted` 事件类型（`session_id: String`, `result_summary: String`）
- [x] 4.2 在 `crates/synthia-agent/src/subagent/factory.rs` 的 `run_child` background 路径，子 agent 完成时通过 `parent_event_sender` 发 `AgentEvent::SubagentEvent { inner: SubagentCompleted { ... } }`
- [x] 4.3 `result_summary` 取 `AgentResult` 输出的前 500 字符，UTF-8 安全截断（复用 `find_safe_boundary`）
- [x] 4.4 处理 `parent_event_sender` 已关闭情况：best-effort，send 失败静默忽略
- [x] 4.5 新增单元测试：`test_background_success_notifies_parent`、`test_background_failure_notifies_parent`、`test_closed_sender_is_noop`、`test_summary_truncated_at_char_boundary`
- [x] 4.6 运行 `cargo test -p synthia-agent` 验证通过

## 5. P1: 递归子树取消 (F11)

- [x] 5.1 在 `crates/synthia-agent/src/subagent/team.rs` 的 `SubagentManager` 增加 `child_sessions: DashMap<SessionId, Vec<SessionId>>` 字段
- [x] 5.2 在 `create_child` 成功后注册：`child_sessions.entry(parent_id).or_default().push(child_id)`
- [x] 5.3 在 `remove_session` 时级联清理：从父的 child list 移除，删除自身的 child list entry（递归清理后代，cancel token）
- [x] 5.4 实现 `cancel_session_tree(session_id: &SessionId)` 方法：DFS 遍历 child_sessions，递归 cancel 每个后代，最后 cancel 目标
- [x] 5.5 为每个 child session 增加 per-session `child_cancel_token: CancellationToken`（从 parent token child_token 派生），与共享 token 并存
- [x] 5.6 新增单元测试：`test_cancel_parent_cancels_descendants`、`test_cancel_no_children`、`test_cancel_handles_concurrent_removal`、`test_subtree_cancel_no_sibling_impact`、`test_remove_session_cleans_up_descendants`
- [x] 5.7 运行 `cargo test -p synthia-agent` 验证通过

## 6. P1: always 权限持久化 (F19)

- [x] 6.1 在 `crates/synthia-permission/src/checker/checker.rs` 的 `PermissionChecker` 增加 `saved_rules: Arc<DashSet<(String, String)>>` 字段
- [x] 6.2 修改 `check()` 逻辑：对每个 request，先查 `saved_rules.contains(&(action, resource))`，命中返回 `Permission::AutoApprove`
- [x] 6.3 实现 `pub fn remember_always(&self, action: String, resource: String)` 插入 saved_rules
- [x] 6.4 实现 `pub fn forget_always(&self, action: &str, resource: &str)` 移除 saved_rules
- [x] 6.5 更新 `crates/synthia-permission/src/checker/checker.rs` 构造函数初始化 saved_rules
- [x] 6.6 在 approval service 调用方（如 `HeadlessApprovalService` 或未来用户交互层）增加 `remember_always` 调用点（用户回复 "always" 时）
- [x] 6.7 新增单元测试：`test_saved_rule_auto_approve`、`test_saved_rule_no_match_evaluates_policy`、`test_remember_always_inserts`、`test_forget_always_removes`、`test_forget_nonexistent_is_noop`
- [x] 6.8 运行 `cargo test -p synthia-permission` 验证通过

## 7. P1: failInterruptedTools 批量清理 (F20)

- [x] 7.1 在 `crates/synthia-tool-orchestrator/src/lib.rs` 的 `DefaultToolOrchestrator` 实现 `pub fn fail_interrupted_tools(&self) -> usize`：遍历 `active_calls`，cancel + 移除 + 发 `ToolCallCompleted { is_error: true }` 事件
- [x] 7.2 通过 event sender 发 `AgentEvent::ToolCallCompleted { tool_name, output: "Tool execution interrupted", is_error: true }`，每个被中断的工具一个事件
- [x] 7.3 在 `crates/synthia-agent/src/stream_builder/builder/` 的主循环检测到中断时（`cancel_token.cancelled()` 或 steering 中断）调用 `tool_orchestrator.fail_interrupted_tools()`
- [x] 7.4 确保中断事件被持久化到 session JSONL 并加入 `ctx.recent_tool_results`
- [x] 7.5 新增单元测试：`test_fail_interrupted_multiple_tools`、`test_fail_interrupted_no_active_tools`、`test_fail_interrupted_concurrent_completion`、`test_interrupted_events_persisted`
- [x] 7.6 运行 `cargo test -p synthia-tool-orchestrator` 和 `cargo test -p synthia-agent` 验证通过

## 8. P1: bash 输出上限提升 (F23)

- [x] 8.1 在 `crates/synthia-agent/src/tools/builtins/system_tools.rs` 修改 `MAX_OUTPUT_BYTES` 常量从 `30_000` 为 `1_048_576`
- [x] 8.2 更新任何引用该常量的测试断言（grep `30_000` / `30000` 找测试）
- [x] 8.3 验证 head+tail 截断逻辑在 1MB 上限下仍正常工作（无需改逻辑，只调常量）
- [x] 8.4 新增回归测试：`test_output_under_1mb_not_truncated`、`test_output_over_1mb_truncated`、`test_utf8_safety_at_1mb_boundary`
- [x] 8.5 运行 `cargo test -p synthia-agent` 验证通过

## 9. 集成验证

- [x] 9.1 运行 `cargo check --all-targets --all-features` 确认无编译错误
- [x] 9.2 运行 `cargo clippy --all-targets --all-features --tests --all` 修复所有警告
- [x] 9.3 运行 `cargo +nightly fmt --all` 格式化
- [x] 9.4 运行 `cargo test --all` 全量测试通过
- [x] 9.5 运行 `openspec validate subagent-tool-debt-closure` 验证 spec 格式
- [x] 9.6 手动验证场景：spawn subagent 超 max_depth 返回 error、background 完成父能看到通知、always 权限生效、中断后无僵尸工具
