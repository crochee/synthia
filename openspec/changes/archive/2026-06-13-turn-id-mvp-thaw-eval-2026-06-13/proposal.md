## Why

`turn-id-mvp` change 自 2026-06-13 启动后被冻结 3 个月（2026-06-13 → 2026-09-13）。**当日**（2026-06-13）发生 2 个独立事件：(a) `turn-id-unfreeze` change 记录 codex PR #28002/#27996 满足条件 #1，第一次评估 4-0 维持冻结；(b) 3 个前置条件（`unify-token-usage-types` / `turn-id-unify` / `recovery-path-explicit`）**首次**全部 spec-complete + code-committed。本 change 触发第二次 mid-freeze 评估。

触发事件：
- **2026-06-13 早些时候**：`turn-id-unfreeze` change archived（4-0 维持冻结）
- **2026-06-13 上午**：`unify-token-usage-types` change archived 2026-06-12（3 个月前完成，但作为 3 前置的 #1 早已完成）
- **2026-06-13 下午**：`turn-id-unify` change archived（commit `c4d388b` + `13bb2fb` 2026-06-13 提交 turn_id 表示收敛代码；< 30 行净变更；spec + 8 artifacts 完整）
- **2026-06-13 下午**：`explicit-recovery-paths` change archived（manifested as `recovery-path-explicit` 的实施版本；~1649 行 + 34/34 micro-tasks + 157 guardian tests + 8 new e2e tests；commit `e4c8d3e`）

codex 增量信号：
- **Codex CLI v0.129 (2026-05-08)**：Session picker 暴露 "Turn count and approximate token usage"——**`usize` 计数器** 暴露，与 Synthia `LoopContext.iteration: usize` 现状一致
- **Codex CLI v0.140 alpha (2026-06-10)**："Multi-Agent v2 Path Tracking"——**未明确**为 `Uuid`；路径追踪 ≠ turn 标识；alpha 信号待 GA

本 change **不实施代码变更**，仅作为元变更（meta-change）：(a) 记录 3/3 前置条件 mid-freeze 完成的触发事件；(b) 重新评估 3 个月冻结决策是否应因前置完成而缩短；(c) 形式化"3/3 完成但维持冻结"的 4-0 共识决策。

## What Changes

**记录 3/3 前置条件 2026-06-13 mid-freeze 完成**
- From: `turn-id-mvp` change 处于 FROZEN 状态（2026-06-13 → 2026-09-13）
- To: `turn-id-mvp` 仍冻结至 2026-09-13，但**记录 3/3 前置条件 spec+code 全完成作为状态变化**
- Reason: 透明化"前置条件 3/3 完成 ≠ 自动解冻"的决策
- Impact: 零代码变更；仅 OpenSpec 元数据更新

**重新评估 3 个月冻结期的合理性（基于 3/3 完成）**
- From: 冻结期 3 个月硬性延迟到 2026-09-13
- To: 维持 3 个月冻结期不缩短，**理由**：
  1. 3/3 前置完成是**实施前置**（减少实施风险），不是**解冻触发**条件
  2. codex v0.129 暴露 "Turn count" 用 `usize` 而非 `Uuid`——工业级实践未用 typed UUID
  3. codex v0.140 alpha 未明确 multi-agent 用 `Uuid` typed ID
  4. 0 production caller 需要 multi-agent 跨 turn 关联
- Reason: 维持"speculative architecture 应被推迟"项目原则；保留 3 个月观察窗口
- Impact: 零代码变更；决议记录在本 change

**标记 codex v0.129 + v0.140 alpha 为观察信号（observational signal）**
- From: 无外部观察信号
- To: 解冻后实施 TurnId MVP 时，可参考 codex v0.129 + v0.140 GA 后的工业级细节，但**不复制**
- Reason: v0.129 = `usize` 实践（与我们一致）；v0.140 alpha = 待观察 multi-agent typed ID 落地
- Impact: 解冻后实施时有外部参考；本 change 不做实际参考分析

**禁止本 change 自身实施 TurnId**
- ❌ 不创建 `crates/synthia-agent/src/turn.rs`
- ❌ 不修改 `LoopContext`
- ❌ 不修改 `StreamBuilder`
- ❌ 不修改 `synthia-hook::AgentContext`
- 实施仍归 `turn-id-mvp` change（解冻后由其 tasks.md 执行）

## Capabilities

### New Capabilities

- `turn-id-mvp-thaw-eval-2026-06-13`: 元变更（meta-change）—— 记录 3/3 前置条件 mid-freeze 完成事件、基于 4 派共识重新评估冻结期、把"3/3 完成但维持冻结"决策形式化。**本 change 0 代码变更**，仅 OpenSpec 元数据 + 决策记录

### Modified Capabilities

- `turn-id-label`: 仍处于 FROZEN 状态。变更：spec 内"Upon thaw" 段落的"thaw trigger"项追加 3/3 前置完成作为**观察信号**（**不缩短冻结期**），并保留 codex PR #28002/#27996 作为条件 #1 已满足的证据（来自 `turn-id-unfreeze`）

## Impact

**冻结期影响（2026-06-13 → 2026-09-13）：**
- 零代码变更
- `turn-id-mvp` 状态保持 FROZEN
- `turn-id-mvp-thaw-eval-2026-06-13` change 记录在 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/`
- 监控 codex v0.140 GA（约 2026-07 至 2026-08）+ Synthia 内部 multi-agent caller 出现情况

**解冻后影响（2026-09-13 起，如果决定解冻）：**
- 仍由 `turn-id-mvp` change 的 tasks.md 执行 ~20 行 MVP
- 可选子任务：阅读 codex v0.129 turn count 暴露 + v0.140 GA 后 multi-agent typed ID 细节，写 "synthia-vs-codex Turn design notes" markdown（**仅做参考分析，不复制任何代码**）
- 前置条件未完成时：继续 FROZEN，到 2026-12-13 硬截止时归档

**Affected code（本 change）：**
- 无（本 change 0 代码变更）

**Affected code（仅当 `turn-id-mvp` 解冻后实施时）：**
- 新增 `crates/synthia-agent/src/turn.rs`（~10 行）
- `crates/synthia-agent/src/loop_context.rs` 加 1 个字段
- `crates/synthia-agent/src/stream_builder/builder.rs:360` 字符串构造替换（`crate::turn_id::format_turn_id(ctx.iteration)` 改为 `ctx.current_turn_id`）
- `crates/synthia-agent/src/turn_id.rs` 在 `turn-id-mvp` 解冻后**删除**（`format_turn_id` 函数变 1 行删除 + `AgentContext.turn_id` 升级为 `Option<TurnId>`）
- 可选：`synthia-hook::AgentContext.turn_id` 类型从 `String` 升级为 `TurnId`

**风险等级：极低**（本 change 0 代码变更；解冻决策仅元数据；实际 MVP 仍受 3 个月冻结期门控）

**关键缓解：**
1. 本 change 不实施 MVP；MVP 仍归 `turn-id-mvp` change，受其 3 个月冻结期门控
2. 解冻决策的发起方是 4 派共识（怀疑/架构/生产/简化 4-0），不是内部推测
3. 3 个月冻结期不缩短，保留"speculative architecture 应被推迟"项目原则的克制
4. codex 设计仅作 reference，Synthia 走简化派 MVP（~20 行）而非 codex 全量（3000+ 行）
