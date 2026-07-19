# turn-id-unfreeze Tasks

> **重要：本 change 是元变更（meta-change），仅做"记录 + 评估 + 决策"三件事。**
> **本 tasks.md 0 代码变更任务。**
> **TurnId MVP 实施仍归 `turn-id-mvp/tasks.md`（受 3 个前置条件门控，解冻后 2026-09-13 起执行）。**

---

## 1. 记录 codex 解冻触发事件

- [ ] 1.1 在 `openspec/changes/turn-id-unfreeze/proposal.md` 明确记录：
  - codex PR #28002 `[codex] Send turn state through compact requests`
  - codex PR #27996 `[codex] Send request-scoped turn state over WebSocket`
  - PR #27996 描述原文（直接引用）
  - 改动的 codex 文件路径（`codex-rs/core/src/session/turn.rs` 等 5+ 个模块）
- [ ] 1.2 在 `openspec/changes/turn-id-unfreeze/design.md` 的 Decisions 段记录 D6 决议（"触发证据以 PR 链接 + commit hash 记录"）
- [ ] 1.3 在 `openspec/changes/turn-id-unfreeze/design.md` 的 Context 段记录 codex 投资规模（2296 行 + 391 + 349 + 241 + 2 个未列出行数）

## 2. 重新评估 3 个月冻结期

- [ ] 2.1 列出原 3 个解冻条件（来自 `turn-id-mvp/design.md` D2）：
  - [ ] 2.1.1 条件 #1：出现"按 turn 维度查询"的真实 caller
  - [ ] 2.1.2 条件 #2：TokenUsage / recovery path 等其他原语收敛
  - [ ] 2.1.3 条件 #3：3 个月时间窗口（2026-06-13 → 2026-09-13）
- [ ] 2.2 评估 codex PR 是否满足条件 #1：
  - [ ] 2.2.1 引用 PR #27996 描述："Turn state is scoped to one logical turn, but the WebSocket path currently exchanges it through upgrade headers"
  - [ ] 2.2.2 记录评估结论：条件 #1 已满足
- [ ] 2.3 评估是否应缩短 3 个月冻结期：
  - [ ] 2.3.1 检查 3 个前置条件完成状态：
    - [ ] 2.3.1.1 `unify-token-usage-types` change 状态（`openspec list` 查询）
    - [ ] 2.3.1.2 `turn-id-unify` change 状态
    - [ ] 2.3.1.3 `recovery-path-explicit` change 状态
  - [ ] 2.3.2 决议：维持 3 个月冻结期不缩短（记录 D2 决议）
  - [ ] 2.3.3 决议：实施仍受 3 个前置条件门控（记录 D4 决议）
- [ ] 2.4 评估 codex 设计作为 reference 的边界：
  - [ ] 2.4.1 列出 codex Turn 模型的模块清单（`turn.rs` / `turn_timing.rs` / `turn_metadata.rs` / `turn_diff_tracker.rs` / `state/turn.rs` / `context/turn_aborted.rs`）
  - [ ] 2.4.2 决议：仅作 reference，不复制（记录 D3 决议）

## 3. 形式化解冻决策

- [ ] 3.1 在 `openspec/changes/turn-id-unfreeze/specs/turn-id-unfreeze/spec.md` 创建 5 个 ADDED Requirements：
  - [ ] 3.1.1 Requirement: Unfreeze trigger evidence SHALL be recorded
  - [ ] 3.1.2 Requirement: Three-month freeze period SHALL NOT be shortened
  - [ ] 3.1.3 Requirement: Implementation SHALL remain gated by three prerequisites
  - [ ] 3.1.4 Requirement: codex design SHALL be treated as reference only
  - [ ] 3.1.5 Requirement: turn-id-mvp directory SHALL NOT be modified
  - [ ] 3.1.6 Requirement: turn-id-unfreeze SHALL introduce zero code changes
- [ ] 3.2 每个 Requirement 至少 1 个 Scenario（共 ≥ 6 个 Scenario）
- [ ] 3.3 所有 Requirement 第一句包含 SHALL 或 MUST（OpenSpec validate 规则）

## 4. 验证与提交

