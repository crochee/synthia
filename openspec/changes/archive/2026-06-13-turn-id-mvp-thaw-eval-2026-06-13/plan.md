# turn-id-mvp-thaw-eval-2026-06-13 Implementation Plan (META-CHANGE #2)

> **重要：本 change 是二次元变更（meta-change #2），0 代码变更。**
> **本 plan.md 仅记录"写 8 个 artifact + 1 个手动归档"流程。**
> **TurnId MVP 实施仍归 `turn-id-mvp/tasks.md`（受 3 个月冻结期门控 2026-09-13 解冻后执行）。**

**Goal:** 把"3/3 前置条件 2026-06-13 mid-freeze 完成 + 4 派共识 0-thaw 决策"以 OpenSpec 元数据形式记录，让 `turn-id-mvp` 状态变化有完整可追溯的决策链。**0 代码变更，0 实施风险**。

**Architecture:** 不修改任何 crates 代码；不修改 `turn-id-mvp/` 目录；不创建新 source 文件；不修改 `LoopContext` / `StreamBuilder` / `synthia-hook::AgentContext`。本 change 的产出 100% 限于 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` 目录下的 8 个 OpenSpec artifacts。

**Tech Stack:** OpenSpec, Markdown, Git (gitignored openspec/)

---

## 任务列表

### 1. 完成 8 个 OpenSpec artifacts

- [x] 1.1 `.openspec.yaml` (schema + created date)
- [x] 1.2 `README.md` (决议摘要)
- [x] 1.3 `brainstorm.md` (4 派论证 + 4 题脑暴 + 与第一次评估的差异)
- [x] 1.4 `design.md` (D1-D6 决议 + Context/Goals/Architecture/Risks)
- [x] 1.5 `proposal.md` (Why/What Changes/Capabilities/Impact)
- [x] 1.6 `specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md` (8 个 ADDED Requirements + Scenarios)
- [x] 1.7 `tasks.md` (7 个 task group: 记录 + 评估 + 4 派 + 形式化 + 验证 + 监控 + 硬截止)
- [x] 1.8 `plan.md` (本档)

### 2. 验证

- [ ] 2.1 运行 `openspec validate turn-id-mvp-thaw-eval-2026-06-13 --type change --strict` 期望通过
- [ ] 2.2 运行 `openspec show turn-id-mvp-thaw-eval-2026-06-13` 验证 8 个 artifact 全部存在
- [ ] 2.3 验证 `turn-id-mvp/` 目录未被修改（`git status` 期望无变化）
- [ ] 2.4 验证本 change 0 代码变更（`git diff --stat crates/` 期望空输出）
- [ ] 2.5 写 `verify.md` (验证记录)
- [ ] 2.6 写 `retrospective.md` (元变更 #2 流程经验总结)

### 3. 手动归档

> **注**：项目 `openspec/` 是 gitignored（项目记忆约束），不能 `git commit` 归档内容。
> 与 `turn-id-unfreeze` (2026-06-13) 的归档方式一致：手动复制到 `archive/` 目录。

- [ ] 3.1 复制 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` → `openspec/changes/archive/2026-06-13-turn-id-mvp-thaw-eval-2026-06-13/`
- [ ] 3.2 同步 spec 到 `openspec/specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md`
  - **注**：delta 格式 `## ADDED Requirements` 需改为 cumulative `## Requirements`（项目记忆：openspec spec validate 检查 "Requirements" 而非 "ADDED Requirements"）
- [ ] 3.3 删除 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` 活跃目录
- [ ] 3.4 `openspec list` 验证活跃列表自然剔除本 change

---

## 验证标准

- **OpenSpec validate**: pass
- **turn-id-mvp/ unmodified**: 0 文件变更
- **0 code changes**: `git diff --stat crates/` 空输出
- **8 artifacts**: `.openspec.yaml` + `README.md` + `brainstorm.md` + `design.md` + `proposal.md` + `specs/.../spec.md` + `tasks.md` + `plan.md` + `verify.md` + `retrospective.md` = 10 个文件
- **8+ ADDED Requirements** with at least 1 Scenario each
- **4-0 4 派共识** recorded in brainstorm.md

---

## 关键差异（与 `turn-id-unfreeze` 第一次评估对比）

| 维度 | `turn-id-unfreeze` (#1) | `turn-id-mvp-thaw-eval-2026-06-13` (#2) |
|------|------------------------|----------------------------------------|
| 触发证据 | codex PR #28002+#27996（条件 #1 满足） | 3/3 前置条件全完成（**实施前置**，非解冻触发） |
| 4 派立场 | 4-0 维持冻结 | 4-0 维持冻结 |
| 关键论据 | "speculative architecture 应被推迟" + 3 前置未完成 | 3 前置完成 ≠ 自动解冻；v0.129 用 `usize`；3 个月窗口有独立价值 |
| codex 工业证据 | 2296 + 391 + 349 + 241 行（turn.rs 核心） | 增量：v0.129 turn count + v0.140 alpha multi-agent path tracking |
| 决议 | 维持冻结 2026-09-13 | 维持冻结 2026-09-13 |
| 监控项 | codex 后续 PR | v0.140 GA + Synthia 内部 caller 出现 |

---

## 衔接说明（解冻后执行）

本 change 0 代码变更。解冻后（2026-09-13 起）的实施仍归 `turn-id-mvp/tasks.md` 2.1-2.6 节：
- 创建 `crates/synthia-agent/src/turn.rs`（< 30 行）
- 在 `LoopContext` 加 `current_turn_id: Option<TurnId>` 字段
- 替换 `builder.rs:360` 字符串构造
- 验证、grep 审计、提交
- 同步归档本 change 与 `turn-id-unfreeze` 到 `archive/`
