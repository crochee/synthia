## Why

2026-06-13 启动并冻结的 `turn-id-mvp` change 设定 3 个解冻条件：(1) 出现"按 turn 维度查询"的真实 caller；(2) TokenUsage / recovery path 等其他原语收敛；(3) 3 个月期满。**当日（2026-06-13）OpenAI codex 团队合并的两个 PR 直接满足条件 #1**：

- **codex PR #28002** `[codex] Send turn state through compact requests` — 改动 `codex-rs/core/src/session/turn.rs`
- **codex PR #27996** `[codex] Send request-scoped turn state over WebSocket` — 改动 `codex-rs/core/src/session/turn.rs`

PR #27996 的描述原文："Turn state is scoped to one logical turn, but the WebSocket path currently exchanges it through upgrade headers, which are scoped to the physical connection. A connection may be reused across turns, so its handshake cannot represent the turn lifecycle reliably." —— 这与原提案中"按 turn 维度可观测 / 可关联"的真实需求完全一致，且 codex 已为该需求投入了 2296 行 + 多个相关模块的实现（`turn_timing.rs` 391 行 / `turn_metadata.rs` 349 行 / `turn_diff_tracker.rs` / `state/turn.rs` 241 行 / `context/turn_aborted.rs`），证明这是**工业级真实需求**而非臆想。

本 change **不实施代码变更**，仅作为元变更（meta-change）：(a) 记录 codex 触发的解冻事件；(b) 重新评估 3 个月冻结决策是否仍合理；(c) 把 `turn-id-mvp` 从"FROZEN"状态解冻（unfreeze）到"READY-TO-IMPLEMENT"，但实施仍受原 3 个前置条件约束。

## What Changes

**记录 codex 解冻触发事件**
- From: `turn-id-mvp` change 处于 FROZEN 状态（2026-06-13 → 2026-09-13）
- To: `turn-id-mvp` 仍冻结至 2026-09-13，但**记录 codex PR #28002 / #27996 作为条件 #1 已满足的证据**
- Reason: 透明化"条件 #1 何时被谁触发"，避免未来误以为 MVP 仍处于"无真实 caller"状态
- Impact: 零代码变更；仅 OpenSpec 元数据更新

**重新评估 3 个月冻结期的合理性**
- From: 冻结期 3 个月硬性延迟到 2026-09-13
- To: 维持 3 个月冻结期不缩短，但**明确"条件 #1 已满足，实施仍受前置条件 #2/#3 门控"**
- Reason: 条件 #1 满足后立即解冻会增加破坏"speculative architecture 应被推迟"项目原则的风险；前置条件（TokenUsage 收敛、turn_id 表示收敛、recovery path 显式化）未完成时实施 MVP 仍会与 5 个 turn_id 表示产生第 6 个冲突
- Impact: 零代码变更；决议记录在本 change

**标记 codex 设计为参考实现（reference design）**
- From: 无外部参考实现
- To: 解冻后实施 TurnId MVP 时，可参考 codex `codex-rs/core/src/session/turn.rs` + `turn_metadata.rs` + `turn_timing.rs` 的结构，但**不复制**（Synthia 仍走简化派 MVP 路径）
- Reason: codex 的 Turn 模型远比 Synthia MVP 复杂（13 字段 + 状态机 + 4 事件 + 持久化），Synthia 只需 1 个 `TurnId(Uuid)` 类型；参考 codex 仅用于"确认需求真实存在"和"理解 turn 维度的工业级语义"
- Impact: 解冻后实施时有外部参考；本 change 不做实际参考分析（留作后续 turn-id-mvp 解冻后实施时的子任务）

**禁止本 change 自身实施 TurnId**
- ❌ 不创建 `crates/synthia-agent/src/turn.rs`
- ❌ 不修改 `LoopContext`
- ❌ 不修改 `StreamBuilder`
- ❌ 不修改 `synthia-hook::AgentContext`
- 实施仍归 `turn-id-mvp` change（解冻后由其 tasks.md 执行）

## Capabilities

### New Capabilities

- `turn-id-unfreeze`: 元变更（meta-change）—— 记录 `turn-id-mvp` 解冻触发事件、重新评估冻结期合理性、把解冻决策形式化。**本 change 0 代码变更**，仅 OpenSpec 元数据 + 决策记录

### Modified Capabilities

- `turn-id-label`: 仍处于 FROZEN 状态。变更：spec 内"Upon thaw" 段落的"thaw trigger"项追加 codex PR #28002 / #27996 作为条件 #1 已满足的证据，**但不缩短冻结期**（仍 2026-06-13 → 2026-09-13）

## Impact

**冻结期影响（2026-06-13 → 2026-09-13）：**
- 零代码变更
- `turn-id-mvp` 状态保持 FROZEN
- `turn-id-unfreeze` change 记录在 `openspec/changes/turn-id-unfreeze/`
- 监控 codex 后续是否有 TurnId 相关的二次 PR（compact 请求的状态字段、recovery path、persistence 策略）

**解冻后影响（2026-09-13 起，如果前置条件 #2/#3 完成）：**
- 仍由 `turn-id-mvp` change 的 tasks.md 执行 ~20 行 MVP
- 可选子任务：阅读 codex 2296 行 `turn.rs` 后写一份 "synthia-vs-codex Turn design notes" markdown（**仅做参考分析，不复制任何代码**）
- 前置条件未完成时：继续 FROZEN，到 2026-12-13 硬截止时归档

**Affected code（本 change）：**
- 无（本 change 0 代码变更）

**Affected code（仅当 `turn-id-mvp` 解冻后实施时）：**
- 新增 `crates/synthia-agent/src/turn.rs`（~10 行）
- `crates/synthia-agent/src/loop_context.rs` 加 1 个字段
- `crates/synthia-agent/src/stream_builder/builder.rs:327` 字符串构造替换
- 可选：`synthia-hook::AgentContext.turn_id` 类型从 `String` 升级为 `TurnId`

**风险等级：极低**（本 change 0 代码变更；解冻决策仅元数据；实际 MVP 仍受 3 个前置条件门控）

**关键缓解：**
1. 本 change 不实施 MVP；MVP 仍归 `turn-id-mvp` change，受其 3 个前置条件门控
2. 解冻决策的发起方是 codex 团队（外部工业级证据），不是 Synthia 内部推测
3. 3 个月冻结期不缩短，保留"speculative architecture 应被推迟"项目原则的克制
4. codex 设计仅作 reference，Synthia 走简化派 MVP（~20 行）而非 codex 全量（2296 行）
