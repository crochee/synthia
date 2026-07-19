<!--
Raw capture of multi-expert adversarial review (acting as brainstorming)
for the turn-id-mvp change.

**重要：此 change 在 2026-06-13 完成多专家对抗性审查后被冻结 3 个月。**
预计解冻时间：2026-09-13。解冻条件：3 个月内出现"按 turn 维度查询"的真实 caller。

**2026-06-13 更新：用户主动请求解冻（提前 3 个月）。解冻触发条件 #3（用户主动请求）命中。冻结期 3 个月窗口作废，进入实施阶段。** 0 代码之外的修改：仅实施 MVP 范围；reason = 用户判断。

本档是 raw capture, 包含审查背景、决策链、设计取捨。
design.md 将从本档萃取并重新整理。

设计探索来源：4 派对抗性审查（怀疑派、架构派、生产派、简化派）+ 苏格拉底式拆解
共识结论：拒绝完整 Turn 模型提案，但可接受简化派 MVP（~20 行）
-->

# Brainstorm: turn-id-mvp (THAWED 2026-06-13)

## 背景（Context）

### 提案历史

2026-06-13 启动了 Codex session/Turn 模型的全面设计提案，包含：
- `synthia-session/src/turn.rs`（约 150 行）含 `TurnId(Uuid)` / `TurnStatus` / `Turn` struct（13 字段）
- `AgentEvent` 4 个新变体（`TurnStarted/Completed/Failed/Aborted`）
- `StreamBuilder.current_turn: Option<Turn>` + `start_turn`/`end_turn` 流程

声称的 4 个使用场景：
- session resume
- audit trail
- multi-agent isolation
- compaction trigger

### 4 派对抗性审查结果（2026-06-13）

| 角色 | 裁决 | 核心立场 |
|------|------|----------|
| 怀疑派 | ❌ 拒绝 | 5 个隐藏假设全部未验证，4 场景零增量价值 |
| 架构派 | ❌ 拒绝 | 13 项现有抽象冲突，4 场景全部有现成方案 |
| 生产派 | ⚠️ 有条件通过 | 方向对但 5/12 退出路径会破坏 end_turn 配对 |
| 简化派 | ❌ 拒绝（砍掉重写） | 13 字段 12 个 YAGNI，~400 行集成 vs 0 行最简方案 |
| 综合 | ❌ 拒绝当前形态 | 3/4 拒绝，1/4 附带 3 个强约束 |

### 简化派 MVP 提案（被采纳的最小单元）

```rust
// crates/synthia-agent/src/turn.rs (拟新建, ~20 行)
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(pub Uuid);

impl TurnId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}
```

配套：
- `LoopContext` 加 1 个字段：`current_turn_id: Option<TurnId>`
- 替换 `builder.rs:327` 的 `format!("turn-{}", ctx.iteration)` 为 `ctx.current_turn_id`
- 不引入 `Turn` struct、不引入 `TurnStatus`、不引入新事件、不持久化

### 决策

**MVP 冻结 3 个月**（2026-06-13 → 2026-09-13）。

理由：
1. **遵循项目工作流原则**："First fix critical bugs and remove duplicate code, then discuss architectural abstractions after a stabilization period (6 months)"
2. **避免 speculative abstraction**：4 派审查未发现具体 caller 需求
3. **降低风险**：MVP 仅 ~20 行，3 个月内 codebase 状态可能变化（如 TokenUsage 收敛后）
4. **保留选项**：3 个月后如有真实需求，可快速解冻实施

### 冻结期内的前置任务

3 个正交前置任务必须先完成（与 Turn MVP 独立）：

1. **TokenUsage 收敛**（已启动 `unify-token-usage-types` change）
2. **turn_id 表示收敛**（未启动，hook context 4 处 String/u64/usize 统一）
3. **recovery path 显式化**（未启动，`builder.rs:355-363` 的 `continue` 修复）

---

## 问题链（Decision Chain）

### Q1: 简化派 MVP 是否真的"最小可行"？

**共识**：是。`TurnId(Uuid)` 作为可观测性标签，不引入 struct、不引入状态机、不引入新事件。

### Q2: 为什么用 Uuid 而非 usize iteration？

候选：
- A. `usize`（直接复用 `LoopContext.iteration`）—— 零成本，零类型
- B. `Uuid` —— 跨进程稳定，可关联外部系统

**决策**：B. `Uuid`。
理由：
- `LoopContext.iteration` 已经在内部用，外部 hook context 收到 `String`（builder.rs:327 的 `format!("turn-{}", ctx.iteration)`）—— 引入 `TurnId(Uuid)` 让外部观察者看到稳定 ID
- Uuid 在分布式 trace 中有优势
- 成本：~100ns 一次，3 个月内不成为瓶颈

### Q3: TurnId 应该放在哪个 crate？

候选：
- A. `synthia-session` —— 与 Session/Turn 概念同源
- B. `synthia-agent` —— Turn 是 agent 概念
- C. `synthia-core` —— 最低层

