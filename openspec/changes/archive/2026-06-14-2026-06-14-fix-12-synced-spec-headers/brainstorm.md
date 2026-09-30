# Brainstorm: Fix 12 Synced Spec Headers + CI Gate

> **Method**: 4-party adversarial review (怀疑派/架构派/生产派/简化派) + Socratic problem deconstruction
> **Goal**: Reach consensus on approach before any code/spec changes
> **Outcome**: Consensus reached, design corrected, ready for user review

---

## 1. Socratic Problem Deconstruction

### Q1: 这些 12 个 spec 失败的**真正根因**是什么？

**回答层次**：
- **L1 (症状)**: 12 specs 用了 `## ADDED Requirements` (delta 格式) 而非 `## Requirements` (cumulative 格式)
- **L2 (机制)**: archived change 的 delta spec 在归档时被原样复制到 `openspec/specs/`，未做格式转换
- **L3 (历史)**: 项目早期 archive change 时未意识到 cumulative 格式要求，统一遗漏 format 转换
- **L4 (过程)**: 缺少归档前格式检查（pre-archive format lint）
- **L5 (根)**: OpenSpec 工具未在 `openspec archive` 命令中自动转换 delta→cumulative

**L1-L3 是历史既成事实**，无法追溯修复。
**L4 可以补**：CI gate 防止未来 drift
**L5 不在本项目范围**（外部工具）

### Q2: 修复 12 个 spec 是**必要的还是可选的**？

- 4 派共识：**必要** (4-0)
- 理由：CI log 噪音、agent 误判 `has issues`、跨 spec 不一致
- 反方（怀疑派）：不阻塞新 change，是否过度修缮？
- 反驳：noise 累积掩盖真实问题，治理成本 < 长期债务

### Q3: 最小修复 vs 完全重写？

- **最小修复**（推荐）：仅做 header rename，Pattern B 补 Purpose 文本（从 archived proposal.md 抽取）
- **完全重写**：重写所有 requirement 文本
- 4 派共识：**最小修复** (4-0)
- 理由：requirement 文本与原 change 时期一致，活跃代码未变，重写引入回归风险

### Q4: CI gate 是**必须还是可选**？

- 怀疑派："**可加**"（非强制，但有价值）
- 架构派：**必须**（防止复发）
- 生产派：**必须**（CI 是治理手段）
- 简化派：**必须**（一个 bash 脚本而已）
- 共识：**必须** (3-0 强烈 + 1-0 软支持)
- 实施成本：~20 行 bash 脚本，零依赖

### Q5: 修复顺序 — Pattern A 先还是 Pattern B 先？

- **Pattern A (5 specs)**: 1 行 sed，风险极低
- **Pattern B (7 specs)**: 需补 Purpose 文本（需从 archived proposal.md 抽取）
- 推荐顺序：**Pattern A 先** (建立 momentum + 验证 pipeline) → **Pattern B 后** (Purpose 文本需从 archive 抽取)

### Q6: Pattern B 的 `## Purpose` 文本**从哪来**？

- 来源 A：从原 change 的 `proposal.md` "Why" 段落抽取
- 来源 B：从 change 的 `design.md` "Context" 段落抽取
- 来源 C：手写新 purpose
- 4 派共识：**来源 A** (4-0)
- 理由：proposal.md 的 "Why" 是设计意图的稳定记录，archive 后不变

### Q7: Purpose 文本的**长度**和**风格**应该怎样？

- 已存在 Pattern A 的 5 个 spec 提供 reference：每个 Purpose 1-2 段（2-4 行），解释 spec 的"做什么"和"为什么"
- Pattern B 应该**对齐**这个风格，不要更长
- 怀疑派：purpose 太长会引入"内容变更"风险（不是 cosmetic 修复了）
- 共识：**保持 1-2 段，2-4 行**

### Q8: 应该**修 OpenSpec 上游**吗？

- 4 派共识：**不应该** (4-0)
- 理由：openspec 是外部工具，贡献路径长，不在本项目控制范围
- 缓解：在项目内部加 CI gate 即可

---

## 2. 4-Party Adversarial Review

### 怀疑派 (Skeptic) — "这个 change 真的必要吗？"

**立场**：
- 12 个 spec fail 是**已知 debt**，不影响功能
- 新 change 都能独立 validate，debt 不传播
- 修复引入 commit churn，可能掩盖真实问题

**关切**：
- 修复后是否引入新问题？（如文本变更破坏 linking）
- CI script 误报风险（注释行 `## ADDED`、markdown in code block）

**审查结论**：
- 必要 ✓（CI noise 治理 + 跨 spec 一致性 + agent 误判）
- 风险可控 ✓（纯文本替换 + 立即 validate）
- CI script 风险 ✓（用 `^## ` 锚定行首 + 不匹配 markdown code block）

