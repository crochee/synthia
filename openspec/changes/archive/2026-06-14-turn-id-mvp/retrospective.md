# turn-id-mvp Retrospective

> **Date**: 2026-06-14
> **Author**: synthia-agent executor
> **Scope**: 完整 cycle (FROZEN → THAWED → IMPLEMENTED → ARCHIVED)
> **Cycle time**: 1 day (2026-06-13 freeze → 2026-06-14 archive)

---

## 1. What Went Well

### 1.1 MVP 范围严格执行

- 简化派 MVP 设计在 4 派审查时已被识别为唯一可接受方案（`turn-id-mvp/design.md` D1）
- 实施时严格遵守：~24 行 production code, 0 个 `Turn` struct, 0 个 `TurnStatus` enum, 0 个新 `AgentEvent` variant, 0 个 persistence layer
- 验证一次通过：所有 spec scenarios 满足；0 处违规

### 1.2 配套前置任务完成

`turn-id-mvp` 依赖 3 个前置 task 全部完成：
- `unify-token-usage-types` (2026-06-12 archived)
- `turn-id-unify` (2026-06-13 archived, 中央化 `format_turn_id` + 删 `ApprovalRequest.turn_id`)
- `recovery-path-explicit` / `explicit-recovery-paths` (2026-06-13 archived)

3/3 spec+code 完成消除了实施风险。`turn_id.rs` 删 + `TurnId(Uuid)` 创建 = 1-line delete 预测 (per `turn-id-unify/retrospective.md`) 完美实现。

### 1.3 Spec 严格执行

- `wc -l turn.rs` 必须 ≤ 30 → 把 inline unit tests 移至 `tests/turn_id_test.rs` integration test
- `pub struct TurnId` 唯一位置约束保证 0 个 `Turn` struct 误创建
- `pub enum TurnStatus` 0 匹配保证 0 个 state machine 误创建

### 1.4 验证一次过

- `cargo check --workspace --all-targets` 0 errors
- `cargo test -p synthia-agent` 502 lib + 79 integration + 3 turn_id 全部 pass
- `cargo +nightly fmt --all` 无变更
- `cargo clippy` 0 新增 warning
- `openspec validate turn-id-mvp` + `openspec spec validate turn-id-label` 一次过

---

## 2. What Went Wrong / Friction

### 2.1 Turn-Id 名称冲突

- 之前 `turn-id-unify` 添加了 `turn_id.rs` (function module)
- `turn-id-mvp` 设计要求 `turn.rs` (struct module)
- 解决：`turn_id.rs` 整文件删除（`format_turn_id` 已被 `turn::TurnId` 取代）
- **教训**：design 阶段应统一命名空间；future "turn" 相关 module 都应放 `turn.rs`，避免 `turn_id` / `turn_label` / `turn_state` 分散

### 2.2 设计中文件大小约束 vs 实际 inline tests

- `turn-id-label/spec.md` "File size under 30 lines" 强制 ≤ 30
- 设计 2.5.1 要求 `turn.rs` 内有 3 个 unit tests
- inline `#[cfg(test)] mod tests {}` 会让文件超 30 行
- 解决：把 TurnId unit tests 移到 `tests/turn_id_test.rs` integration test
- **教训**：spec 写时未考虑 tests，design 2.5.1 与 spec 2.4.5 存在表面冲突。下次 OpenSpec spec 应明确"file < 30 lines includes/excludes tests"

### 2.3 User 主动解冻 vs 4 派 0-thaw 共识

- `turn-id-mvp-thaw-eval-2026-06-13` meta-change 4 派 4-0 维持冻结
- 用户直接 override："不要搞什么冻结了，给我干就完事了"
- 处理：直接实施，OpenSpec artifact 记录"用户主动解冻"事件
- **教训**：meta-change "0-thaw" 共识是 advisory，不是 binding。用户指令是 absolute。3 个月观察窗口是 default guardrail，不是 sacred rule

