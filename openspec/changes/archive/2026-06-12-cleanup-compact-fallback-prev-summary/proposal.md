## Why

父 change [`compact-truncate-prune-convergence`](../archive/2026-06-12-compact-truncate-prune-convergence/) 的 retrospective 暴露了两个 P2/P3 实施 gap：

1. **FU.2**: `try_l4_compact` 在 [`recovery_cascade.rs:205-206`](../archive/2026-06-12-compact-truncate-prune-convergence/design.md) 已计算 `original_tokens`，但调用的 `compact_with_fallback` → `compact_level1` 在 [`compactor.rs:618`](../archive/2026-06-12-compact-truncate-prune-convergence/design.md) 内部又对**同一份 messages** 重跑一次 `estimate_tokens`。每次 L4 触发 = 2×O(n) 扫描而非 1×。
2. **FU.5**: `previous_summary` 在 L1 锚定路径上无字符上限。每次 L1 把上一轮 summary 包进 `<previous-summary>...</previous-summary>`，连续 N 轮后 summary 长度 O(N) 线性增长，summary body 实际被 anchor block 挤压失效。

本 change 收尾这两个 P2/P3 gap（最小变更），不引入新抽象。沿用父 change 的 `synthia_context::compaction` 设计哲学。

**预期收益**：
- L4 路径 1×O(n) 替代 2×O(n)（O(n) 扫描节省 ~50%）
- `previous_summary` 累积膨胀被 4K 字符 cap 住，连续 L1 之后 summary body 仍有实际支配地位
- 公开 API 100% 向后兼容（`Option<usize>` 新增参数 + `None` 等同旧行为）

## What Changes

**FU.2: `compact_with_fallback` / `compact_level1` 接受 `Option<usize>` 预计算 token 数**
- From: `compact_level1(messages, provider, previous_summary)` 内部固定调用 `estimate_tokens(messages)` 一次
- To: `compact_level1(messages, provider, previous_summary, precomputed_original_tokens: Option<usize>)`，`Some(n)` 时复用 `n`，`None` 时回退到内部 estimate
- Reason: L4 路径已 compute，避免重复 O(n)
- Impact: **breaking**（新增最后参数），但调用方 2 个（`compact_with_fallback` / `apply_compaction`）+ 5 个 test = 7 个更新点

**FU.5: `previous_summary` 超 4K 字符截断（head 60% + tail 40% + marker）**
- From: `build_structured_summary` / `build_structured_summary_fallback` / `level1_summary_with_provider` 三处直接 wrap `previous_summary` 进 anchor block，无 cap
- To: 引入 `truncate_previous_summary(prev, max_chars)` helper，在 3 处 wrap 前调用，常量 `PREVIOUS_SUMMARY_MAX_CHARS = 4000`
- Reason: 防止连续 L1 后 anchor block 线性膨胀挤压 summary body
- Impact: non-breaking（仅行为约束，输出字符串 schema 不变，marker 文本为新元素）

## Capabilities

### Modified Capabilities

- **compaction-single-pass**: 增加 `compact_with_fallback` / `compact_level1` 接受 `Option<usize>` 预计算 token 数的能力，新增 2 个场景覆盖 L4 路径不回退
- **previous-summary-anchor**: 增加 `previous_summary` 字符上限保护（4K 字符 head/tail 截断 + marker），新增 3 个场景覆盖正常 / 超限 / 多字节 UTF-8