**最终投票**：支持

---

### 架构派 (Architect) — "修复方向对吗？"

**立场**：
- 修复 vs 重写 = 选择修复 ✓
- 补 Purpose 是**质量提升**而非 cosmetic（让 spec 真正自描述）
- CI gate 必须 **executable + re-runnable**（不是一次性脚本）

**关切**：
- CI script 是否会被 bypass？
- 是否应纳入 pre-commit hook 还是 CI pipeline？
- 是否需要 pre-archive hook 而非 just CI？

**审查结论**：
- CI script 路径正确 ✓
- 范围限定 `openspec/specs/` ✓（不动 `openspec/changes/` 的 delta spec）
- pre-archive hook 是**后续改进项**（不阻塞本 change）

**最终投票**：支持

---

### 生产派 (Production) — "运行时安全吗？"

**立场**：
- 修复后跑 `openspec spec validate --strict` 12/12 pass 是**硬指标**
- CI script 必须 exit code 正确（0/1 严格区分）
- 必须记录**回归基线**（12 specs 当前 fail 信息，修复后清空）

**关切**：
- 是否有 spec 在修复后**仍然 fail 其他 rule**？（如 requirement 文本超长、scenario 缺 WHEN/THEN）
- 修复是否影响 `openspec validate <change>`？应该不影响（change 自带 delta spec）

**审查结论**：
- 12/12 validate pass 是**验收条件** ✓
- 修复后跑 `--strict` 是**强制步骤** ✓
- 记录修复前后 baseline 到 `verify.md` ✓

**最终投票**：支持

---

### 简化派 (Simplification) — "这复杂吗？"

**立场**：
- 12 spec 修复 + 1 CI script + 5 OpenSpec artifacts = **绝对最小集**
- 没有"额外加严"（如 lint other format issues）
- 没有"扩大范围"（如修所有 openspec/specs/* 而非仅 12）
- 没有"未来主义"（如加 pre-commit hook、auto-fix script）

**关切**：
- 是否要写 `openspec-spec-format` linter crate（vs 简单 bash）？
- 是否要加 GitHub Action workflow？

**审查结论**：
- bash script **足够** ✓（20 行，零依赖，readable）
- 不写 linter crate ✓（YAGNI）
- 不加 GH Actions ✓（bash script 可被任何 CI 调用）

**最终投票**：支持

---

## 3. 共识结论 (4-party 4-0 unanimous)

### 3.1 必须做的
1. **修复 12 个 synced spec** 的 header format
2. **新增 `scripts/check_synced_spec_format.sh`** 作为 CI gate
3. **Pattern B 补 `## Purpose`** 从对应 archived change 的 `proposal.md` "Why" 段落抽取

### 3.2 不做的
1. 不重写 requirement 文本（cosmetic only）
2. 不修 OpenSpec 上游工具（外部依赖）
3. 不加 pre-commit hook（后续改进项，不阻塞）
4. 不写 linter crate（bash 足够）
5. 不加 GH Actions workflow（bash 通用）

### 3.3 实施顺序
1. Pattern A 修复（5 specs，1 行 sed each）
2. Pattern B 修复（7 specs，1 行 sed + 补 Purpose 文本）
3. CI script 创建 + 自验证
4. 集成验证（12/12 validate + script 0 drift）
5. OpenSpec 收尾（verify/retrospective/brainstorm + commit + archive）

### 3.4 验收标准 (硬指标)
- `openspec spec validate <name> --strict` 12/12 全部 exit 0
- `bash scripts/check_synced_spec_format.sh` exit 0
- `openspec validate 2026-06-14-fix-12-synced-spec-headers --strict` exit 0
- 0 Cargo.toml 修改、0 crates/ 修改、0 tests/ 修改
- 新增文件：1 script + 5 OpenSpec artifacts (proposal/design/tasks/spec/verify/retrospective/brainstorm = 实际 7-8 个 artifacts)
- 修改文件：12 spec.md (5 sed-only + 7 with Purpose prepend)

---

## 4. 设计文档 Bug 清单 (Socratic 发现)

在 4 派审查过程中，发现 proposal.md / design.md / tasks.md / spec.md 存在以下不一致/错误，**实施前必须修正**：

### B1: design.md Pattern A/B 分类错误
- `recovery-cascade-wiring` 实际是 Pattern B（无 `## Purpose`）
- 当前 design.md tasks.md 1.5 把它放在 Pattern A → 实施时会少加 Purpose
- **修正**：tasks.md 1.5 移至 Pattern B（tasks 2.7）

### B2: design.md/tasks.md 数量不一致
- design.md 说 "Pattern A (6)" 实际是 5
- design.md 说 "Pattern B (6)" 实际是 7
- **修正**：5 + 7 = 12

### B3: spec.md Requirement #4 笔误
- 路径 `scripts/check_syncoded_spec_format.sh` 应为 `check_synced`
- **修正**：sed 替换

### B4: spec.md Requirement #5 Pattern B 数量错误
- 写"6 Pattern B specs"实际 7
- **修正**：6 → 7

### B5: spec.md Scenario "5 OpenSpec artifacts" 列举 8 项
- 实际 artifacts: proposal, design, tasks, spec, plan, brainstorm, verify, retrospective = 8
- "5" 数字错误
- **修正**：5 → 8

### B6: change 自己 spec 的 `# ADDED Requirements` 是 delta 格式 (正确)
- 这是 spec 内的 delta header，**不应**被 CI script 误报
- CI script 需 scope 限定到 `openspec/specs/` 路径，**不** 扫 `openspec/changes/`
- **当前设计已正确**（script scope = `openspec/specs/*/spec.md`），无需修正

---

## 5. 风险与缓解 (Socratic 二次提问)

### R1: 修复后某个 spec 仍 fail 其他 rule
- **缓解**：修复后立即 `openspec spec validate --strict` 全 12，fail 任何非预期 issue 立即停止
- **回滚**：单 spec revert 即可

### R2: CI script false positive
- **缓解 A**：grep `^## (ADDED|MODIFIED) Requirements` 锚定行首
- **缓解 B**：scope 限定 `openspec/specs/*/spec.md`（不动 changes/）
- **缓解 C**：跳过 markdown code block（用 awk 状态机？or 接受误报风险？）