### 2.4 Pre-existing dead match arms

- `synthia-cli/src/output.rs:162-164` 引用不存在的 `AgentEvent::TurnStarted/TurnEnd/TurnAborted` variants
- 这些是 pre-existing dead code（5f39dee `feat: init agent` 引入），不在本 change 范围
- 解决：未触动（CLAUDE.md 最小变更原则）
- **教训**：审计 grep 命中时区分 "我引入的" vs "pre-existing"；不应 scope creep

### 2.5 OpenSpec archive abort on pre-synced specs

- 已 sync 过的 spec 会被 `openspec archive` 视为重复 ADD（apply-patch-tool 报错）
- 解决：用 `--skip-specs -y` 跳过
- 之前手动 archive (`turn-id-unfreeze`, `turn-id-unify`, `turn-id-mvp-thaw-eval-2026-06-13`) 是按项目 memory 实践；这次因为有 `--skip-specs` flag 而直接走 archive
- **教训**：`--skip-specs` 是新工具，比手动 archive 简洁；前提是 spec 已在 archive 前 synced

---

## 3. Key Insights

### 3.1 "Simplified派 MVP" 的价值再次验证

4 派审查时简化派提出 "只加 `TurnId(Uuid)` 类型 + 1 个字段 = 20 行" 方案。`turn-id-mvp` 完整实施后只新增 1 个 struct（24 行）+ 1 个字段 + 1 个 helper method。零破坏性，零 ripple。这种 YAGNI 极限的方案在 0 callers 的场景下是最优的。

### 3.2 Meta-change 的本质是 advisory

`turn-id-mvp-thaw-eval-2026-06-13` 的 4 派 0-thaw 共识维持 1 天后即被用户 override。这说明：
- meta-change 适合"记录决策依据"，不适合"否决实施"
- 真正决策权始终在用户
- meta-change 价值 = "如果不做，理由是什么" 的可追溯文档

### 3.3 Spec 验证的回归

OpenSpec `## ADDED Requirements` + synced `## Requirements` 双重 spec 体系是 robust 的：
- 任何 spec scenario 失败都能被 openspec validate 抓出
- 文件大小、行数、字段数等"硬约束"通过 `wc -l` / `grep` 等命令验证
- 这套机制在 turn-id-mvp 实施时一次过验证 11 个 spec scenarios

---

## 4. Process Improvements

### 4.1 早期文件大小约束要明确

OpenSpec spec scenario "file < 30 lines" 写时未明确 "includes/excludes tests"。建议下次：
- spec scenario 写明 "wc -l <file> excluding tests" 或 "including tests"
- 避免 design + spec 表面冲突

### 4.2 User-override 路径应该更短

用户 override 4 派 meta-change 时：
- 旧路径：4 派 meta-change 冻结 → 再次 meta-change 评估 → 用户 override → 实施
- 新路径（本次）：用户直接说"解冻" → 立即实施 → 实施后 archive 时 record "user override" 事件

新路径节省 1 个 meta-change cycle。如果 override 频繁，meta-change 流程需要重新评估。

### 4.3 Push-to-remote 决策

本地 master 领先 origin/master 45 commits，与本 change 无关。建议：
- 本 change 的 push 决策由整体 push 策略决定，不应单独 push
- tasks.md 2.6.3 标注 "未推送，独立决策" 反映此现实

---

## 5. Follow-up Items

- [ ] 监控 hook consumers 是否准备好升级 `AgentContext.turn_id: String` → `TurnId(Uuid)`
- [ ] 6-month hard cap (2026-12-13) N/A（本 change 已实施）
- [ ] 持续监控 codex Turn-related PRs（turns.jsonl 持久化 / turn 级 cost attribution）—— 升级到 full Turn model 的工业级证据
- [ ] 整体 push strategy：决定何时把 45 commits 推到 origin/master
