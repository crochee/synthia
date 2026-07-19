## Context

### 背景

`turn-id-mvp` change 自 2026-06-13 启动后被冻结 3 个月（2026-06-13 → 2026-09-13），冻结期内不解冻、不实施。`turn-id-mvp/design.md` D2 列出 3 个解冻条件：

1. **条件 #1**：出现"按 turn 维度查询"的真实 caller
2. **条件 #2**：TokenUsage / recovery path 等其他原语收敛
3. **条件 #3**：3 个月时间窗口

### 第一次评估（`turn-id-unfreeze`, 2026-06-13 当日早些时候）

OpenAI codex 团队合并 2 个 PR（#28002 + #27996）满足条件 #1。`turn-id-unfreeze` change 记录触发证据 + 4 派共识 4-0 维持冻结期不缩短。

### 第二次评估（本 change, 2026-06-13 mid-freeze）

当日稍后，3 个前置条件**首次**全部 spec-complete + code-committed：

| 前置条件 | Archived 日期 | 规模 | 验证 |
|----------|------------|------|------|
| `unify-token-usage-types` | 2026-06-12 | ~250 行净变更 | `openspec list` 显示 archived |
| `turn-id-unify` | 2026-06-13 | < 30 行（commit `c4d388b` + `13bb2fb` 2026-06-13） | `git log --oneline -2` 显示 2 个 refactor commits |
| `recovery-path-explicit` | 2026-06-13 (as `explicit-recovery-paths`) | ~1649 行 + 34/34 micro-tasks + 157 guardian tests + 8 new e2e tests | `openspec list` 显示 archived |

3/3 前置条件完成的实施前置已 100% 满足。

### codex 增量信号（2026-05-08 至 2026-06-10）

| 事件 | 日期 | 与 `turn-id-mvp` 关系 | 类型 |
|------|------|---------------------|------|
| Codex CLI v0.129 | 2026-05-08 | Session picker 显示 "Turn count and approximate token usage"——**`usize` 计数器** 暴露 | 工业级实践 = 与 Synthia `LoopContext.iteration: usize` 一致 |
| Codex CLI v0.140 alpha | 2026-06-10 | "Multi-Agent v2 Path Tracking"——**未明确**为 `Uuid`；路径追踪 ≠ turn 标识 | Alpha 信号，待 GA |
| Codex Compact 3 层历史 | 2026-03-24 | `RolloutItem::TurnContext(turn_context_item)` 持久化 | 持久化 turn context ≠ typed `TurnId(Uuid)` |

**关键观察**：codex 截至 2026-06-13 的公开信号中，**没有** `TurnId(Uuid)` 类型的工业级落地。所有"turn 维度需求"由 codex 团队用**计数 + context item** 解决，而非 typed UUID。这削弱了"提早解冻借鉴 codex 工业实践"的论据。

### 本 change 的定位

本 change **不实施 TurnId MVP**。它是**元变更（meta-change）**，解决以下 3 个问题：

1. **记录触发事件**：3/3 前置条件 2026-06-13 首次全完成 + codex 增量信号
2. **重新评估冻结期**：前置条件全完成 + codex 增量信号 = 是否应立即解冻？
3. **形式化决策**：把"3/3 完成但维持冻结"的决策纳入 OpenSpec 元数据，避免未来误以为条件已自动满足

实施 TurnId MVP 仍归 `turn-id-mvp` change，本 change 只做"记录 + 评估 + 决策"三件事。

### 关键参考

- `openspec/changes/turn-id-mvp/proposal.md`（冻结的 MVP 提案）
- `openspec/changes/turn-id-mvp/design.md`（冻结的设计文档，D1-D6 决议 + 3 解冻条件定义）
- `openspec/changes/turn-id-mvp/tasks.md`（冻结后不解冻的实施任务）
- `openspec/changes/archive/2026-06-13-turn-id-unfreeze/`（第一次评估，4-0 维持冻结）
- `openspec/changes/archive/2026-06-13-turn-id-unify/retrospective.md`（2/3 → 3/3 完成的最后 1 个前置）
- `openspec/changes/archive/2026-06-13-explicit-recovery-paths/retrospective.md`（recovery-path-explicit 完成记录）
- `openspec/changes/archive/2026-06-12-unify-token-usage-types/`（第一个前置）

---

## Goals / Non-Goals

### Goals

