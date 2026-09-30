<!--
Raw capture of brainstorming for the turn-id-mvp-thaw-eval-2026-06-13 change.

**重要：此 change 是二次元变更（meta-change），0 代码变更。**
本档记录"为何在 3/3 前置条件全完成时仍维持冻结"的 4 派对抗性审查 + 与 `turn-id-unfreeze` (2026-06-13) 第一次评估的差异。

Context 摘要（详见 design.md D1-D6）：
- 2026-06-13 第一次评估（turn-id-unfreeze）：codex PR #28002+#27996 满足条件 #1，4 派维持冻结
- 2026-06-13 第二次评估（本 change）：3/3 前置条件全完成（unify-token-usage-types + turn-id-unify + recovery-path-explicit），codex 工业级证据增量（v0.129 turn count + v0.140 multi-agent path tracking）
-->

# Brainstorm: turn-id-mvp-thaw-eval-2026-06-13 (META-CHANGE #2)

## 背景（Context）

### 触发事件：3/3 前置条件首次全完成

2026-06-13 第一次评估（`turn-id-unfreeze`）时，3 个前置条件进度为 0/3：
- `unify-token-usage-types` ⏳
- `turn-id-unify` ⏳
- `recovery-path-explicit` ⏳

2026-06-13 当日稍后（本 change 触发），3 个前置条件**首次**全部 spec-complete + code-committed：

| 前置条件 | Archived 日期 | 规模 | Spec / Code 状态 |
|----------|------------|------|-----------------|
| `unify-token-usage-types` | 2026-06-12 | ~250 行净变更 | ✓ spec + ✓ code |
| `turn-id-unify` | 2026-06-13 | < 30 行净变更（commit `c4d388b` + `13bb2fb` 2026-06-13） | ✓ spec + ✓ code |
| `recovery-path-explicit` (manifested as `explicit-recovery-paths`) | 2026-06-13 | ~1649 行 + 34/34 micro-tasks + 157 guardian tests + 8 new e2e tests | ✓ spec + ✓ code |

这是 `turn-id-mvp` change 自 2026-06-13 启动以来，**3 个前置条件首次**全部 spec-complete（0/3 → 3/3 在同一日完成最后 2 个）。code-committed 状态：3/3 全部 commit 到 master。

### 与第一次评估（turn-id-unfreeze）的差异

| 维度 | 第一次评估（turn-id-unfreeze, 2026-06-13） | 第二次评估（本 change, 2026-06-13 mid-freeze） |
|------|-------------------------------------------|------------------------------------------------|
| 触发证据 | codex PR #28002 + #27996 满足条件 #1 | 3/3 前置条件 spec+code 完成 |
| 4 派立场 | 4-0 维持冻结（怀疑/架构/生产/简化） | 4-0 维持冻结（怀疑/架构/生产/简化） |
| 关键论据 | "speculative architecture 应被推迟" + 3 前置未完成 | 3 前置完成 ≠ 自动解冻；codex v0.129 暴露 "Turn count" 用 `usize` 而非 `Uuid`；3 个月观察窗口本身有独立价值 |
| codex 工业证据 | 2296 行 + 391 + 349 + 241 行（turn.rs 核心） | 增量信号：v0.129 (2026-05-08) "Turn count" 暴露 + v0.140 alpha (2026-06-10) "Multi-Agent v2 Path Tracking" |
| 决议 | 维持冻结 2026-09-13 | 维持冻结 2026-09-13 |

### codex 增量证据

| Codex 事件 | 日期 | 与 `turn-id-mvp` 的关系 | 是否构成解冻条件 #1？ |
|------------|------|------------------------|----------------------|
| PR #28002 + #27996 合并 | 2026-06-13（首次评估已记录） | 跨 compact/WebSocket 传递 turn state | ✓（已记录于 turn-id-unfreeze） |
| Codex CLI v0.129 | 2026-05-08 | Session picker 显示 "Turn count and approximate token usage"（`usize` 计数器暴露） | ❌ 暴露的是 `usize`，非 `Uuid` —— 工业级实践与我们 `LoopContext.iteration: usize` 现状一致 |
| Codex CLI v0.140 alpha | 2026-06-10 | "Multi-Agent v2 Path Tracking" 暗示 multi-agent 场景需 stable turn 标识符 | ⚠️ 信号但**未明确**为 `Uuid`；路径追踪 ≠ turn 标识 |
| Codex Compact 3 层历史管理 | 2026-03-24 | `RolloutItem::TurnContext(turn_context_item)` 持久化 | ❌ 持久化 turn context ≠ 暴露 `TurnId(Uuid)`；codex 选择 `usize` 计数而非 `Uuid` |

**关键观察**：codex 截至 2026-06-13 公开信号中，**没有**任何 `TurnId(Uuid)` 类型的工业级落地。所有"turn 维度的工业级需求"都被 codex 团队用**计数 + context item** 解决，而非 typed UUID。这削弱了"提早解冻 `turn-id-mvp` 借鉴 codex 工业实践"的论据。

---

## 4 派对抗性审查（脑暴 4 题）

### Q1: 3/3 前置条件全完成 = 自动解冻条件满足？

**A.1（候选 B，立即解冻）**：3/3 前置 spec+code 完成 = 解冻条件 #2 完成 = 立即解冻
- ❌ 混淆"前置条件完成"与"解冻决策"——3 前置是**实施前置**，不是**解冻触发**条件
- ❌ 违背 `turn-id-mvp/proposal.md` 中的"3 个月观察窗口"原意——窗口价值是观察 codebase 状态变化，不是条件清单

