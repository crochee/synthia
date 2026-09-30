# turn-id-mvp Implementation Plan (FROZEN)

> **重要：此 change 在 2026-06-13 完成多专家对抗性审查后被冻结 3 个月。**
> **冻结期：2026-06-13 → 2026-09-13**
> **解冻前请勿执行任何任务！**
>
> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task — ONLY after the freeze is lifted.

**Goal:** 提供 `TurnId(Uuid)` 类型作为可观测性标签，支持跨事件 Turn 标识关联。解冻后严格按简化派 MVP 实施（~20 行），不扩展为完整 Turn 模型。

**Architecture:** 新增 `crates/synthia-agent/src/turn.rs` 文件（~10 行），定义 `pub struct TurnId(pub Uuid)`。`LoopContext` 加 1 个字段 `current_turn_id: Option<TurnId>`。`StreamBuilder` 把 `format!("turn-{}", ctx.iteration)` 替换为 `ctx.current_turn_id`。**不引入 struct、不引入状态机、不引入新事件、不持久化。**

**Tech Stack:** Rust, uuid, serde, OpenSpec

---

## ⚠️ FROZEN STATE NOTICE

```
THIS CHANGE IS FROZEN FROM 2026-06-13 TO 2026-09-13.

DO NOT IMPLEMENT THE FOLLOWING TASKS DURING THE FROZEN PERIOD.

Thaw conditions (any one):
  1. A real caller needs "turn-level" querying (audit, billing, debug tool)
  2. User explicitly requests thaw
  3. All three prerequisite tasks complete:
     - unify-token-usage-types (in progress)
     - turn-id-unify (not started)
     - recovery-path-explicit (not started)

Hard cap: 2026-12-13. If not thawed by then, archive indefinitely.
```

---

## Task Group 1: Frozen Period Tasks (DO NOT IMPLEMENT)

**Status:** FROZEN — these tasks track state, not implementation work.

- [ ] **Step 1.1:** Wait for any of the thaw conditions to be met
- [ ] **Step 1.2:** If 2026-12-13 reached without thaw, archive to `archive/turn-id-mvp-expired/`
- [ ] **Step 1.3:** Mark `turn-id-label` capability as "deferred indefinitely" in `openspec/specs/`

---

## Task 2: Create TurnId type (ONLY AFTER THAW)

**Files:**
- `crates/synthia-agent/src/turn.rs` (create, < 30 lines)
- `crates/synthia-agent/src/lib.rs` (modify, add `pub mod turn;`)

> ⚠️ DO NOT EXECUTE THIS TASK DURING FROZEN PERIOD

- [ ] **Step 1:** 验证 3 个前置任务全部 archived：`openspec list | grep -E "unify-token-usage-types|turn-id-unify|recovery-path-explicit"`
- [ ] **Step 2:** 创建 `crates/synthia-agent/src/turn.rs`：
  ```rust
  use serde::{Deserialize, Serialize};
  use uuid::Uuid;

  /// Stable identifier for a turn, used for cross-event correlation.
  ///
  /// MVP label type — does NOT carry business data. All turn-related
  /// data is read from `LoopContext` and `AgentEvent` stream.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub struct TurnId(pub Uuid);

  impl TurnId {
      pub fn new() -> Self {
          Self(Uuid::new_v4())
      }
  }

  impl Default for TurnId {
      fn default() -> Self {
          Self::new()
      }
  }
  ```
- [ ] **Step 3:** 在 `crates/synthia-agent/src/lib.rs` 添加 `pub mod turn;`
- [ ] **Step 4:** 验证文件 < 30 行：`wc -l crates/synthia-agent/src/turn.rs`
- [ ] **Step 5:** 运行 `cargo check -p synthia-agent` → 期望 0 错误

---

## Task 3: Add current_turn_id to LoopContext (ONLY AFTER THAW)

**Files:**
- `crates/synthia-agent/src/loop_context.rs` (modify, add 1 field)

> ⚠️ DO NOT EXECUTE THIS TASK DURING FROZEN PERIOD

- [ ] **Step 1:** 读取 `crates/synthia-agent/src/loop_context.rs:9-21` 当前 `LoopContext` 定义
- [ ] **Step 2:** 添加 `pub current_turn_id: Option<TurnId>` 字段：
  ```rust
  use crate::turn::TurnId;

  pub struct LoopContext {
      pub session_id: String,
      pub iteration: usize,
      pub messages: Vec<Message>,
      pub end_reason: Option<SessionEndReason>,
      pub cumulative_tokens: usize,
      pub recent_tool_results: Vec<(String, String, bool)>,
      pub needs_compact: bool,
      pub span_ctx: SpanContext,
      pub context_token_limit: Option<usize>,
      pub current_turn_id: Option<TurnId>,  // NEW
  }
  ```
