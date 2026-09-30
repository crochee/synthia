# Design: prune-renderer-shape-unification

> Change: `prune-renderer-shape-unification`
> Status: design (awaiting user approval)
> Parent:  [`compact-truncate-prune-convergence` (archived)](../archive/2026-06-12-compact-truncate-prune-convergence/)

---

## 1. 背景

上一个 change (`compact-truncate-prune-convergence`) 的 retrospective 暴露了两个实现 gap，本 change 仅处理 **FU.1**。FU.6 已确认推迟（见 §7）。

### 1.1 当前 bug

| 路径 | 检测方式 | 接受的形状 |
|------|----------|-----------|
| `pruning::is_tool_result` | `Content::iter().any(ContentPart::ToolResult(_))` | **Shape A** (`Role::User` + `Content::Single(ContentPart::ToolResult(_))`) |
| `truncate_messages` cleared-placeholder 分支 | `msg.content.extract_text().is_some()` | **Shape B** (`Role::Tool` + `Content::Multi([ContentPart::Text(_)])` + `tool_call_id: Some(_)`) |

Shape A 文本在 `ToolResult.content[].text()`（不在顶层 `ContentPart::Text`）。`extract_text()` 只对顶层 `ContentPart::Text` 返回 `Some`。→ prune 标记 Shape A 消息后，renderer **不会**把它替换为 placeholder（保持原内容在 LLM 可见流中泄漏）。

**实际生产路径不会触发此 gap**（生产循环通过 `add_tool_result` 只更新 sidecar `recent_tool_results`，不把 `Message` push 到 `ctx.messages`），但**任何**将 tool results 移入 `ctx.messages` 的未来工作会立即撞上。

### 1.2 测试已暴露

`crates/synthia-context/tests/compact_truncate_pipeline.rs` 中 `pipeline_renderer_replaces_cleared_with_placeholder` 测试用 `Role::Tool` + `ContentPart::Text` + `tool_call_id` 形状（Shape B）**手工**设置 `cleared_at`，绕过了 `is_tool_result` 的检测限制。**真正**通过 `prune()` 触发的形状（Shape A）从未在 renderer 测试中验证过。

---

## 2. 目标

1. `truncate_messages` cleared-placeholder 分支在 **Shape A** 下也工作（即 `prune()` 标记的真实生产形状）
2. **不破坏** Shape B 路径（已通过的测试必须继续通过）
3. 关闭"prune 标记 → renderer 不替换"gap，**不引入新抽象**

## 3. 非目标

- 不标准化到单一形状（保留 Shape A 和 Shape B 各自的 wire format 差异）
- 不修改 `is_tool_result` 的实现
- 不动 `prune()` 函数
- 不动 stream builder（FU.6 已推迟）
- 不动 `Content` 的 enum 形态

## 4. 架构

### 4.1 数据流（修复后）

```
prune()                                    truncate_messages()
─────────                                  ─────────────────────
Shape A 标记 ok                            is_tool_result(&msg) → true
  ↓                                         ↓
tool_result_cleared_at = Some(_)           replace_first_text_anywhere()
                                              ├ Shape A: tr.content[0].text
                                              └ Shape B: ContentPart::Text.text
                                                 ↓
                                          placeholder text
```

### 4.2 关键决策

#### 决策 D1: 在 `truncate_messages` 内本地处理形状分发

不引入 `MessageShape` 枚举或 `Message::is_tool_result()` 方法（架构派建议），原因：
- 单一调用点，抽象 ROI 不足
- 形状检测逻辑已经存在于 `pruning::is_tool_result`（自由函数）
- 未来如需在多处使用再考虑 `Message::is_tool_result()` 方法

**新加的私有辅助**：

```rust
// crates/synthia-context/src/truncate.rs
/// Replace the first text-like field in `content` with `new_text`.
/// Handles both Shape A (ContentPart::ToolResult with nested text) and
/// Shape B (top-level ContentPart::Text). Returns true if a replacement
/// happened.
fn replace_first_text_anywhere(content: &mut Content, new_text: &str) -> bool;
```

