# Design: cleanup-compact-fallback-prev-summary

> Change: `cleanup-compact-fallback-prev-summary`
> Parent: [`compact-truncate-prune-convergence` (archived)](../archive/2026-06-12-compact-truncate-prune-convergence/) (FU.2 + FU.5 from retrospective)

---

## 1. 背景

父 change 的 retrospective 列出了 8 个 follow-up（FU.1-FU.7 + FU.7.5），其中 FU.1 已在本会话前序 change `prune-renderer-shape-unification` 中完成。本 change 收尾 FU.2 + FU.5。

### 1.1 FU.2 当前问题

| 位置 | 代码 | 说明 |
|------|------|------|
| `recovery_cascade::try_l4_compact` line 205-206 | `let original_tokens = ctx.messages.iter().map(estimate_message_tokens).sum();` | 1×O(n) |
| → `compact_with_fallback` line 944 | 调用 `compact_level1` | — |
| → `compact_level1` line 618 | `let original_tokens = estimate_tokens(messages);` | **1×O(n) 重复** |

n = 10000 messages 时这意味着每次 L4 触发多扫 ~10000 messages × 2 次 = 20000 次。`estimate_message_tokens` 单次成本 O(message 长度)，所以总成本是 O(n × avg_msg_size) × 2。

### 1.2 FU.5 当前问题

`compact_level1` 成功路径返回的 summary 长度 = 锚定块 (`<previous-summary>...</previous-summary>` 包裹的旧 summary) + 新 summary body。

- 首次 L1：锚定块 = 空（`previous_summary = None`），输出 ≈ body
- 第二次 L1：锚定块 = 第 1 次的完整输出，body = 第 1 次 body 长度 ≈ 锚定块 × (body 比例)
- 第 N 次 L1：锚定块 ≈ 输出长度 × 0.7（body 被挤压），新 body ≈ 锚定块 × 0.3
- 第 10+ 次：锚定块主导，body 实际信息量 < 10%

无字符上限时，连续 L1 后 summary 总长 O(N) 线性增长，与"压缩消息"的本意相反。

---

## 2. 目标

1. **FU.2**: L4 路径 `compact_with_fallback` 1×O(n) 替代 2×O(n)，调用方 `try_l4_compact` 透传 `original_tokens` 给 `compact_with_fallback` → `compact_level1`
2. **FU.5**: `previous_summary` 在 wrap 进 anchor block 前截断到 ≤ 4000 字符（head 60% + tail 40% + marker），3 处 wrap 路径统一
3. 公开 API 100% 向后兼容（`Option<usize>` 新参数 + `None` 默认行为；`truncate_previous_summary` 是 module-private helper）

## 3. 非目标

- 不动 `compact_level1` 的 prompt 模板 / 业务行为（仅 token 路径优化）
- 不动 `Compactor::compact` / `Compactor::compact_with_provider`（结构体方法，已有 `original_tokens` 路径）
- 不动 L2/L3 的 estimate 次数
- 不引入 `CompactOptions` 抽象（CLAUDE.md § 2 "Don't add abstractions for single-use code"）
- 不动 LLM provider 的 `generate_summary` 接口（`previous_summary` 在调用 provider 前由 caller 截断）

---

## 4. 架构

### 4.1 FU.2 数据流（修复后）

```
try_l4_compact                           compact_with_fallback                   compact_level1
──────────────                           ────────────────────                   ───────────────
let n = estimate(ctx.messages);  ─────►  (n available)  ────────────────────►  (n available)
                                        ...                                    let original_tokens = n; // 复用
                                        if L1 fail: L2 path uses estimate(l2)  ...
```

调用方行为：
- `try_l4_compact` → `compact_with_fallback(msgs, budget, provider, prev, Some(n))`
- `apply_compaction` → `compact_level1(msgs, p, prev, Some(n))`（已 compute `n = estimate(msgs_to_compact)` at line 872）
- 其他测试 / 旧调用方 → `compact_level1(msgs, p, prev, None)`（保持原行为）

### 4.2 FU.5 截断算法

