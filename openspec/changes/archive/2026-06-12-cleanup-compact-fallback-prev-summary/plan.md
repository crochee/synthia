# Plan: cleanup-compact-fallback-prev-summary

> 实施步骤：FU.2 + FU.5 收尾（来自父 change retrospective）
> 目标：L4 路径 1×O(n) 替代 2×O(n) + `previous_summary` 4K 字符 cap

---

## 1. 实施顺序

```
Stage 1 (FU.2 核心): 改 compact_level1 签名 + compact_with_fallback 签名
  ↓
Stage 2 (FU.2 透传): apply_compaction 传 Some(n) + try_l4_compact 传 Some(n) + 更新 5 个旧 test 调用
  ↓
Stage 3 (FU.2 测试): 3 个新单测 + 1 个集成
  ↓
Stage 4 (FU.5 helper): truncate_previous_summary 函数 + 4 个单测
  ↓
Stage 5 (FU.5 集成): 3 处 wrap 前调用 helper + 2 个集成
  ↓
Stage 6 (验证): fmt + clippy + cargo test + openspec validate
```

Stage 1-3 是 FU.2，Stage 4-5 是 FU.5。FU.2 先做（无依赖，签名变更是 breaking）；FU.5 后做（基于既有 signature，纯增量）。

---

## 2. Stage 1: FU.2 签名扩展 (M)

### 1.1 `compact_level1` 签名
```rust
// crates/synthia-context/src/compaction/compactor.rs
pub async fn compact_level1(
    messages: &[Message],
    provider: &dyn CompactionProvider,
    previous_summary: Option<&str>,
    precomputed_original_tokens: Option<usize>,  // NEW
) -> Result<CompactionPart, ContextError>
```

### 1.2 实现
```rust
let original_tokens = match precomputed_original_tokens {
    Some(n) => n,
    None => estimate_tokens(messages),
};
```

### 1.3 `compact_with_fallback` 签名
```rust
pub async fn compact_with_fallback(
    messages: &[Message],
    token_budget: usize,
    provider: Option<&dyn CompactionProvider>,
    previous_summary: Option<&str>,
    precomputed_original_tokens: Option<usize>,  // NEW
) -> Vec<Message>
```

实现：
```rust
if let Some(p) = provider {
    let result = compact_level1(messages, p, previous_summary, precomputed_original_tokens).await;
    ...
}
```

---

## 3. Stage 2: FU.2 透传 (S)

### 2.1 `apply_compaction` line 877
```rust
// 从: compact_level1(msgs_to_compact, p, previous_summary).await
// 到:  compact_level1(msgs_to_compact, p, previous_summary, Some(original_tokens)).await
```
`original_tokens` 已在 line 872 compute。

### 2.2 `try_l4_compact` line 207
```rust
// 从: compact_with_fallback(&ctx.messages, target, provider, previous_summary)
// 到:  compact_with_fallback(&ctx.messages, target, provider, previous_summary, Some(original_tokens))
```
`original_tokens` 已在 line 205-206 compute。

### 2.3 5 个 test 更新
- `test_compact_level1_empty_messages` line 1543
- `test_compact_level1_success` line 1558
- `test_compact_level1_provider_empty_fallback` line 1571
- `test_compact_level1_provider_failure_fallback` line 1583
- `test_compact_with_fallback_*` (5 个) line 1743+

每个都加 `None` 作为最后参数：
```rust
let result = compact_level1(&messages, &provider, None, None).await.unwrap();
let result = compact_with_fallback(&messages, 1000, Some(&provider), None, None).await;
```

---

## 4. Stage 3: FU.2 测试 (S)

### 3.1 `compact_level1_uses_precomputed_tokens_when_supplied`
```rust
#[tokio::test]
async fn test_compact_level1_uses_precomputed_tokens_when_supplied() {
    let provider = MockSuccessProvider { summary: "ok".to_string() };
    let messages = vec![Message::user("x")];
    let result = compact_level1(&messages, &provider, None, Some(42_000)).await.unwrap();
    assert_eq!(result.original_tokens, 42_000, "precomputed value must override estimate");
}
```

### 3.2 `compact_with_fallback_propagates_precomputed_tokens`
```rust
#[tokio::test]
async fn test_compact_with_fallback_propagates_precomputed_tokens() {
    // MockSuccessProvider 配合 thread-local record 验证 L1 收到 Some(9999)
    // OR: 用 CapturingProvider 在 provider 内抓取 precomputed via result.original_tokens
    let provider = CapturingProvider { ... };
    let messages = vec![Message::user("x"), Message::assistant("y")];
    let _ = compact_with_fallback(&messages, 1000, Some(&provider), None, Some(9999)).await;
    // 验证 L1 返回的 part.original_tokens == 9999
    let captured = provider.last_original_tokens.lock().unwrap();
    assert_eq!(*captured, Some(9999));
}
```

需要 `CapturingProvider` 扩展一个 `last_original_tokens: Mutex<Option<usize>>` 字段（已有 `last_previous`）。

### 3.3 `try_l4_compact_avoids_duplicate_estimate` (integration, in recovery_cascade.rs)
```rust
#[tokio::test]
async fn test_try_l4_compact_avoids_duplicate_estimate() {
    // 构造大 messages (n=100) → L4 触发
    // 用 CapturingProvider 验证 L1 收到的 precomputed_original_tokens 是 try_l4_compact
    //   在 line 205 算的 original_tokens（而非 estimate 重算）
    // 验证: provider.last_original_tokens == Some(try_l4_compact 算的 n)
}
```

