## Why

2026-06-13 启动的完整 Turn 模型提案（13 字段 struct + 4 个新事件 + ~400 行集成）经 4 派对抗性审查后被一致拒绝——零具体使用场景驱动，引入 13 项现有抽象冲突，违反"speculative architecture 应被推迟"的项目原则。但简化派提出了最小可行版本（MVP）：仅 `TurnId(Uuid)` 类型 + `LoopContext.current_turn_id` 字段，约 20 行。

**2026-06-13 用户主动解冻：3 个月观察期（→ 2026-09-13）提前结束；按简化派 MVP 实施。** 0 代码之外的修改：仅实施 MVP 范围；解冻条件 #3（用户主动请求解冻）触发。

## What Changes

**冻结简化派 MVP 提案 3 个月**
- From: 完整 Turn 模型提案（活跃状态）
- To: MVP 提案（冻结状态，2026-06-13 → 2026-09-13）
- Reason: 4 派审查拒绝当前形态；MVP 是唯一可接受方案，但需先验证真实需求
- Impact: 零代码变更（仅 OpenSpec 状态变更）

**解冻后实施（如果满足解冻条件）：TurnId 类型 + LoopContext 字段**
- From: 现有 `LoopContext.iteration: usize`（整数计数器）
- To: 新增 `LoopContext.current_turn_id: Option<TurnId>`（可选 UUID）
- Reason: 提供"跨事件关联 Turn 标识"的可观测性能力，不引入 struct/状态机/新事件
- Impact: 非破坏性；现有 `iteration: usize` 保留，外部 hook context 字符串升级为 `TurnId`

**解冻后实施（如果满足解冻条件）：TurnId 类型定义**
- From: 无 `TurnId` 类型
- To: 新增 `pub struct TurnId(pub Uuid)`（~10 行）
- Reason: 可观测性标签，跨进程稳定 ID
- Impact: 非破坏性；新文件 `crates/synthia-agent/src/turn.rs`

**禁止扩展（解冻后也不做）**
- ❌ 完整 `Turn` struct（13 字段）
- ❌ `TurnStatus` 枚举
- ❌ 4 个新 `AgentEvent` 变体
- ❌ 独立 `turns.jsonl` 持久化
- ❌ RAII `TurnGuard`
- ❌ Turn 级 checkpoint
- ❌ turn_id 表示发散

## Capabilities

### New Capabilities

- `turn-id-label`: 提供 `TurnId(Uuid)` 类型作为可观测性标签，支持跨事件 Turn 标识关联。**冻结 3 个月**，期间不解冻；解冻后仅实施简化派 MVP（~20 行），不扩展为完整 Turn 模型

### Modified Capabilities

（无。完整 Turn 模型相关的 spec 不存在，4 派审查拒绝引入）

## Impact

**冻结期影响：**
- 零代码变更
- OpenSpec change 状态变更为 FROZEN
- 监控是否有"按 turn 维度查询"的真实 caller

**解冻后影响（如果实施）：**
- 新增文件 `crates/synthia-agent/src/turn.rs`（~10 行）
- `crates/synthia-agent/src/loop_context.rs` 加 1 个字段
- `crates/synthia-agent/src/stream_builder/builder.rs:327` 字符串构造替换
- `synthia-hook` 内部 `AgentContext.turn_id` 类型可能从 `String` 升级为 `TurnId`

**前置任务依赖（必须先完成）：**
1. `unify-token-usage-types` change（已启动，TokenUsage 收敛）
2. turn_id 表示收敛（未启动，hook context 4 处 String/u64/usize 统一）
3. recovery path 显式化（未启动，`builder.rs:355-363` 的 `continue` 修复）

**风险等级：极低**（MVP < 20 行，零破坏性变更；冻结期 0 代码变更）
</content>
</invoke>