```
fn truncate_previous_summary(prev: &str, max_chars: usize) -> String {
    if prev.len() <= max_chars { return prev.to_string() }
    let head_budget = max_chars * 6 / 10;  // 60% 给头部（最新决策）
    let tail_budget = max_chars - head_budget - MARKER_LEN;  // 40% 给尾部（最旧决策）
    // 关键：所有 [..end] 必须 floor 到 is_char_boundary（呼应 P0 UTF-8 panic 修复）
    let head = floor_to_char_boundary(&prev[..head_budget]);
    let tail = floor_to_char_boundary(&prev[prev.len() - tail_budget..]);
    format!("{head}\n[... {} chars truncated ...]\n{tail}", prev.len() - max_chars)
}
```

应用点：
- `Compactor::build_structured_summary` line 264 → wrap 前截断
- `build_structured_summary_fallback` line 1059 → wrap 前截断
- `Compactor::level1_summary_with_provider` line 234 → 调 provider 前截断（provider 拿到的 `previous_summary` 已是 truncated 版）

**关键不变量**：
- 截断后字符串长度 ≤ `max_chars`（不含 marker，但 marker 自身算入下一次 estimate 的字符预算）
- 头 60% + 尾 40% 选择保留"最新决策 + 最旧决策"——中间被截断的"中段决策"在反复 compaction 后本就是 L1 最先 push 出的旧 body（呼应"持续 L1 累积"的实际场景）

---

## 5. 决策

### D1: 预计算 token 用 `Option<usize>` 透传，**不**引入 `CompactOptions` struct

- **选择**: `compact_with_fallback(messages, budget, provider, prev, precomputed: Option<usize>)` 和 `compact_level1(messages, provider, prev, precomputed: Option<usize>)`
- **理由**:
  - 5 个字段（messages, budget, provider, prev, precomputed）暂时还不构成 struct 收益阈值
  - 调用方 2 个，5 个 test = 7 个更新点
  - 父 change 的 CompactResult/CompactionPart 已经有复杂结构，函数签名扩展 1 个参数比新 struct 简单
- **已考虑 alternative**:
  - 新建 `CompactOptions` struct — ❌ 5 字段未到 struct 阈值；过设计
  - 用 thread-local 或全局 cache — ❌ 不可重入，破坏 async 多任务
  - 把 `compact_with_fallback` 整个删掉、调用方直接内联 — ❌ 调用方已 2 处，多 1 处就抽函数

### D2: 截断比例 60/40（head/tail）

- **选择**: head = 60%, tail = 40% (减去 marker 长度)
- **理由**:
  - 最新决策（head）比最旧决策（tail）信息密度高（最近一次 L1 输出的 body 通常包含当前焦点）
  - 60/40 与 OpenCode `head_lines` 配置哲学一致（最近 60% + 最旧 40%）
- **已考虑 alternative**:
  - 50/50 — ❌ 头尾同等对待，但实际"最新"信息更值钱
  - 只保留 head (截中段) — ❌ 抛弃最旧决策可能丢失 setup context
  - 随机采样 — ❌ 不可预测，难调试

### D3: 截断阈值 `PREVIOUS_SUMMARY_MAX_CHARS = 4000`

- **选择**: 常量 4000 字符
- **理由**:
  - 4000 字符 ≈ 1000 tokens (4 chars/token) ≈ 单条 LLM 响应的"低成本"预算
  - 与 anthropic prompt cache breakpoint 边界对齐（cache 块 4K 字符是常见粒度）
  - 实测 N=10 轮 L1 后，旧行为 anchor block ≈ 7K-12K 字符；4K 是压到 50% 以下的安全点
- **已考虑 alternative**:
  - 2000 — ❌ 太激进，3-4 轮就开始截断
  - 8000 — ❌ 拦截太晚，第 5-6 轮 L1 后还是溢出
  - 动态按 model context 比例 — ❌ 复杂度，无明显收益（4K 已是经验值）

### D4: marker 格式 `"[... N chars truncated ...]"`

- **选择**: `\n[... N chars truncated ...]\n` 与 `truncate::truncate_output` 的 marker 风格对齐
- **理由**:
  - 与项目内既有截断 marker 风格一致
  - N = `prev.len() - max_chars` 提示"丢失了多少"以便调试
  - 包裹在换行符中，避免与上下文字符粘接
