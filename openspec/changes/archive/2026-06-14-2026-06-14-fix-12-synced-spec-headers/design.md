# Design: Fix 12 Synced Spec Headers + Add Format Drift CI Gate

## Context

- **触发问题**：`openspec spec validate --strict` 在 12 个 synced specs 上 fail
- **根因**：synced spec 路径用了 `## ADDED Requirements` (delta 格式) 而非 `## Requirements` (cumulative 格式)
- **历史 pattern**：项目 memory 已记录此分歧（"OpenSpec synced spec format divergence"），但未系统修复
- **失败 spec 列表**（openspec/specs/ 下 12 个）：

| Spec | Pattern | 当前状态 |
|------|---------|----------|
| `cache-control-mark` | A | has title + ## Purpose + ## ADDED Requirements |
| `command-blacklist` | A | 同上 |
| `loop-detector-algorithm` | A | 同上 |
| `permission-fail-closed` | A | 同上 |
| `synthia-session-reexport-policy` | A | 同上 |
| `context-management` | B | 无 title，无 ## Purpose，直接 ## ADDED Requirements |
| `cron-system` | B | 同上 |
| `error-recovery` | B | 同上 |
| `memory-system` | B | 同上 |
| `observability` | B | 同上 |
| `recovery-cascade-wiring` | B | 同上 |
| `tool-execution` | B | 同上 |

**Pattern A 修复**（5 个）：1 行 header rename
**Pattern B 修复**（7 个）：补 `## Purpose` section（从原 archived change 的 `proposal.md` "Why" 段落抽取）+ header rename

## Decisions

### D1: 修复所有 12 个，不分优先级

- **理由**：4 派共识 4-0 维持（怀疑派/架构派/生产派/简化派全票支持）
- **风险**：0（纯文本替换 + 立即 validate）
- **影响**：12/12 spec validation 通过

### D2: 仅 format fix，不重写 requirement 文本

- **理由**：12 spec 关联代码活跃（4 派审查已确认），requirement 文本未变
- **风险**：0（rename header 是 cosmetic）
- **影响**：synced spec 文本与原 change delta spec 文本在 requirement 部分完全一致

### D3: 新增 `scripts/check_synced_spec_format.sh` 作为 CI gate

- **理由**：防止 archive 时再次遗漏（arch 派 + 生产派 + 简化派 3-0 支持；怀疑派"可加"）
- **scope**：grep `openspec/specs/*/spec.md` 是否含 `## ADDED Requirements` 或 `## MODIFIED Requirements`
- **fail semantics**：含 1 个以上 fail exit 1
- **CI 集成**：pre-commit hook / CI pipeline 调用

### D4: Pattern B 补 `## Purpose` section 文本来源

- **来源**：从对应 change 的 `proposal.md` "Why" 段落抽取
- **fallback**：若 change 已 archive，从 `openspec/changes/archive/<date>-<name>/proposal.md` 取
- **风险**：purpose 文本是 change 时期的设计意图，synced spec 是稳定 requirement 文档 — 两者应该一致

### D5: 验证策略

- 修复后立即跑 `openspec spec validate <name> --strict` 12 次
- 跑新增 CI script 确认 0 漂移
- 跑 `openspec validate 2026-06-14-fix-12-synced-spec-headers --strict` 确认本 change 自身 valid

### D6: 实施顺序

1. 12 spec fixes（Pattern A 先，Pattern B 后）
2. CI script 创建 + 验证
3. 集成验证（修完 12/12 validate 通过 + CI script 0 drift）
4. OpenSpec change archive

## Risks

- **R1**：修复后某个 spec 仍 fail 其他 rule（不仅是 header）
  - 缓解：修复后立即 validate 全 12，fail 任何非预期 issue 立即停止
- **R2**：CI script false positive（grep 命中 `# ## ADDED Requirements` 注释行）
  - 缓解：grep 用 `^## ` 锚定行首；或用更严格的 `^## (ADDED|MODIFIED) Requirements$`
- **R3**：Pattern B 的 `## Purpose` 文本不准（change 时期 vs 现在）
  - 缓解：从原始 change proposal.md 抽 "Why" 段落，标注 "(recovered from archived change proposal)"

## Alternatives Considered

### 方案 A（拒绝）：仅修 12 spec，不加 CI gate
- 优势：快
- 劣势：不防复发（架构派 + 生产派反对）

### 方案 C（拒绝）：修 OpenSpec 上游工具
- 优势：根治
- 劣势：超出本项目范围（`openspec` 是外部工具）

### 方案 D（拒绝）：不修，标记 accepted tech debt
- 优势：零工作量
- 劣势：4 派全票反对（CI noise / 跨 spec 不一致 / agent 误判风险）