- 记录 3/3 前置条件 2026-06-13 mid-freeze 完成的触发事件
- 4 派对抗性审查"3/3 完成 = 自动解冻 vs 维持冻结"决策
- 形式化决议：维持冻结到 2026-09-13（4-0 共识）
- 保留 3 个月观察窗口至 2026-09-13 + 观察 v0.140 GA 后 codex 工业级细节
- 实施 0 代码变更；决策 100% 限于 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` 目录

### Non-Goals

- ❌ 不实施 `TurnId(Uuid)` MVP（仍归 `turn-id-mvp` change）
- ❌ 不修改 `turn-id-mvp/` 目录（FROZEN 状态完整性）
- ❌ 不创建 `crates/synthia-agent/src/turn.rs`
- ❌ 不修改 `LoopContext` / `StreamBuilder` / `synthia-hook::AgentContext`
- ❌ 不复制 codex 任何模块

---

## Decisions

### D1: 3/3 前置完成不构成自动解冻条件
- **理由**：前置条件是"减少实施风险"的实施前置，**不是**"必须有 TurnId(Uuid)"的理由
- **影响**：0 代码变更；记录在本 change

### D2: codex 增量信号（v0.129/v0.140）不构成提前解冻论据
- **理由**：v0.129 暴露 "Turn count" 用 `usize` 而非 `Uuid`；v0.140 alpha 未明确 multi-agent 用 typed UUID
- **影响**：维持 3 个月观察窗口至 2026-09-13

### D3: 4 派共识 4-0 维持冻结
- 怀疑派：3 前置完成 ≠ 自动解冻；MVP 仍无真实 caller；codex 用 `usize` 而非 `Uuid`
- 架构派：实施前置 ≠ 解冻触发；3 个月观察窗口本身有独立价值
- 生产派：0 production caller；v0.140 是 alpha 不应据此决策
- 简化派：冻结期 0 代码变更零风险；3 个月窗口可观察 v0.140 GA 后的工业级细节
- **影响**：决议 D3 = 维持冻结至 2026-09-13；4 派共识一致通过

### D4: 本 change 是元变更（meta-change）
- **理由**：与 `turn-id-unfreeze` (2026-06-13) 形式一致；OpenSpec 元数据层隔离
- **影响**：0 代码变更；0 `turn-id-mvp/` 目录修改

### D5: codex 设计仅作 reference，不复制
- **理由**：codex 走 3000+ 行 Turn 模型，Synthia 走简化派 MVP（~20 行）
- **影响**：解冻后实施 MVP 时可参考 codex 工业级语义，但不复制任何代码

### D6: 触发证据以 codex 版本号 + 文档链接记录
- **理由**：可追溯性强；未来审阅者可直接验证
- **影响**：在本 change `proposal.md` + `design.md` Context 段明确记录 v0.129 / v0.140 alpha 引用

---

## Architecture

无架构变更。本 change 0 代码变更，0 crate 修改。

产出 100% 限于 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` 目录下的 8 个 OpenSpec artifacts：
- `.openspec.yaml`
- `README.md`
- `brainstorm.md`（4 派论证）
- `design.md`（本档）
- `proposal.md`（Why/What Changes/Capabilities/Impact）
- `specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md`（≥ 7 个 ADDED Requirements + Scenarios）
- `tasks.md`（micro-task 清单）
- `plan.md`（实施计划）
- `verify.md`（验证记录）
- `retrospective.md`（回顾）

---

## Risks / Trade-offs

### Risk 1: 维持冻结错过"3/3 完成"的机会窗口
- **风险描述**：若 2026-09-13 前出现真实 multi-agent caller 需求，Synthia 仍要等到 9 月才能实施 MVP
- **缓解**：v0.140 alpha 暗示 multi-agent 路径追踪，但 Synthia 内部 0 production caller；机会窗口为"理论"而非"实际"
- **可接受**：✅ 维持冻结

### Risk 2: codex v0.140 GA 早于 2026-09-13
- **风险描述**：若 codex v0.140 在 2026-07 或 2026-08 GA 且明确 multi-agent 用 `Uuid` typed ID，可能错过参考实现
- **缓解**：本 change 保留 3 个月观察窗口；如果 v0.140 GA 后出现 typed `Uuid` 工业实践，可在 2026-09-13 之前再做**第三次**评估
- **可接受**：✅ 维持冻结

### Trade-off: 维持冻结 vs 立即解冻
- **维持冻结收益**：保留"speculative architecture 应被推迟"项目原则；观察 v0.140 GA；3 个月窗口本身有独立价值
- **立即解冻收益**：3/3 前置完成 = 实施风险最低（0 协调成本）
- **决策**：维持冻结收益 > 立即解冻收益（D3 4 派共识）