- **已考虑 alternative**:
  - `[truncated: N chars]` — ❌ 与 truncate_output marker 风格不一致
  - JSON `{"_truncated": N}` — ❌ 破坏字符串 schema，渲染层需要特殊处理

---

## 6. 测试矩阵

### FU.2 测试 (2 个新单测 + 1 个集成)

| 测试 | 验证 |
|------|------|
| `compact_level1_uses_precomputed_tokens_when_supplied` | `compact_level1(msgs, p, None, Some(1234))` → 返回的 `original_tokens == 1234`（不是 estimate 结果） |
| `compact_with_fallback_propagates_precomputed_tokens` | `compact_with_fallback(msgs, 1000, Some(p), None, Some(9999))` → 透传到 L1 → L1 返回 `original_tokens == 9999` |
| `try_l4_compact_avoids_duplicate_estimate` (integration) | L4 触发后 `compact_with_fallback` 内部**不**重算 estimate（用 `MockProvider` 验证 call count 或 L1 返回值用 precomputed 值） |

### FU.5 测试 (5 个新单测 + 2 个集成)

| 测试 | 验证 |
|------|------|
| `previous_summary_below_limit_passes_through_unchanged` | `truncate_previous_summary("short", 4000)` → `"short"` |
| `previous_summary_above_limit_is_truncated_with_marker` | `truncate_previous_summary(&"x".repeat(8000), 4000)` → 长度 ≤ 4000 + 包含 marker |
| `previous_summary_truncation_preserves_head_and_tail` | head 60% 内容 + tail 40% 内容 + marker 都在结果中 |
| `previous_summary_truncation_handles_multibyte_utf8` | `"你好世界🌍".repeat(2000)` → 不 panic (呼应 P0 UTF-8 fix) |
| `build_structured_summary_truncates_previous_summary` | anchor block 的 `<previous-summary>` 内容 ≤ 4000 + marker 出现 |
| `build_structured_summary_fallback_truncates_previous_summary` | 同上对 fallback 路径 |
| `compact_level1_threads_truncated_previous_summary_to_provider` | 调 provider 时 `previous_summary` 已是 truncated 版（用 CapturingProvider 抓取 actual arg） |

### 回归测试（不动）

所有既有 6 个 cleared-placeholder 测试 + 5 个 compact-level1-with-provider 测试 + 5 个 compact-with-fallback 测试 + 4 个 integration pipeline 测试都应继续通过。

---

## 7. 风险 & 缓解

| 风险 | 等级 | 缓解 |
|------|------|------|
| `compact_level1` 签名 breaking 影响下游调用 | 中 | 跨 workspace 搜索 2 个调用方（已确认：apply_compaction / compact_with_fallback）+ 5 test，更新 7 处 |
| 截断后某些 case 丢失关键决策 | 低 | head 60% 保留最近 60% 决策 + tail 40% 保留最旧决策 + marker 提示数量；用户在 OQ 中可调比例 |
| 4K 阈值与 anthropic cache breakpoint 不完全对齐 | 低 | 4K 是经验值，下个 change 可按 model context 动态调整（D3 备选） |
| `truncate_previous_summary` 是 UTF-8 边界 case | 低 | 复用 `is_char_boundary` 模式（与父 change P0 修复一致），加显式 multi-byte test |

---

## 8. Out-of-scope (不碰)

- `Compactor::compact` / `Compactor::compact_with_provider`（结构体方法路径）
- L2/L3 的 estimate 次数
- `compaction_service::compact_messages` 入口
- 整个 L1 prompt 模板
- `generate_summary` provider 接口签名

---

## 9. 验证标准

- 既有 5 个 `compact_level1_*` test + 5 个 `compact_with_fallback_*` test + 6 个 cleared-placeholder test + 4 个 integration test **全部通过**
- 7 个新 test **全部通过**
- `cargo +nightly fmt --all` 0 new diff
- `cargo clippy` 0 new warning (in `compactor.rs`)
- `openspec validate cleanup-compact-fallback-prev-summary` pass
- `cargo test -p synthia-exec` + `cargo test -p synthia-agent --lib` 0 regression
