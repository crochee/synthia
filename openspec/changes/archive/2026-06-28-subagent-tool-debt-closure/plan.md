# subagent-tool-debt-closure Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** 关闭 synthia 在 Subagent 框架与 Tool 系统的 8 项真实债（F6/F7/F8/F11/F15/F19/F20/F23），不引入新抽象。

**Architecture:** 8 项独立改进，按 P0（防泄漏/防 panic）→ P1（架构正确性/用户体验）顺序实施。每项改进复用现有 trait/struct，通过新增字段、修改方法签名、增加 trait bound 实现。无新 crate 依赖。

**Tech Stack:** Rust + tokio + dashmap + serde_json + tokio_util::sync::CancellationToken

**Reference Artifacts:**
- `openspec/changes/subagent-tool-debt-closure/proposal.md` — 变更范围与 capabilities
- `openspec/changes/subagent-tool-debt-closure/design.md` — 8 个技术决策（D1-D8）与权衡
- `openspec/changes/subagent-tool-debt-closure/specs/*/spec.md` — 8 个 capability 的可测试规约
- `openspec/changes/subagent-tool-debt-closure/tasks.md` — 9 个任务组共 47 个 checkbox

---

## Task 1: P0 配额 RAII 化 (F8)

- [ ] **Step 1:** 在 `crates/synthia-agent/src/subagent/team.rs` 定义 `SlotGuard` struct：
  ```rust
  pub struct SlotGuard {
      manager: Arc<SubagentManager>,
      released: bool,
  }
  impl Drop for SlotGuard {
      fn drop(&mut self) {
          if !self.released { self.manager.release_slot(); }
      }
  }
  impl SlotGuard {
      pub fn commit(mut self) { self.released = true; }
  }
  ```
- [ ] **Step 2:** 修改 `try_acquire_slot()` 返回 `Option<SlotGuard>`（CAS 成功后包 guard）
- [ ] **Step 3:** 在 `agent_tool.rs` 6 处调用点：成功路径 `guard.commit()`，错误路径 drop（移除手动 `release_slot()`）
- [ ] **Step 4:** 新增 3 个单元测试（drop 释放 / commit 防双重 / 配额耗尽返回 None）
- [ ] **Step 5:** 验证 `cargo test -p synthia-agent` 通过
- [ ] **Commit:** `feat(subagent): RAII SlotGuard for quota management (F8)`

## Task 2: P0 工具输入 schema 校验 (F15)

- [ ] **Step 1:** 在 `crates/synthia-tool-orchestrator/src/lib.rs` 的 `ToolAdapter<T: Tool>` impl block 增加 `where T::Input: serde::de::DeserializeOwned`
- [ ] **Step 2:** 修改 `execute` 方法，在 `tool.call(input)` 前加：
  ```rust
  let input = match serde_json::from_value::<T::Input>(request.arguments.clone()) {
      Ok(v) => v,
      Err(e) => return ToolOutput::error(format!("Invalid input: {e}")),
  };
  ```
- [ ] **Step 3:** grep 确认所有 `Tool` impl 的 `Input` 类型有 `#[derive(Deserialize)]`
- [ ] **Step 4:** 新增 4 个单元测试（valid / invalid type / missing field / error visible）
- [ ] **Step 5:** 验证 `cargo test -p synthia-tool-orchestrator` 通过
- [ ] **Commit:** `feat(tool): serde input validation in ToolAdapter (F15)`

## Task 3: P1 max_depth 接通 (F6/F14)

- [ ] **Step 1:** `crates/synthia-agent/src/subagent/config.rs` 的 `SubagentConfig` 增加 `pub depth: usize`
- [ ] **Step 2:** `SubagentSessionFactory::create_child` 签名加 `parent_depth: usize`，子 depth = parent + 1
- [ ] **Step 3:** `crates/synthia-server/src/state/subagent_factory.rs` 传递 parent_depth
- [ ] **Step 4:** `team.rs::current_depth()` 从 `0` 改为 `self.config.depth`
- [ ] **Step 5:** `agent_tool.rs::call` spawn 前检查 `depth >= max_depth` 返回 error
- [ ] **Step 6:** 新增 4 个单元测试
- [ ] **Step 7:** 验证 `cargo test -p synthia-agent` 通过
- [ ] **Commit:** `feat(subagent): wire max_depth check and depth tracking (F6/F14)`

## Task 4: P1 background 完成通知 (F7)

- [ ] **Step 1:** 在 `crates/synthia-agent/src/events/` 新增 `SubagentCompleted { session_id, result_summary }` 事件类型
- [ ] **Step 2:** `factory.rs::run_child` background 路径，子完成时发 `AgentEvent::SubagentEvent` 到 `parent_event_sender`
- [ ] **Step 3:** `result_summary` 取前 500 字符，UTF-8 安全截断（复用 `find_safe_boundary`）
- [ ] **Step 4:** 处理 closed sender：`let _ = sender.send(...)` 静默忽略
- [ ] **Step 5:** 新增 4 个单元测试
- [ ] **Step 6:** 验证 `cargo test -p synthia-agent` 通过
- [ ] **Commit:** `feat(subagent): background completion notification via SubagentCompleted (F7)`

