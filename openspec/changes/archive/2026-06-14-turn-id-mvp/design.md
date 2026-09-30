## Context

### 背景

2026-06-13 启动了 Codex session/Turn 模型的设计提案，包含 `TurnId(Uuid)` / `TurnStatus` / `Turn` struct（13 字段）+ 4 个新 `AgentEvent` 变体 + `StreamBuilder.current_turn` 集成（~150 行）。4 派对抗性审查 + 苏格拉底式拆解达成共识：**拒绝当前形态**。

但简化派提出了一个最小可行版本（MVP）：**仅保留 `TurnId(Uuid)` 类型 + `LoopContext.current_turn_id` 字段**，约 20 行。

本 change **冻结 3 个月**（2026-06-13 → 2026-09-13），期间不实施；解冻后严格按简化派 MVP 实施。

### 4 派审查裁决汇总

| 角色 | 裁决 |
|------|------|
| 怀疑派 | ❌ 拒绝 |
| 架构派 | ❌ 拒绝 |
| 生产派 | ⚠️ 有条件通过 |
| 简化派 | ❌ 拒绝（砍掉重写）→ 提出 MVP |
| 综合 | ❌ 拒绝当前形态；接受 MVP（冻结） |

### 简化派 MVP（解冻后实施的内容）

```rust
// crates/synthia-agent/src/turn.rs
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(pub Uuid);

impl TurnId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}
```

配套：
- `LoopContext` 加 `current_turn_id: Option<TurnId>` 字段
- 替换 `builder.rs:327` 的 `format!("turn-{}", ctx.iteration)` 为 `ctx.current_turn_id`

### 冻结期内的前置任务

3 个正交前置任务必须先完成（与 Turn MVP 独立）：

1. **TokenUsage 收敛**（已启动 `unify-token-usage-types` change）
2. **turn_id 表示收敛**（未启动，hook context 4 处 String/u64/usize 统一）
3. **recovery path 显式化**（未启动）

---

## Goals / Non-Goals

**Goals:**

- G1: 冻结简化派 MVP 提案 3 个月
- G2: 解冻后严格按 MVP 实施（~20 行）
- G3: 保留"按 turn 维度查询"的可观测性扩展能力
- G4: 不引入 struct、不引入状态机、不引入新事件

**Non-Goals:**

- N1: ❌ 完整 `Turn` struct（13 字段）
- N2: ❌ `TurnStatus` 枚举
- N3: ❌ 4 个新 `AgentEvent` 变体
- N4: ❌ 独立 `turns.jsonl` 持久化
- N5: ❌ RAII `TurnGuard`
- N6: ❌ Turn 级 checkpoint
- N7: ❌ turn_id 表示发散

---

## Decisions

### D1: 简化派 MVP 作为唯一可实施方案

- **选择**：解冻后严格按简化派 MVP 实施
- **理由**：
  1. 4 派审查达成"拒绝当前形态"共识
  2. 简化派 MVP 是"YAGNI 极限"——仅解决"跨事件关联 Turn 标识"一个真实问题
  3. 实施成本 < 20 行，零破坏性变更
- **已考虑 alternative**：
  - **B. 完整 Turn struct** → 4 派一致拒绝
  - **C. 推迟 6 个月** → 与项目工作流 6 个月原则一致，但 MVP 已足够小，可缩短到 3 个月

### D2: 冻结期 3 个月（2026-06-13 → 2026-09-13）

- **选择**：3 个月冻结期
- **理由**：
  1. 3 个月足够观察"按 turn 维度查询"的真实 caller
  2. 与"3 个正交前置任务"的实施时间对齐
  3. 比 6 个月项目原则更短——MVP 风险极低，可加速
- **解冻条件**（满足任一）：
  - 出现"按 turn 维度查询"的真实 caller
  - 用户主动请求解冻
  - 3 个前置任务全部完成
- **不满足时**：
  - 归档到 `openspec/changes/archive/turn-id-mvp-frozen/`
  - 6 个月（2026-12-13）后再次评估
- **2026-06-13 状态变更**：用户主动请求解冻（条件 #2 命中）。3 个月观察窗口作废，进入实施阶段。3 个前置任务（unify-token-usage-types 2026-06-12 / turn-id-unify 2026-06-13 / recovery-path-explicit 2026-06-13）均已完成。`turn-id-mvp-thaw-eval-2026-06-13` meta-change 的"维持冻结"决议被用户直接覆盖；本 change 按 D1 简化派 MVP 严格实施。

### D3: `TurnId` 放在 `synthia-agent` crate

- **选择**：`crates/synthia-agent/src/turn.rs`（约 20 行）
- **理由**：
  1. 当前 `iteration` 概念在 `synthia-agent::LoopContext`
  2. Turn MVP 暂时不涉及 session 持久化
  3. 3 个月后如有 session 级需求，可迁移到 `synthia-session`
- **已考虑 alternative**：
  - **B. `synthia-session`** → 与 Session 概念同源，但 MVP 阶段不持久化
  - **C. `synthia-core`** → 违反 YAGNI

### D4: `TurnId` 用 Uuid v4（非 v7）

- **选择**：`Uuid::new_v4()`（完全随机）
- **理由**：
  1. MVP 阶段不要求时间排序
  2. v7 需要 `uuid` crate v1.x 特性
  3. 3 个月后如需 v7，迁移成本低