**决策**：B. `synthia-agent`（MVP 阶段）。
理由：
- 当前 `iteration` 概念在 `synthia-agent::LoopContext`
- Turn MVP 暂时不涉及 session 持久化
- 3 个月后如有 session 级需求，迁移到 `synthia-session`

### Q4: 何时解冻？

**决策**：2026-09-13。
解冻条件（满足任一）：
- 出现"按 turn 维度查询"的真实 caller（如生产审计、计费、debug 工具）
- 用户主动请求解冻
- 相关前置任务（TokenUsage 收敛、turn_id 表示、recovery path）全部完成

不满足解冻条件时：
- 自动归档到 `openspec/changes/archive/turn-id-mvp-frozen/`
- 6 个月（2026-12-13）后再次评估

### Q5: 解冻后实施什么？

**决策**：**严格按简化派 MVP 实施**（~20 行）。
禁止：
- ❌ 扩展为完整 Turn struct
- ❌ 增加新事件变体
- ❌ 引入 TurnStatus 状态机
- ❌ 持久化 Turn 数据

允许：
- ✅ `TurnId(Uuid)` 类型定义
- ✅ `LoopContext.current_turn_id: Option<TurnId>` 字段
- ✅ 替换 `builder.rs:327` 的字符串构造
- ✅ 把 `current_turn_id` 透传到 hook context（`AgentContext.turn_id` 类型升级为 `TurnId`）

---

## 设计取捨（Design Trade-offs）

### 取捨 1: Uuid v4 vs v7

候选：
- v4：完全随机，无时间信息
- v7：时间排序，索引友好

**决策**：v4（标准 `Uuid::new_v4()`）。
理由：
- MVP 阶段不要求时间排序
- v7 需要 `uuid` crate v1.x 特性
- 3 个月后如需 v7，迁移成本低

### 取捨 2: 序列化 derive

`TurnId` 需要 `Serialize/Deserialize` 吗？

候选：
- A. 加 derive —— 支持跨进程 ID 传输
- B. 不加 —— MVP 阶段不跨进程

**决策**：A. 加 derive。
理由：
- 成本低（derive 宏）
- 3 个月后如需 `Turn` struct，`TurnId` 已可序列化
- 与 `Uuid` 自带 serde 兼容

### 取捨 3: 是否需要 `Display` impl？

MVP 阶段 `format!("turn-{}", ctx.iteration)` 被替换为 `format!("turn-{}", ctx.current_turn_id)`。
如果 `TurnId` 有 `Display` impl，hook 消费者代码更简洁。

**决策**：MVP 阶段不实现 `Display`。
理由：
- 外部 hook 消费者收到 `String`（如 `tracing::info!(turn_id = %turn_id)`）
- 现有代码用 `format!("turn-{}", ctx.iteration)` 生成字符串，转 `TurnId` 后改 `format!("turn-{}", ctx.current_turn_id)`（即 `format!("turn-{}", Uuid)`）也能工作
- 如需 `Display`，3 个月后补 1 行 impl

---

## 风险与缓解（Risks & Mitigations）

| 风险 | 严重性 | 缓解 |
|------|--------|------|
| 3 个月内用户强烈要求完整 Turn 模型 | 中 | 解冻后走完整审查流程（不绕过） |
| MVP 实施时过度扩展 | 高 | tasks.md 明确禁止扩展；OpenSpec verify 检查 |
| 3 个月后 codebase 变化大，MVP 失去意义 | 低 | 解冻时重新评估 |
| TurnId 与现有 turn_id 5 个表示产生第 6 个 | 中 | turn_id 表示收敛前置任务先完成 |
| 冻结期内其他 change 与本 change 冲突 | 低 | 冻结状态在 OpenSpec metadata 中标注 |

---

## 范围边界（Scope Boundary）

### 冻结期可做（IN SCOPE for frozen period）

- ✅ OpenSpec 提案归档
- ✅ README 标注冻结状态
- ✅ 监控是否有真实 caller 需求

### 解冻后可做（POST-THAW IN SCOPE）

- ✅ 创建 `TurnId(Uuid)` 类型（~10 行）
- ✅ `LoopContext.current_turn_id: Option<TurnId>` 字段（1 行）
- ✅ 替换 `builder.rs:327` 的字符串构造（1 行）
- ✅ 把 `current_turn_id` 透传到 `AgentContext.turn_id`（类型升级）

### 永不做（FOREVER OUT OF SCOPE）

- ❌ 完整 `Turn` struct（13 字段）
- ❌ `TurnStatus` 枚举
- ❌ 4 个新 `AgentEvent` 变体
- ❌ 独立 `turns.jsonl` 持久化
- ❌ RAII `TurnGuard`
- ❌ Turn 级 checkpoint
- ❌ turn_id 表示发散（必须先收敛到 1 个）
</content>
</invoke>