---

## 5. Stage 4: FU.5 helper + 测试 (M)

### 4.1 `truncate_previous_summary` 函数
位置：`crates/synthia-context/src/compaction/compactor.rs` (module-private, near `build_structured_summary_fallback`)

```rust
/// Cap a `previous_summary` to `max_chars` total characters, keeping the
/// head (60%, most recent decisions) and tail (40%, oldest decisions) of
/// the original string with a marker between them indicating how much
/// was dropped. The marker is included in the output budget.
///
/// UTF-8 safe: all slice boundaries are floored to the nearest
/// `is_char_boundary` (mirrors the P0 bash UTF-8 panic fix).
const PREVIOUS_SUMMARY_MAX_CHARS: usize = 4000;

fn truncate_previous_summary(prev: &str, max_chars: usize) -> String {
    if prev.len() <= max_chars {
        return prev.to_string();
    }
    // Reserve ~80 chars for the marker line (well above actual marker
    // length even for huge `prev`).
    const MARKER_OVERHEAD: usize = 80;
    let usable = max_chars.saturating_sub(MARKER_OVERHEAD);
    let head_budget = usable * 6 / 10;
    let tail_budget = usable - head_budget;
    let dropped = prev.len().saturating_sub(head_budget + tail_budget);

    let head_end = floor_to_char_boundary(prev, head_budget);
    let tail_start = floor_to_char_boundary_from_end(prev, tail_budget);
    format!(
        "{}\n[... {dropped} chars truncated ...]\n{}",
        &prev[..head_end],
        &prev[tail_start..]
    )
}

fn floor_to_char_boundary(s: &str, mut at: usize) -> usize {
    if at >= s.len() { return s.len(); }
    while !s.is_char_boundary(at) { at -= 1; }
    at
}

fn floor_to_char_boundary_from_end(s: &str, tail_budget: usize) -> usize {
    let start = s.len().saturating_sub(tail_budget);
    floor_to_char_boundary(s, start)
}
```

### 4.2 4 个新单测
- `truncate_previous_summary_passes_through_short_input`
- `truncate_previous_summary_truncates_long_input_with_marker`
- `truncate_previous_summary_preserves_head_and_tail`
- `truncate_previous_summary_handles_multibyte_utf8`

---

## 6. Stage 5: FU.5 集成 (S)

### 5.1 `Compactor::build_structured_summary` line 264
```rust
// 在 match previous_summary 之前：
let previous_summary = previous_summary.map(|p| truncate_previous_summary(p, PREVIOUS_SUMMARY_MAX_CHARS));
```

### 5.2 `build_structured_summary_fallback` line 1059
同上。

### 5.3 `Compactor::level1_summary_with_provider` line 234
```rust
// 在 provider.generate_summary 之前：
let prev_for_llm = previous_summary.map(|p| truncate_previous_summary(p, PREVIOUS_SUMMARY_MAX_CHARS));
match p.generate_summary(messages, prev_for_llm.as_deref()).await {
    ...
}
```

### 5.4 2 个集成测试
- `build_structured_summary_truncates_previous_summary` (assert anchor block 内容 ≤ 4000 chars + marker 出现)
- `build_structured_summary_fallback_truncates_previous_summary` (同上)
- `compact_level1_threads_truncated_previous_summary_to_provider` (用 CapturingProvider 抓取 actual arg，验证是 truncated 版)

---

## 7. Stage 6: 验证 (S)

```bash
cargo +nightly fmt --all
cargo clippy --all-targets --all-features --tests --all  # 0 new warning in compactor.rs
cargo test -p synthia-context --all-features
cargo test -p synthia-exec  # 回归
cargo test -p synthia-agent --lib  # 回归
openspec validate cleanup-compact-fallback-prev-summary --strict
```

---

## 8. 风险登记

| ID | 风险 | 等级 | 触发条件 | 缓解 |
|----|------|------|----------|------|
| R1 | 签名 breaking 影响 2 调用方 + 5 test | 低 | 编译失败 | 已列 7 处明确更新点 |
| R2 | 截断比例 60/40 在某些场景丢失关键决策 | 低 | 用户反馈 | 4K cap 是经验值，下个 change 可调 |
| R3 | UTF-8 边界 case 漏掉 | 低 | 罕见多字节 | multi-byte test 显式覆盖 |
| R4 | `CapturingProvider` 扩展字段破坏既有 test | 低 | 字段未加 `Default` | 既有 4 个 CapturingProvider test 用字段初始化语法，加新字段后自动报错 |

---

## 9. Out-of-scope 确认

不动以下（与本 change 无关，列出来避免误改）：
- `Compactor::compact` (struct 方法)
- `Compactor::compact_with_provider` (struct 方法)
- L2 / L3 的 estimate 次数
- `compaction_service::compact_messages` 入口
- L1 prompt 模板
- `CompactionProvider::generate_summary` 签名
- 既有 6 个 cleared-placeholder test（与本 change 无关）

---

## 10. 完成定义 (DoD)

- [x] Stage 1-3 全部 commit，FU.2 完整实现
- [x] Stage 4-5 全部 commit，FU.5 完整实现
- [x] 既有 0 个 test regression
- [x] 7 个新 test 全部 pass
- [x] `openspec validate` pass
- [x] 2 个 delta spec 各 +1 ADDED requirement
- [x] Retrospective 写入
- [x] Working tree clean