- **已考虑 alternative**：
  - **B. Uuid v7** → 索引友好但需要更新依赖

### D5: `TurnId` 加 `Serialize/Deserialize` derive

- **选择**：加 derive
- **理由**：
  1. 成本低（derive 宏）
  2. 3 个月后如需 `Turn` struct，`TurnId` 已可序列化
  3. 与 `Uuid` 自带 serde 兼容
- **已考虑 alternative**：
  - **B. 不加 derive** → MVP 阶段不跨进程，但 3 个月后扩展困难

### D6: 不实现 `Display` impl

- **选择**：MVP 阶段不实现 `Display`
- **理由**：
  1. 现有代码用 `format!("turn-{}", ctx.iteration)` 生成字符串
  2. 替换后 `format!("turn-{}", ctx.current_turn_id)` 仍可工作
  3. 如需 `Display`，3 个月后补 1 行 impl
- **已考虑 alternative**：
  - **B. 实现 `Display`** → 1 行成本，但 MVP 阶段 0 个 caller

---

## Risks / Trade-offs

### R1: 冻结期内用户强烈要求完整 Turn 模型

- **Risk**: 生产环境紧急需求，无法等待 3 个月
- **Mitigation**: 解冻后走完整审查流程（不绕过）；临时方案：用 `format!("turn-{}", iteration)` 字符串

### R2: MVP 实施时过度扩展

- **Risk**: 开发者实施时擅自加 struct/事件/持久化
- **Mitigation**:
  - tasks.md 明确禁止扩展
  - OpenSpec verify 检查行数（< 30 行）
  - 实施前重新走 4 派审查

### R3: 3 个月后 codebase 变化大，MVP 失去意义

- **Risk**: TokenUsage 收敛 / turn_id 表示收敛 / recovery path 显式化后，MVP 与 codebase 状态不一致
- **Mitigation**: 解冻时重新评估；如 MVP 不再需要，永久归档

### R4: TurnId 与现有 5 个 turn_id 表示产生第 6 个

- **Risk**: `LoopContext.iteration` (usize) / `AgentContext.turn_id` (String) / `PrefixStabilityEvent.turn_id` (u64) / `ApprovalRequest.turn_id` (String) / `TurnId(Uuid)` 共 5 个
- **Mitigation**: turn_id 表示收敛前置任务必须先完成（解冻条件之一）

### T1: MVP 价值 vs 冻结成本

- **Trade-off**: 3 个月延迟实施 vs 风险控制
- **接受理由**: MVP 风险极低（~20 行，零破坏），3 个月延迟可接受

---

## Migration Plan

本 change **不涉及部署变更**（冻结期 + MVP 实施都是 0 部署影响）。

### 冻结期（2026-06-13 → 2026-09-13）

1. OpenSpec 提案归档到 `openspec/changes/turn-id-mvp/`
2. README 标注 "FROZEN - DO NOT IMPLEMENT"
3. 监控是否有真实 caller 需求
4. 完成 3 个前置任务

### 解冻后实施（如果满足解冻条件）

1. 创建 `crates/synthia-agent/src/turn.rs`（~10 行）
2. `LoopContext` 加 `current_turn_id: Option<TurnId>` 字段
3. `builder.rs:327` 的 `format!("turn-{}", ctx.iteration)` 替换为 `ctx.current_turn_id`
4. 编译验证 + 测试通过
5. 提交 commit，OpenSpec verify + archive

### Rollback 策略

- 本 change 0 破坏性变更
- 如发现新 bug，revert PR 即可
- 冻结期内不解冻，等下次评估

### 验收条件（解冻后）

- [ ] `crates/synthia-agent/src/turn.rs` 文件 < 30 行
- [ ] `LoopContext` 字段数量 + 1
- [ ] `builder.rs:327` 字符串构造替换
- [ ] 32 处外部引用不受影响
- [ ] `cargo check --workspace` 0 错误
- [ ] `cargo test --workspace` 100% 通过
- [ ] `cargo clippy` 0 警告
- [ ] `grep "pub struct TurnId"` 仅返回 `synthia-agent/src/turn.rs`
- [ ] `grep "pub struct Turn\b"` 返回 0 行
- [ ] `grep "TurnStatus"` 返回 0 行
- [ ] `grep "TurnStarted\|TurnCompleted\|TurnFailed\|TurnAborted"` 返回 0 行

---

## Open Questions

### Q1: 解冻后是否需要 `AgentContext.turn_id` 类型升级？

- 当前决策：升级 `String` → `TurnId`（`Hash + Copy`）
- 替代方案：保留 `String`，仅在生成时包装
- 待决原因：影响 hook 消费者代码（synthia-hook 内部）

### Q2: 冻结期内是否需要监控 caller 需求？

- 当前决策：被动监控（用户主动提及）
- 替代方案：主动监控（grep codebase 看是否有 "turn-level" 查询）
- 待决原因：MVP 实施成本低，主动监控可能不必要

### Q3: 6 个月后仍未解冻，是否彻底搁置？

- 当前决策：归档后搁置，等待用户主动提及
- 替代方案：永久删除 change
- 待决原因：与项目工作流"6 个月再评估"原则一致