实现要点：
- `Content::Single(ContentPart::Text(t))` → `t.text = new_text` ✓
- `Content::Single(ContentPart::ToolResult(tr))` → 找到 `tr.content` 第一个 `ContentPart::Text(t)`，`t.text = new_text`（若存在）
- `Content::Multi(parts)` → 找到第一个 `ContentPart::Text(t)` 或 `ContentPart::ToolResult(tr)`，按上处理
- 其他情况返回 `false`（不抛错，调用方按 no-op 处理）

#### 决策 D2: 复用 `is_tool_result` 自由函数

不重新定义形状检测，直接 `use synthia_context::pruning::is_tool_result;`。`truncate_messages` 的 cleared-placeholder 分支改为：

```rust
let cleared_at = msg.tool_result_cleared_at;
if let Some(at) = cleared_at {
    if is_tool_result(msg) {
        let marker = cleared_placeholder(at);
        replace_first_text_anywhere(&mut msg.content, &marker);
    }
    continue;
}
```

注意：原分支用 `extract_text().is_some()` 隐式地只匹配 Shape B。修复后用 `is_tool_result(msg)` 匹配 Shape A 和 Shape B（注意 `is_tool_result` 也会匹配 Shape A 的 `Role::User + ContentPart::ToolResult` 形状，但**不会**匹配 Shape B 的 `Role::Tool + ContentPart::Text` —— Shape B 不通过 `is_tool_result`）。

**等等** —— 这是一个新的 gap。让我重新看：

`is_tool_result(msg)` 用 `Content::iter().any(ContentPart::ToolResult(_))` 检测。
- Shape A (`Role::User` + `Content::Single(ContentPart::ToolResult(_))`) → **true** ✓
- Shape B (`Role::Tool` + `Content::Multi([ContentPart::Text(_)])`) → **false** ✗

所以如果用 `is_tool_result` 作为 renderer 的形状门，**Shape B 的现有测试会失败**。要么：
- (a) 把 `is_tool_result` 扩展为同时识别 Shape B（`Role::Tool && tool_call_id.is_some()`）→ 但这会改变 `prune()` 的语义（开始标记 Shape B 消息）
- (b) renderer 用更宽的形状门（`is_tool_result OR (Role::Tool && tool_call_id.is_some())`）
- (c) renderer 改成两层：先用 `is_tool_result`（Shape A path），再用 `extract_text`（Shape B path），合并

**最终决定 D2-fix**：用 (c) —— 保留 `is_tool_result` 的纯粹含义，renderer 在 cleared-placeholder 分支里**两路都试**：

```rust
let cleared_at = msg.tool_result_cleared_at;
if let Some(at) = cleared_at {
    let marker = cleared_placeholder(at);
    if replace_first_text_anywhere(&mut msg.content, &marker) {
        // Replaced in Shape A or Shape B
    }
    // If neither shape matched, no-op (don't crash, don't fall through to size-based truncation)
    continue;
}
```

**关键洞察**：`replace_first_text_anywhere` 内部已经处理两种形状，所以 renderer 只需调用它一次。它内部**直接**判断形状（不需要外部 `is_tool_result`）。这让 `is_tool_result` 的语义保持纯粹（prune 用），renderer 的形状分发封装在 `replace_first_text_anywhere` 内。

#### 决策 D3: 不修改 `set_msg_text`

现有的 `set_msg_text` 只对 `ContentPart::Text` 工作。**保留** `set_msg_text` 不动（用于非 cleared 路径的 size-based truncation），**新增** `replace_first_text_anywhere` 仅用于 cleared 路径。两条路径互不干扰。

### 4.3 改动清单

| 文件 | 改动 | 行数 |
|------|------|------|
| `crates/synthia-context/src/truncate.rs` | 加 `replace_first_text_anywhere` 私有 fn；改 cleared-placeholder 分支调它；加单测 4 条 | +60 / -8 |
| `crates/synthia-context/tests/compact_truncate_pipeline.rs` | 加 1 个新测试覆盖 Shape A + prune + render 全链路；现有 Shape B 测试保留 | +40 / 0 |
| `openspec/specs/prune-idempotent-marker/spec.md` (已存在) | 加 1 个 ADDED Requirement：renderer SHALL handle `ContentPart::ToolResult` shape | +20 / 0 |
| `openspec/specs/tool-output-truncate/spec.md` (已存在) | 加 1 个 ADDED Requirement：cleared placeholder SHALL work for both shapes | +15 / 0 |