## Task 5: P1 递归子树取消 (F11)

- [ ] **Step 1:** `team.rs::SubagentManager` 增加 `child_sessions: DashMap<SessionId, Vec<SessionId>>`
- [ ] **Step 2:** `create_child` 成功后 `child_sessions.entry(parent).or_default().push(child)`
- [ ] **Step 3:** `remove_session` 级联清理（从父 list 移除 + 删自身 entry）
- [ ] **Step 4:** 实现 `cancel_session_tree(session_id)`：DFS 遍历递归 cancel
- [ ] **Step 5:** 为每个 child 增加 per-session `child_cancel_token`（parent token.child_token()）
- [ ] **Step 6:** 新增 4 个单元测试
- [ ] **Step 7:** 验证 `cargo test -p synthia-agent` 通过
- [ ] **Commit:** `feat(subagent): recursive subtree cancellation (F11)`

## Task 6: P1 always 权限持久化 (F19)

- [ ] **Step 1:** `crates/synthia-permission/src/checker/checker.rs::PermissionChecker` 增加 `saved_rules: Arc<DashSet<(String, String)>>`
- [ ] **Step 2:** 构造函数初始化 `saved_rules: Arc::new(DashSet::new())`
- [ ] **Step 3:** `check()` 中先查 `saved_rules.contains(&(action, resource))`，命中返回 AutoApprove
- [ ] **Step 4:** 实现 `pub fn remember_always(&self, action: String, resource: String)`
- [ ] **Step 5:** 实现 `pub fn forget_always(&self, action: &str, resource: &str)`
- [ ] **Step 6:** 新增 5 个单元测试
- [ ] **Step 7:** 验证 `cargo test -p synthia-permission` 通过
- [ ] **Commit:** `feat(permission): always-rule persistence with remember/forget API (F19)`

## Task 7: P1 failInterruptedTools 批量清理 (F20)

- [ ] **Step 1:** `tool-orchestrator/lib.rs::DefaultToolOrchestrator` 实现 `pub fn fail_interrupted_tools(&self) -> usize`
- [ ] **Step 2:** 遍历 `active_calls`，每个 entry：cancel + remove + 发 `ToolCallCompleted { is_error: true }`
- [ ] **Step 3:** 在 `stream_builder/builder/` 主循环中断检测点调用 `fail_interrupted_tools()`
- [ ] **Step 4:** 确保事件持久化到 JSONL 并加入 `ctx.recent_tool_results`
- [ ] **Step 5:** 新增 4 个单元测试
- [ ] **Step 6:** 验证 `cargo test -p synthia-tool-orchestrator` 和 `cargo test -p synthia-agent` 通过
- [ ] **Commit:** `feat(tool): fail_interrupted_tools batch cleanup on interruption (F20)`

## Task 8: P1 bash 输出上限提升 (F23)

- [ ] **Step 1:** `system_tools.rs` 修改 `const MAX_OUTPUT_BYTES: usize = 1_048_576;`
- [ ] **Step 2:** grep `30_000\|30000` 找测试断言并更新
- [ ] **Step 3:** 新增 3 个回归测试（under 1MB / over 1MB / UTF-8 at boundary）
- [ ] **Step 4:** 验证 `cargo test -p synthia-agent` 通过
- [ ] **Commit:** `feat(bash): raise MAX_OUTPUT_BYTES to 1MB (F23)`

## Task 9: 集成验证

- [ ] **Step 1:** `cargo check --all-targets --all-features`
- [ ] **Step 2:** `cargo clippy --all-targets --all-features --tests --all`（修复所有警告）
- [ ] **Step 3:** `cargo +nightly fmt --all`
- [ ] **Step 4:** `cargo test --all`
- [ ] **Step 5:** `openspec validate subagent-tool-debt-closure`
- [ ] **Step 6:** 手动验证：spawn 超 depth 返回 error / background 完成父见通知 / always 生效 / 中断无僵尸
- [ ] **Commit:** `test: integration verification for subagent-tool-debt-closure`

---

## 执行策略

**并行性**: Task 1/2/6/7/8 互相独立，可并行派发 subagent。Task 3/4/5 都改 subagent 模块，建议串行（3 → 4 → 5）。

**验证门槛**: 每个 Task 结束必须 `cargo test -p <crate>` 通过才能进入下一个 Task。

**回滚**: 每个 Task 独立 commit，git revert 即可回滚单 Task。

**OpenSpec 归档**: Task 9 验证全过后，运行 `openspec archive subagent-tool-debt-closure` 归档。
