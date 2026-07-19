# Tasks: Fix 12 Synced Spec Headers + Add Format Drift CI Gate

## 1. Pattern A 修复（5 specs，1 行 sed）

- [x] 1.1 `cache-control-mark/spec.md`: rename `## ADDED Requirements` → `## Requirements`
- [x] 1.2 `command-blacklist/spec.md`: 同上
- [x] 1.3 `loop-detector-algorithm/spec.md`: 同上
- [x] 1.4 `permission-fail-closed/spec.md`: 同上
- [x] 1.5 `synthia-session-reexport-policy/spec.md`: 同上

## 2. Pattern B 修复（7 specs，补 ## Purpose + rename）

- [x] 2.1 `context-management/spec.md`: 补 `## Purpose` section + rename
- [x] 2.2 `cron-system/spec.md`: 同上
- [x] 2.3 `error-recovery/spec.md`: 同上
- [x] 2.4 `memory-system/spec.md`: 同上
- [x] 2.5 `observability/spec.md`: 同上
- [x] 2.6 `recovery-cascade-wiring/spec.md`: 同上
- [x] 2.7 `tool-execution/spec.md`: 同上

## 3. CI Gate 脚本

- [x] 3.1 创建 `scripts/check_synced_spec_format.sh`
- [x] 3.2 脚本逻辑：遍历 `openspec/specs/*/spec.md`，grep `^## (ADDED|MODIFIED) Requirements`
- [x] 3.3 fail semantics：含 1 个以上 fail exit 1，0 drift exit 0
- [x] 3.4 脚本自验证：合成 drift file 测试 FAIL 路径 + clean 状态测试 PASS 路径
- [x] 3.5 文档化：脚本顶部注释说明用途 + 调用方式

## 4. 验证

- [x] 4.1 跑 `openspec spec validate <name> --strict` 12 次 → **12/12 PASS**
- [x] 4.2 跑 `bash scripts/check_synced_spec_format.sh` → **exit 0 (61 specs, 0 drift)**
- [x] 4.3 跑 `openspec validate 2026-06-14-fix-12-synced-spec-headers --type change --strict` → **PASS**
- [x] 4.4 跑 `openspec spec validate --strict` 全部 61 specs → **无新增 failure**

## 5. OpenSpec 收尾

- [x] 5.1 创建 `verify.md` 记录 12/12 pass + CI script pass
- [x] 5.2 创建 `retrospective.md` 记录过程
- [x] 5.3 创建 `brainstorm.md` 记录 4 派审查
- [x] 5.4 `openspec archive` (worktree-local since `openspec/` is gitignored per project memory)
- [x] 5.5 8 OpenSpec artifacts 全部就位: proposal.md, design.md, tasks.md, plan.md, brainstorm.md, verify.md, retrospective.md, specs/fix-12-synced-spec-headers/spec.md