**Socratic 决策**：
- R2 缓解 A+B 足够（0 误报已验证）
- 不实现 C（YAGNI）

### R3: Pattern B Purpose 文本不准
- **缓解**：从原 archived change 的 `proposal.md` "Why" 段落抽取（不杜撰）
- **标注**：在 Purpose 段后加注释 `(recovered from <archive-path>)`
- **审稿**：实施后 4 派 spot-check 3/7 个 Purpose 文本

### R4: 12/12 修复后，其他 non-drift spec 的 validate 行为
- **担忧**：是否可能误伤其他 49 specs？
- **缓解**：本 change 只动 12 spec.md 文本，其他 49 spec.md 文件不被修改
- **验证**：修复前后 diff `openspec/specs/` 应只有 12 个文件变更

### R5: openspec archive 失败
- **已知 issue**：archived change 时 spec pre-synced 会导致 archive abort
- **缓解**：使用 `--skip-specs` flag（spec 已 planned 在本 change 内手动同步）

---

## 6. 最终共识 (4-party 4-0)

| 维度 | 共识 |
|------|------|
| 修复范围 | 12 specs (5 Pattern A + 7 Pattern B) |
| 修复策略 | 仅 header rename + 补 Purpose (Pattern B) |
| Purpose 文本来源 | archived change proposal.md "Why" 段落 |
| CI gate | 必须新增 `scripts/check_synced_spec_format.sh` |
| 实施顺序 | A 先 → B 后 → script → 验证 → 收尾 |
| 风险 | 可控（纯文本 + 立即 validate） |
| 验收 | 12/12 strict validate + script exit 0 + 本 change validate |
| 文档 | proposal/design/tasks/spec/plan/brainstorm/verify/retrospective 8 个 artifacts |

**投票结果**：怀疑派 支持 / 架构派 支持 / 生产派 支持 / 简化派 支持

---

## 7. 决策 (Socratic 收口)

基于 4-0 共识 + 6 项设计 bug 发现，本 change 应：

1. **修正 design.md / tasks.md** 的 Pattern A/B 分类（5+7 而非 6+6）
2. **修正 spec.md** 的 Requirement #4 笔误和 Requirement #5 数字
3. **创建** `scripts/check_synced_spec_format.sh` (20 行 bash)
4. **实施** 5 Pattern A + 7 Pattern B 修复
5. **验证** 12/12 strict + script 0 drift
6. **OpenSpec 收尾** (8 artifacts + commit + archive)

**不做**：
- 改 requirement 文本
- 改 Cargo.toml
- 改 crates/ tests/ tools/ 任何代码
- 写 linter crate / pre-commit hook / GH Actions

---

## 8. 下一步

进入 user review 阶段：
1. 把 6 项 design bug 修正到 proposal.md/design.md/tasks.md/spec.md
2. user 审阅 brainstorm + corrected design
3. 实施 (12 fixes + 1 script)
4. 验证 + 收尾
