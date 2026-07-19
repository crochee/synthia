# turn-id-mvp-thaw-eval-2026-06-13 Tasks

> **重要：本 change 是二次元变更（meta-change #2），仅做"记录 + 评估 + 决策"三件事。**
> **本 tasks.md 0 代码变更任务。**
> **TurnId MVP 实施仍归 `turn-id-mvp/tasks.md`（受 3 个月冻结期门控 2026-09-13 解冻后执行）。**

---

## 1. 记录 3/3 前置条件 mid-freeze 完成事件

- [ ] 1.1 在 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/proposal.md` 明确记录：
  - 3 个前置条件名：`unify-token-usage-types` (2026-06-12) + `turn-id-unify` (2026-06-13) + `recovery-path-explicit` (2026-06-13, manifested as `explicit-recovery-paths`)
  - `turn-id-unify/retrospective.md` 3/3 spec-complete follow-up 引用
  - 第一次评估（`turn-id-unfreeze`, 2026-06-13 早些时候）的差异
- [ ] 1.2 在 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/design.md` Context 段记录：
  - 3 个前置条件的 archive 日期 + 规模
  - 3/3 完成 ≠ 自动解冻的论证
- [ ] 1.3 在 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/brainstorm.md` 记录：
  - 与第一次评估（`turn-id-unfreeze`）的差异表
  - codex 增量信号表

## 2. 重新评估 3 个月冻结期（基于 3/3 完成 + codex 增量信号）

- [ ] 2.1 列出原 3 个解冻条件（来自 `turn-id-mvp/design.md` D2）：
  - [ ] 2.1.1 条件 #1：出现"按 turn 维度查询"的真实 caller
  - [ ] 2.1.2 条件 #2：TokenUsage / recovery path 等其他原语收敛
  - [ ] 2.1.3 条件 #3：3 个月时间窗口（2026-06-13 → 2026-09-13）
- [ ] 2.2 评估 3/3 前置条件完成是否应缩短 3 个月冻结期：
  - [ ] 2.2.1 区分"实施前置"（3 前置）与"解冻触发条件"（3 个 D2 条件）
  - [ ] 2.2.2 记录评估结论：3/3 完成 ≠ 自动解冻（D1 决议）
  - [ ] 2.2.3 记录 4 派立场（4-0 维持冻结）
- [ ] 2.3 评估 codex 增量信号（v0.129 + v0.140 alpha）：
  - [ ] 2.3.1 记录 v0.129 (2026-05-08) "Turn count" 用 `usize` 而非 `Uuid`
  - [ ] 2.3.2 记录 v0.140 alpha (2026-06-10) multi-agent path tracking 未明确为 `Uuid`
  - [ ] 2.3.3 决议：codex 增量信号不构成提前解冻论据（D2 决议）
- [ ] 2.4 评估 codex 设计作为 reference 的边界：
  - [ ] 2.4.1 决议：仅作 reference，不复制（D5 决议）

## 3. 4 派对抗性审查（脑暴 4 题）

- [ ] 3.1 怀疑派立场：3 前置完成 ≠ 自动解冻；MVP 仍无真实 caller；codex 用 `usize` 而非 `Uuid`
- [ ] 3.2 架构派立场：实施前置 ≠ 解冻触发；3 个月观察窗口本身有独立价值
- [ ] 3.3 生产派立场：0 production caller；v0.140 是 alpha 不应据此决策
- [ ] 3.4 简化派立场：冻结期 0 代码变更零风险；3 个月窗口可观察 v0.140 GA 后的工业级细节
- [ ] 3.5 4 派共识决议：4-0 维持冻结到 2026-09-13（D3 决议）

## 4. 形式化决策

- [ ] 4.1 在 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md` 创建 8 个 ADDED Requirements：
  - [ ] 4.1.1 Requirement: 3/3 prerequisite completion event SHALL be recorded
  - [ ] 4.1.2 Requirement: Three-month freeze period SHALL NOT be shortened
  - [ ] 4.1.3 Requirement: codex v0.129 and v0.140 signals SHALL be recorded but SHALL NOT constitute thaw evidence
  - [ ] 4.1.4 Requirement: TurnId MVP implementation SHALL remain gated by 3-month freeze period
  - [ ] 4.1.5 Requirement: codex Turn design SHALL be treated as reference only
  - [ ] 4.1.6 Requirement: FROZEN turn-id-mvp directory SHALL NOT be modified
  - [ ] 4.1.7 Requirement: This change SHALL introduce zero code changes
  - [ ] 4.1.8 Requirement: This change SHALL pass openspec validation