- [ ] **Step 3:** 在 `LoopContext::new`（line 24-36）初始化 `current_turn_id: None`
- [ ] **Step 4:** 验证 `LoopContext.iteration: usize` 字段保留（不删除）
- [ ] **Step 5:** 运行 `cargo check -p synthia-agent` → 期望 0 错误
- [ ] **Step 6:** 运行 `cargo test -p synthia-agent --lib loop_context` → 期望现有测试通过

---

## Task 4: Replace formatted string in StreamBuilder (ONLY AFTER THAW)

**Files:**
- `crates/synthia-agent/src/stream_builder/builder.rs` (modify, line 327)

> ⚠️ DO NOT EXECUTE THIS TASK DURING FROZEN PERIOD

- [ ] **Step 1:** 读取 `crates/synthia-agent/src/stream_builder/builder.rs:325-328` 当前 `AgentContext::new` 调用
- [ ] **Step 2:** 检查 `synthia_hook::AgentContext.turn_id` 的类型（可能是 `String`）
- [ ] **Step 3a:** 如果 `AgentContext.turn_id` 是 `String`，需要类型升级：
  - 修改 `synthia-hook` 的 `AgentContext.turn_id` 从 `String` → `TurnId`
  - 在 hook 内部使用 `turn_id.to_string()` 替代字符串拼接
- [ ] **Step 3b:** 如果 `AgentContext.turn_id` 已经是 `Option<TurnId>`，直接替换
- [ ] **Step 4:** 在 `builder.rs:325-328` 替换：
  ```rust
  // Before:
  let mut agent_ctx = AgentContext::new(
      ctx.session_id.clone(),
      format!("turn-{}", ctx.iteration),
  );

  // After (assuming TurnId is set before this point):
  let mut agent_ctx = AgentContext::new(
      ctx.session_id.clone(),
      ctx.current_turn_id,
  );
  ```
- [ ] **Step 5:** 在 `builder.rs` 主循环开头（line 225 之前）添加 `ctx.current_turn_id = Some(TurnId::new())`
- [ ] **Step 6:** 验证无新 `format!("turn-{}", ...)` 调用：`grep "format!(\"turn-" crates/synthia-agent/src/stream_builder/builder.rs`
- [ ] **Step 7:** 运行 `cargo check -p synthia-agent` → 期望 0 错误
- [ ] **Step 8:** 运行 `cargo test -p synthia-agent` → 期望现有测试通过

---

## Task 5: Validation and audit (ONLY AFTER THAW)

**Files:**
- 全 workspace

> ⚠️ DO NOT EXECUTE THIS TASK DURING FROZEN PERIOD

- [ ] **Step 1:** 运行 `cargo check --workspace` → 期望 0 错误
- [ ] **Step 2:** 运行 `cargo test --workspace` → 期望 100% 测试通过
- [ ] **Step 3:** 运行 `cargo +nightly fmt --all` → 期望无变更
- [ ] **Step 4:** 运行 `cargo clippy --all-targets --all-features --tests --all` → 修复所有警告
- [ ] **Step 5:** grep 审计：
  - `grep -rn "pub struct TurnId" crates/` → 期望 1 处（`synthia-agent/src/turn.rs`）
  - `grep -rn "pub struct Turn\b" crates/` → 期望 0 行
  - `grep -rn "pub enum TurnStatus" crates/` → 期望 0 行
  - `grep -rn "TurnStarted\|TurnCompleted\|TurnFailed\|TurnAborted" crates/` → 期望 0 行
  - `grep -rn "save_turn\|load_turn\|append_turn\|turns.jsonl" crates/` → 期望 0 行

---

## Task 6: OpenSpec finalization (ONLY AFTER THAW)

> ⚠️ DO NOT EXECUTE THIS TASK DURING FROZEN PERIOD

- [ ] **Step 1:** 运行 `openspec validate turn-id-mvp --strict` → 期望 0 错误
- [ ] **Step 2:** 提交 commit：`git add -A && git commit -m "feat(agent): introduce TurnId(Uuid) as MVP turn label (~20 lines)"`
- [ ] **Step 3:** 推送：`git push origin <branch>`
- [ ] **Step 4:** 等待 CI 通过
- [ ] **Step 5:** 运行 `openspec archive turn-id-mvp`

---

## 风险与回滚

- **风险 1:** 冻结期内用户强烈要求完整 Turn 模型
  - **缓解:** 解冻后走完整审查流程；临时方案保留
- **风险 2:** MVP 实施时过度扩展
  - **缓解:** tasks.md 明确禁止；OpenSpec verify 检查行数 < 30
- **风险 3:** 3 个月后 codebase 状态变化大
  - **缓解:** 解冻时重新评估；不满足条件则永久归档
- **回滚策略:** MVP 0 破坏性变更，revert PR 即可
</content>
</invoke>