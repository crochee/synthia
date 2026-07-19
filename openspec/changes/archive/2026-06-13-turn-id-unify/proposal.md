# turn-id-unify Proposal

## Why

`turn-id-mvp` change（冻结至 2026-09-13）的 3 个正交前置任务之一要求"turn_id 表示收敛"——当前 codebase 中存在 4 个 `turn_id` 概念表示（`usize` / `String` × 2 / `u64`），分散在 4 个 crate 中。其中 3 个是 `LoopContext.iteration: usize` 的派生视图（`builder.rs` 中 3 处构造点），第 4 个是 `ApprovalRequest::NetworkAccess.turn_id: String` 的孤儿字段（5 个 ApprovalRequest variant 中仅 1 个有该字段，且 0 生产 caller）。本 change 实施最小可行收敛路径：**集中格式化函数**（B）+ **删除孤儿字段**（C），< 15 行代码变更，零新类型，零与 `turn-id-mvp` 协调成本。

## What Changes

**集中 turn_id 字符串构造（B）**
- From: `format!("turn-{}", ctx.iteration)` 散布在 `stream_builder/builder.rs:360`
- To: 集中到 `synthia_agent::turn_id::format_turn_id(iter: usize) -> String` 函数
- Reason: 单点定义便于未来 `turn-id-mvp` 解冻时统一升级（`String` → `TurnId`）
- Impact: 1 处替换，零行为变化

**删除 `ApprovalRequest::NetworkAccess.turn_id` 孤儿字段（C）**
- From: `NetworkAccess { id, turn_id, target, host, protocol, port }` + `network_access(id, turn_id, target, host, protocol, port)` 构造函数
- To: `NetworkAccess { id, target, host, protocol, port }` + `network_access(id, target, host, protocol, port)` 构造函数
- Reason:
  1. 5 个 `ApprovalRequest` variant 中仅 `NetworkAccess` 有 `turn_id` 字段（变体内不一致）
  2. 0 生产代码 caller（`grep "ApprovalRequest::network_access" crates/` 仅 1 处测试用 `"t"` 字面量）
  3. Guardian 决策函数（`assess_risk`、`make_guardian_decision`）0 处读取 `turn_id` 字段
- Impact: 破坏性 API 变更（`network_access` 构造函数少 1 参数），但项目内 grep 0 处使用

**保留 #1 `LoopContext.iteration: usize` 和 #3 `PrefixStabilityEvent.turn_id: u64`**
- Reason: 两者是 internal 类型（不暴露给 hook），与 `turn-id-mvp` 的 `TurnId(Uuid)` 解耦成本最低
- Impact: 0 变更

**不引入 `TurnId(Uuid)` 提前**
- Reason: 与 `turn-id-mvp` 协调成本（避免 5.5 个表示），留给 `turn-id-mvp` 解冻时（2026-09-13 后）

## Capabilities

### New Capabilities

- `turn-id-unify`: 实施 turn_id 表示的最小可行收敛（集中格式化 + 删除孤儿字段），< 15 行代码变更，**不引入** `TurnId(Uuid)` 类型

### Modified Capabilities

- `turn-id-label`: 状态不变（仍 FROZEN 至 2026-09-13）。本 change 实施后，`turn-id-mvp` 解冻时 `AgentContext.turn_id: String` 升级为 `Option<TurnId>` 的工作量从 5 行降为 4 行（`format_turn_id()` 集中函数已存在，无需替换）

## Impact

**Affected code（本 change）：**
- 新增 `crates/synthia-agent/src/turn_id.rs`（~5 行）
- `crates/synthia-agent/src/stream_builder/builder.rs:360` 1 行替换
- `crates/synthia-guardian/src/approval_request.rs` 删除 1 字段 + 修改 1 构造函数
- `crates/synthia-guardian/src/guardian_coordinator.rs` 更新 1 测试调用（`network_access("id", "t", "target", "host", "https", 443)` → `network_access("id", "target", "host", "https", 443)`）

**Affected code（仅当 `turn-id-mvp` 解冻后实施时）：**
- `crates/synthia-agent/src/turn.rs` 新增（~10 行，`TurnId(Uuid)` 类型）
- `crates/synthia-agent/src/loop_context.rs` 加 1 字段（`current_turn_id: Option<TurnId>`）
- `crates/synthia-agent/src/stream_builder/builder.rs:360` `format_turn_id(ctx.iteration)` → `ctx.current_turn_id`（集中函数调用替换为字段读取）

**风险等级：极低**
- 0 新类型，0 新依赖
- 1 处破坏性 API 变更（`network_access` 构造函数），但项目内 0 处使用
- 与 `turn-id-mvp` 0 协调成本

**前置任务依赖（必须先完成，本 change 是其中之一）：**
- `unify-token-usage-types`（已 archived 2026-06-12）✓
- `turn-id-unify`（**本 change**）
- `recovery-path-explicit`（未启动）

**关键缓解：**
1. 不引入 `TurnId(Uuid)`，避免与 `turn-id-mvp` 的 `TurnId(Uuid)` 冲突
2. 集中函数命名 `format_turn_id` 与 `turn-id-mvp` 的 `TurnId::new()` 命名风格一致，便于未来升级
3. 破坏性 API 变更（`network_access`）影响范围 grep 已验证 0 生产 caller