- [ ] 4.1 运行 `openspec validate turn-id-unfreeze --type change` 期望通过
- [ ] 4.2 运行 `openspec validate turn-id-unfreeze --type change --strict` 期望通过
- [ ] 4.3 运行 `openspec show turn-id-unfreeze` 验证 4 个 artifact 全部存在
- [ ] 4.4 验证 `turn-id-mvp/` 目录未被修改：
  - [ ] 4.4.1 `git status openspec/changes/turn-id-mvp/` 期望 "nothing to commit"
  - [ ] 4.4.2 `git diff openspec/changes/turn-id-mvp/` 期望空输出
- [ ] 4.5 验证本 change 0 代码变更：
  - [ ] 4.5.1 `git diff --stat` 仅显示 `openspec/changes/turn-id-unfreeze/` 下的文件
  - [ ] 4.5.2 `git diff --stat crates/` 期望空输出
  - [ ] 4.5.3 `git diff --stat` 不应出现 `crates/synthia-agent/src/turn.rs` / `loop_context.rs` / `stream_builder/builder.rs`
- [ ] 4.6 提交 commit：
  ```bash
  git add openspec/changes/turn-id-unfreeze/
  git commit -m "$(cat <<'EOF'
  docs(openspec): record turn-id-mvp unfreeze trigger
  
  Record OpenAI codex PR #28002 and #27996 (both merged 2026-06-13)
  as evidence that the "concrete use case" unfreeze condition for
  turn-id-mvp has been met. Maintain the 3-month freeze period
  (2026-06-13 → 2026-09-13) and keep implementation gated by the
  three prerequisite changes (unify-token-usage-types, turn-id-unify,
  recovery-path-explicit). This is a meta-change with zero code
  modifications.
  EOF
  )"
  ```
- [ ] 4.7 推送：等用户确认后再推送（避免在不确定时强制 push）

## 5. 冻结期监控（2026-06-13 → 2026-09-13）

- [ ] 5.1 监控 codex 后续 PR（每周一次）：
  - [ ] 5.1.1 `cd /home/crochee/workspace/codex && git fetch`
  - [ ] 5.1.2 `git log --oneline --since="1 week ago" -- codex-rs/core/src/session/turn.rs`
  - [ ] 5.1.3 如发现重大变更（如 turns.jsonl 持久化、turn-level cost attribution）→ 触发二阶评估
- [ ] 5.2 监控 3 个前置条件完成进度：
  - [ ] 5.2.1 每周 `openspec list | grep -E "unify-token-usage-types|turn-id-unify|recovery-path-explicit"`
  - [ ] 5.2.2 任一前置条件 archived 后，更新本 change 的 `proposal.md` 前置条件状态
- [ ] 5.3 等待 2026-09-13 硬解冻日

## 6. 解冻后衔接（仍归 `turn-id-mvp/tasks.md`）

> 以下是**衔接说明**，不构成本 change 的任务。实际执行在 `turn-id-mvp/tasks.md` 2.1-2.6 节。

- [ ] 6.1 由 `turn-id-mvp` change 执行以下任务（不在本 change 范围）：
  - [ ] 6.1.1 创建 `crates/synthia-agent/src/turn.rs`（< 30 行）
  - [ ] 6.1.2 在 `LoopContext` 加 `current_turn_id: Option<TurnId>` 字段
  - [ ] 6.1.3 替换 `builder.rs:327` 字符串构造
  - [ ] 6.1.4 验证、grep 审计、提交
- [ ] 6.2 可选子任务（不强制）：阅读 codex 2296 行 `turn.rs` 后写 "synthia-vs-codex Turn design notes" markdown（仅 reference notes，不复制代码）
- [ ] 6.3 解冻后归档：
  - [ ] 6.3.1 `openspec archive turn-id-mvp`
  - [ ] 6.3.2 本 change 同步归档到 `openspec/changes/archive/turn-id-unfreeze-archived/`

## 7. 6 个月硬截止（2026-12-13）

- [ ] 7.1 如果 2026-09-13 时前置条件仍未完成：
  - [ ] 7.1.1 `turn-id-mvp` 继续 FROZEN
  - [ ] 7.1.2 评估 codex 后续 PR 是否引入新维度
- [ ] 7.2 如果 2026-12-13 仍未解冻：
  - [ ] 7.2.1 归档 `turn-id-mvp` 到 `openspec/changes/archive/turn-id-mvp-expired/`
  - [ ] 7.2.2 归档本 change 到 `openspec/changes/archive/turn-id-unfreeze-expired/`
  - [ ] 7.2.3 `turn-id-label` capability 标注 "deferred indefinitely"
  - [ ] 7.2.4 通知用户归档决定
