<!--
Raw capture of brainstorming for the turn-id-unfreeze change.

**重要：此 change 是元变更（meta-change），0 代码变更。**
本档是 raw capture，记录"为何元变更不需走完整 4 派对抗性审查"的论证 + 复用 turn-id-mvp 4 派审查结论的边界。
design.md 已含完整 D1-D6 决议（其中 D1 决定"本 change 是元变更"）。
-->

# Brainstorm: turn-id-unfreeze (META-CHANGE)

## 背景（Context）

### 触发事件（2026-06-13 当日）

OpenAI codex 团队合并 2 个 PR，**直接满足 `turn-id-mvp` 解冻条件 #1**：
- codex PR #28002 `[codex] Send turn state through compact requests`
- codex PR #27996 `[codex] Send request-scoped turn state over WebSocket`

PR #27996 描述原文：
> "Turn state is scoped to one logical turn, but the WebSocket path currently exchanges it through upgrade headers, which are scoped to the physical connection. A connection may be reused across turns, so its handshake cannot represent the turn lifecycle reliably."

### 设计探索来源

**复用 `turn-id-mvp/brainstorm.md` 的 4 派对抗性审查结论**：
- 怀疑派：拒绝完整 Turn 模型（13 字段 + 状态机 + 4 事件），但接受元变更形式记录触发证据
- 架构派：元变更形式不引入新抽象，与现有 OpenSpec 元数据层一致
- 生产派：解冻决策不应"立即实施"，应保持 3 个月窗口
- 简化派：元变更可即时完成（~4 个 OpenSpec artifacts + 1 commit），不需 ~20 行实施

**本 change 的"4 派"被替换为"对元变更本身的可行性论证"**——见下方 §Brainstorm 4 题

---

## Brainstorm 4 题

### Q1: 为什么是元变更（meta-change）而不是直接解冻 turn-id-mvp？

**A.1（候选 B）**：直接把 `turn-id-mvp/` 目录从 FROZEN 改为 READY-TO-IMPLEMENT
- ❌ 违反 4 派 2026-06-13 达成的"3 个月冻结期不缩短"共识
- ❌ 污染 FROZEN 状态（"边冻结边修改"逻辑矛盾）
- ❌ 失去 3 个月观察 codebase 状态变化的机会

**A.2（候选 C，本选择）**：独立 `turn-id-unfreeze/` 目录，仅做"记录 + 评估 + 决策"
- ✅ 4 派 2026-06-13 共识明确"元变更形式"是允许的
- ✅ OpenSpec 元数据层隔离，不污染 FROZEN 状态
- ✅ 保留 3 个月观察窗口 + 决策可追溯

**决议 D1**：本 change 是元变更，不实施代码

### Q2: codex 触发的条件 #1 满足后，是否应缩短 3 个月冻结期？

**A.1（候选 B，立即解冻）**：2026-06-13 当日就允许实施 MVP
- ❌ 违反"speculative architecture 应被推迟"项目原则
- ❌ 前置条件（TokenUsage 收敛 / turn_id 表示收敛 / recovery path 显式化）未完成时实施 MVP 会引入第 6 个 turn_id 表示

**A.2（候选 C，本选择）**：维持 3 个月冻结期不缩短
- ✅ 保留项目原则的克制
- ✅ codex 工业级证据已永久记录，未来解冻时无需再次论证
- ✅ 避免"破窗效应"——未来类似场景（外部 PR 触发条件）可参考本 change 的"维持冻结期"先例

**决议 D2**：维持 3 个月冻结期不缩短

### Q3: codex 设计（3000+ 行）是否应复制到 Synthia？

**A.1（候选 B，复制全量）**：
- ❌ 4 派 2026-06-13 一致拒绝（YAGNI、scope 差异、依赖差异）

**A.2（候选 C，复制子集）**：
- ⚠️ MVP 阶段不需要 metadata / persistence，复制子集是 YAGNI 反例

**A.3（候选 D，本选择）**：仅作 reference，不复制
- ✅ 解冻后实施 MVP 时可参考 codex 设计理解工业级语义
- ✅ Synthia 仍走简化派 MVP（~20 行），不引入完整状态机
- ✅ "codex 3000+ 行 vs Synthia 20 行"的 scope 差异保留

**决议 D3**：codex 设计仅作 reference，不复制

### Q4: 元变更自身如何推进？

**A.1（候选 B，重新走 4 派对抗性审查）**：
- ❌ 元变更不实施代码，无"代码方案"可审查
- ❌ 与 `turn-id-mvp` 共享同一组解冻条件，重复审查无新增信息

**A.2（候选 C，本选择）**：复用 `turn-id-mvp` 4 派审查结论 + 在本 brainstorm 单独论证 4 个元变更决策点
- ✅ 论证 4 题（Q1-Q4）已涵盖"是否元变更 / 是否缩短 / 是否复制 / 如何推进"
- ✅ 与 design.md D1-D6 决议一一对应

**决议 A.4**：复用 turn-id-mvp 审查 + 本档 4 题论证

---

## 关键参考

- `openspec/changes/turn-id-mvp/brainstorm.md`（4 派对抗性审查原始记录）
- `openspec/changes/turn-id-mvp/design.md`（D1-D6 决议）
- `openspec/changes/turn-id-mvp/tasks.md`（冻结期任务）
- codex PR #28002 + #27996（外部工业级证据）
- 3 个前置条件（unify-token-usage-types / turn-id-unify / recovery-path-explicit）

---

## 下一阶段

brainstorm → design (D1-D6 已就绪) → proposal (已就绪) → specs (已就绪) → tasks (已就绪) → plan → verify → retrospective → commit → archive