- [ ] 4.2 每个 Requirement 至少 1 个 Scenario（共 ≥ 8 个 Scenario）
- [ ] 4.3 所有 Requirement 第一句包含 SHALL 或 MUST（OpenSpec validate 规则）

## 5. 验证与归档

- [ ] 5.1 运行 `openspec validate turn-id-mvp-thaw-eval-2026-06-13 --type change` 期望通过
- [ ] 5.2 运行 `openspec validate turn-id-mvp-thaw-eval-2026-06-13 --type change --strict` 期望通过
- [ ] 5.3 运行 `openspec show turn-id-mvp-thaw-eval-2026-06-13` 验证 8 个 artifact 全部存在
- [ ] 5.4 验证 `turn-id-mvp/` 目录未被修改：
  - [ ] 5.4.1 `git status openspec/changes/turn-id-mvp/` 期望 "nothing to commit"
  - [ ] 5.4.2 `git diff openspec/changes/turn-id-mvp/` 期望空输出
- [ ] 5.5 验证本 change 0 代码变更：
  - [ ] 5.5.1 `git diff --stat` 仅显示 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` 下的文件
  - [ ] 5.5.2 `git diff --stat crates/` 期望空输出
  - [ ] 5.5.3 `git diff --stat` 不应出现 `crates/synthia-agent/src/turn.rs` / `loop_context.rs` / `stream_builder/builder.rs`
- [ ] 5.6 手动归档（因 openspec/ gitignored）：
  - [ ] 5.6.1 复制 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` → `openspec/changes/archive/2026-06-13-turn-id-mvp-thaw-eval-2026-06-13/`
  - [ ] 5.6.2 同步 spec 到 `openspec/specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md`（cumulative `## Requirements` 格式）
  - [ ] 5.6.3 删除 `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` 活跃目录

## 6. 冻结期监控（2026-06-13 → 2026-09-13）

- [ ] 6.1 监控 codex 后续 PR（每周一次）：
  - [ ] 6.1.1 重点监控 v0.140 GA（约 2026-07 至 2026-08 预期）
  - [ ] 6.1.2 检查 v0.140 multi-agent typed ID 是否落地 `Uuid`
  - [ ] 6.1.3 如发现 v0.140 GA 后明确 `Uuid` typed ID → 触发**第三次** mid-freeze 评估
- [ ] 6.2 监控 Synthia 内部 caller 需求：
  - [ ] 6.2.1 每周 `grep -rn "current_turn_id\|TurnId" crates/` 监控新增 caller
  - [ ] 6.2.2 任何 Synthia 内部 multi-agent 跨 turn 关联需求出现 → 触发第三次 mid-freeze 评估
- [ ] 6.3 等待 2026-09-13 硬解冻日：
  - [ ] 6.3.1 届时由 `turn-id-mvp/tasks.md` 2.1-2.6 节执行 MVP 实施
  - [ ] 6.3.2 本 change 与 `turn-id-unfreeze` 同步归档到 `archive/`

## 7. 6 个月硬截止（2026-12-13）

- [ ] 7.1 如果 2026-09-13 时前置条件仍未完成：
  - [ ] 7.1.1 `turn-id-mvp` 继续 FROZEN
  - [ ] 7.1.2 评估 codex 后续 PR 是否引入新维度
- [ ] 7.2 如果 2026-12-13 仍未解冻：
  - [ ] 7.2.1 归档 `turn-id-mvp` 到 `openspec/changes/archive/turn-id-mvp-expired/`
  - [ ] 7.2.2 归档本 change 到 `openspec/changes/archive/turn-id-mvp-thaw-eval-2026-06-13-expired/`
  - [ ] 7.2.3 `turn-id-label` capability 标注 "deferred indefinitely"
  - [ ] 7.2.4 通知用户归档决定