总计：~95 LoC 加，~8 LoC 减，**零删除**。

---

## 5. 测试矩阵

| 测试 | 形状 | 路径 | 期望 |
|------|------|------|------|
| `replace_first_text_shape_a` | Shape A | 直接调 `replace_first_text_anywhere` | ToolResult 内的 text 被替换 |
| `replace_first_text_shape_b` | Shape B | 直接调 | top-level Text 被替换 |
| `replace_first_text_multi_mixed` | Multi([Text, ToolResult]) | 直接调 | 第一个 Text 被替换 |
| `replace_first_text_no_match` | `Content::Single(ImageContent)` | 直接调 | 返回 false，无 panic |
| `truncate_messages_shape_a_cleared` | Shape A + `cleared_at` | 调 `truncate_messages` | placeholder 替换成功，原 content 结构保留 |
| `truncate_messages_shape_b_cleared` | Shape B + `cleared_at` | 调 `truncate_messages` | placeholder 替换成功（原已有，保留） |
| `pipeline_prune_then_render_shape_a` | Shape A 全链路 | `prune()` → `truncate_messages` | 真生产路径：标记 + 替换都成功 |
| 已有 `pipeline_renderer_replaces_cleared_with_placeholder` | Shape B 手工 cleared | 原测试 | 继续通过 |

## 6. 风险与缓解

| 风险 | 可能性 | 影响 | 缓解 |
|------|--------|------|------|
| `replace_first_text_anywhere` 误改结构（如 `ImageContent` 边角） | 低 | 中 | 单测覆盖 "no match" 路径；返回 `bool` 让 caller 决定 |
| Shape A 的 `ToolResult.content` 是空时无法替换 | 中 | 低 | 返回 false，renderer 视为 no-op（保留原内容），与"找不到 text"语义一致 |
| `is_error: Some(true)` 路径渲染异常 | 低 | 低 | 替换的是 `tr.content[0].text`，与 `is_error` 无关 |
| 新 spec delta 引用旧 requirement 名称 | 低 | 低 | 用 `ADDED Requirements` 而非 `MODIFIED`（旧 requirement 名不在 baseline） |

## 7. FU.6 推迟说明

**为何不同时做 FU.6**：
1. 怀疑派追踪了 `add_tool_result` 的生产路径（`loop_context.rs:62-72`），确认 tool results **未** push 到 `ctx.messages`
2. 因此 `prune()` 在 `StepCompact::check` 调用会**扫描空列表**（无操作也无价值）
3. 等未来 change 把 tool results 移入 `ctx.messages` 时，再 wire `prune()` 是该 change 的自然一部分

**何时重启 FU.6**：
- 当 `LoopContext::add_tool_result` 被改造为 push `Message` 到 `ctx.messages` 时
- 当某个新 consumer 需要 prune 但不愿手动调时

---

## 8. 验收标准

- [ ] `cargo test -p synthia-context --lib truncate` 全绿
- [ ] `cargo test -p synthia-context --test compact_truncate_pipeline` 全绿（含新加的 shape A 全链路测试）
- [ ] 现有所有 prune / truncate / pipeline 集成测试零修改通过
- [ ] `cargo +nightly fmt --check` 无新 diff
- [ ] `cargo clippy -p synthia-context --all-targets --all-features --tests` 无新 warning
- [ ] 2 个 spec delta（`prune-idempotent-marker` + `tool-output-truncate`）通过 `openspec validate`

---

## 9. 后续

- **FU.6** (deferred): 等 tool results 进入 `ctx.messages` 后再 wire `prune()` 进 `StepCompact::check`
- **FU.2** (deferred): `compact_with_fallback` 接受 `Option<usize>` 预计算 token
- **FU.3** (deferred): rustfmt nightly/stable baseline 统一
- **FU.4** (deferred): `lifecycle_tools.rs` 308 → < 300 行
