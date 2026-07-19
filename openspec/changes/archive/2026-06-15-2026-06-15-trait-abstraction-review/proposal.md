# Proposal: trait-abstraction-review

## Why

Synthia 工作区当前有 56 个 `pub trait` 声明 (2026-06-15 静态扫描: 54 个
在 `*.rs` 源文件中, 2 个在 `*.md` README 文档中, 51 个唯一 trait 名)。
距 2025-12 大规模 critical-bug + 重复代码清理已 6 个月, 但项目 memory
明确记录 "Architectural trait abstractions should be re-evaluated 6
months after bug fixes and code deduplication"。

过去 6 个月同时发生:
- 11 个死文件从 `synthia-cli/src/` 删除 (-2572 行)
- `loop_detector.rs` / `permission/policy.rs` / `sandbox.rs` 三处重复实现归一
- 19+ 个 OpenSpec change 完成
- lint 全面归零, 测试审计完成

需要一次**全量、系统、可审计**的 trait 抽象 review, 产出:
1. 57 个 trait 的统一清单 (含可量化信号)
2. 三类分流 (KEEP / REVIEW / REMOVE_CANDIDATE)
3. 10-15 个高信号 trait 的深 review (4-party 对抗)
4. 未来 refactor change 的种子索引

业界经验 (Stabilization Phase 6 月复查) 表明: 此时最容易发现
"沉默 1-impl trait" 与"过载 multi-impl trait", 是性价比最高的架构
改进窗口。

## What Changes

**核心交付物** (4 份,均在 OpenSpec change 内):

- `artifacts/trait-inventory.md` - 57 trait × 8 信号表 (自动产出)
- `artifacts/deep-reviews/{01..15}-*.md` - 10-15 篇深 review
- `artifacts/recommendations.md` - 三类分流 + 未来 refactor 索引
- `artifacts/disagreements.md` - 4-party 对抗审查分歧留痕

**不做**:
- ❌ 不实施 trait 重构/移除
- ❌ 不创建新 trait
- ❌ 不修改任何 `src/` 业务代码
- ❌ 不改公开 API

## Capabilities

### New Capabilities

- `trait-abstraction-review`: 全量 trait 抽象审视能力, 含采集 / 分类 / 深 review / 对抗审查 / 索引

### Modified Capabilities

无 (research-only change, 不修改现有 capability)

## Impact

- **代码**: 0 文件 in `src/` 改动
- **OpenSpec**:
  - 新增 `openspec/changes/2026-06-15-trait-abstraction-review/` 目录
  - 7 份文档 (proposal/design/tasks/specs/verify/brainstorm + 4 份 artifacts)
  - 1 个 0 依赖 bash 采集脚本 `openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh`
- **测试**: 0 业务测试影响; 脚本有 self-test
- **依赖**: 无新增 crate
- **风险**: 低 (纯 research, 不动 src/)
- **回滚**: 删除整个 change 目录即可
