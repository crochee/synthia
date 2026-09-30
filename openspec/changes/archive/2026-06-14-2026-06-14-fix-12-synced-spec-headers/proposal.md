# Proposal: Fix 12 Synced Spec Headers + Add Format Drift CI Gate

## Why

`openspec spec validate --strict` 在 12 个 synced specs 上失败，全部因为同一类问题：synced spec 文件使用了 `## ADDED Requirements`（delta 格式）而不是 `## Requirements`（cumulative 格式）。这违反 OpenSpec 规范（synced spec 路径必须是 cumulative 格式）和项目 `synthia-session-reexport-policy` 类已建立的 pattern。

虽然 12 个失败 spec 不阻塞新 change（已验证：4 个最近的 change（`turn-id-unfreeze`/`turn-id-unify`/`turn-id-mvp-thaw-eval-2026-06-13`/`turn-id-mvp`）均独立通过 `openspec validate`），但：
- CI log 持续产生 noise
- 跨 spec 统一性被破坏
- 未来 agent 读取 specs 时可能因 `has issues` 误判

历史根本原因：项目早期 archive change 时未意识到 cumulative 格式要求，统一遗漏 format 转换（项目 memory 已知 issue，但未系统修复）。

> **决策方式**: 4-party 对抗性审查（怀疑派/架构派/生产派/简化派）+ 苏格拉底问题拆解,详见 [brainstorm.md](brainstorm.md)。共识: 4-0 unanimous 支持本方案。

## What Changes

- **修复 12 个 synced specs**（`openspec/specs/<name>/spec.md`）：
  - 5 个 Pattern A（`cache-control-mark`, `command-blacklist`, `loop-detector-algorithm`, `permission-fail-closed`, `synthia-session-reexport-policy`）：rename `## ADDED Requirements` → `## Requirements`
  - 7 个 Pattern B（`context-management`, `cron-system`, `error-recovery`, `memory-system`, `observability`, `recovery-cascade-wiring`, `tool-execution`）：补 `## Purpose` section（从原 archived change 的 `proposal.md` "Why" 段落抽取）+ rename header
- **新增 CI gate** `scripts/check_synced_spec_format.sh`：grep `openspec/specs/*/spec.md` 是否含 `## ADDED Requirements` / `## MODIFIED Requirements`，含则 fail exit 1

## Impact

- Affected specs: 12 synced specs 全部修复
- Affected scripts: 新增 1 个 CI script（`scripts/check_synced_spec_format.sh`）
- Affected code: 0（纯 spec text + 1 script）
- 0 breaking change，0 行为变更
- 修复后 `openspec spec validate --strict` 在 12/12 specs 上全过

## Out of Scope

- 不修改 OpenSpec 上游工具（`openspec archive`）
- 不重写 12 个 spec 的 requirement 文本（仅 format fix）
- 不重命名 12 个 spec capability name