**A.2（候选 C，本选择）**：3/3 前置完成 ≠ 自动解冻；解冻决策需独立的"是否需要 TurnId(Uuid)?"判断
- ✅ 前置条件是"减少实施风险"，不是"必须有 TurnId(Uuid)"的理由
- ✅ 维持"speculative architecture 应被推迟"项目原则
- ✅ 与 `turn-id-unfreeze` 第一次评估的 4 派结论一致（0-thaw）

**决议 D1**：3/3 前置完成不构成自动解冻条件

### Q2: codex v0.129/v0.140 增量信号是否改变解冻决策？

**A.1（候选 B，codex 多 agent 需求 = 立即解冻）**：v0.140 multi-agent 路径追踪暗示 stable turn ID 需求
- ❌ v0.140 仍为 alpha，且"路径追踪"≠"turn 标识"——未明确是 `Uuid`
- ❌ 0 production caller 在 Synthia 内部需要 multi-agent 跨 turn 关联
- ❌ YAGNI：等到 v0.140 GA + 明确 `Uuid` 落地 + Synthia 出现真实 multi-agent caller，三者同时满足才考虑

**A.2（候选 C，本选择）**：codex 增量信号**加强**了"先观察，不解冻"立场
- ✅ v0.129 暴露 "Turn count" 用 `usize` 而非 `Uuid` —— codex 工业级实践**未**用 typed UUID
- ✅ v0.140 是 alpha 信号，未来 1-2 月可能有更多细节
- ✅ 维持 3 个月窗口至 2026-09-13 + 观察 v0.140 GA + 观察 Synthia 内部 caller 需求

**决议 D2**：codex 增量信号不构成提前解冻论据

### Q3: 维持冻结 vs 立即解冻 的 4 派立场

| 派 | 立场 | 论据 | 反对论据 |
|----|------|------|----------|
| **怀疑派** | 维持冻结 | 3 前置完成 ≠ 自动解冻；MVP 仍无真实 caller；codex 用 `usize` 而非 `Uuid` | "前 1 个月做的工作未直接产生价值" |
| **架构派** | 维持冻结 | 前置条件完成是**实施**前置，不是**解冻**触发；3 个月观察窗口本身有独立价值 | "前 1 个月解冻决策与计划一致" |
| **生产派** | 维持冻结 | 0 production caller；v0.140 是 alpha 不应据此决策 | "若 caller 出现，需重启评估流程" |
| **简化派** | 维持冻结 | 冻结期 0 代码变更零风险；3 个月窗口可观察 v0.140 GA 后的工业级细节 | "机会成本：若 caller 出现需等到 9 月" |

**4 派共识（4-0 维持冻结）**：
- ✅ **怀疑派**：维持冻结（3 前置完成 ≠ 自动解冻）
- ✅ **架构派**：维持冻结（实施前置 ≠ 解冻触发）
- ✅ **生产派**：维持冻结（0 caller + v0.140 alpha）
- ✅ **简化派**：维持冻结（3 个月窗口有独立价值）

**决议 D3**：4 派共识 4-0 维持冻结到 2026-09-13

### Q4: 元变更形式 vs 直接修改 turn-id-mvp/？

**A.1（候选 B，直接修改 turn-id-mvp/）**：在 `turn-id-mvp/proposal.md` / `tasks.md` 标记"3 前置完成，2026-06-13 mid-freeze 评估决定维持冻结"
- ❌ 污染 FROZEN 状态（`turn-id-mvp/` 应保持 2026-06-13 当日的冻结快照）
- ❌ 失去"3/3 完成 vs 仍维持冻结"的决策可追溯性

**A.2（候选 C，本选择）**：独立 `turn-id-mvp-thaw-eval-2026-06-13/` 目录，仅做"记录 + 评估 + 决策"
- ✅ 与 `turn-id-unfreeze` (2026-06-13) 形式一致——OpenSpec 元数据层隔离
- ✅ 保留 FROZEN 状态完整性
- ✅ 决策可追溯：未来审阅者看到两个 meta-change（第一次 codex 触发，第二次 3/3 完成触发）= 完整的解冻评估历史

**决议 D4**：本 change 是元变更，0 代码变更，0 `turn-id-mvp/` 目录修改

---

## 关键参考

- `openspec/changes/turn-id-mvp/brainstorm.md`（原始 4 派对抗性审查）
- `openspec/changes/turn-id-mvp/design.md`（D1-D6 决议，3 解冻条件定义）
- `openspec/changes/turn-id-mvp/tasks.md`（冻结期任务清单）
- `openspec/changes/archive/2026-06-13-turn-id-unfreeze/`（第一次评估，4-0 维持冻结）
- `openspec/changes/archive/2026-06-13-turn-id-unify/retrospective.md`（3/3 前置完成的最末一环）
- `openspec/changes/archive/2026-06-13-explicit-recovery-paths/retrospective.md`（recovery-path-explicit 完成记录）
- codex v0.129 + v0.140 alpha（2026-05-08 + 2026-06-10 增量信号）

---

## 下一阶段

brainstorm → design (D1-D6 已就绪) → proposal (已就绪) → specs (已就绪) → tasks (已就绪) → plan → verify → retrospective
