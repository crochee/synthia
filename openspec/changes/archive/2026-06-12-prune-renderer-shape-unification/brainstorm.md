<!--
Raw capture of brainstorming for prune-renderer-shape-unification.

Hand-written because `superpowers:brainstorming` is not installed.
Captures the full adversarial review + decision chain from the brainstorming
conversation (BS.1 → BS.7) in the natural decision-log format.

The design.md in this change is the structured reorganization of this capture.
Do NOT copy this file's content into design.md — they are complementary.
-->

# Brainstorm: prune-renderer-shape-unification

## 背景（Context）

上一个 change `compact-truncate-prune-convergence`（2026-06-12 归档）暴露两个实现 gap，写入 retrospective：

- **FU.1** (P1): `pruning::is_tool_result` 与 `truncate_messages` cleared-placeholder 分支对工具结果消息的形状检测不一致
- **FU.6** (P2): `prune()` 还没在 stream builder 中被自动调用

新写的集成测试 `crates/synthia-context/tests/compact_truncate_pipeline.rs::pipeline_renderer_replaces_cleared_with_placeholder` 用 `Role::Tool` + `ContentPart::Text` + `tool_call_id` 形状（Shape B）手工设置 `cleared_at` 触发了 placeholder 路径，但绕过了 `is_tool_result`。真正由 `prune()` 触发的 Shape A（`Role::User` + `ContentPart::ToolResult`）从未在 renderer 测试中验证。

## 项目上下文探索

读取的文件：
- `crates/synthia-context/src/pruning.rs`：实现 `prune()` 和 `is_tool_result` 自由函数
- `crates/synthia-context/src/truncate.rs`：实现 `truncate_messages` 和 cleared-placeholder 分支
- `crates/synthia-provider/src/types.rs`：`Message`/`Content`/`ContentPart` 枚举
- `crates/synthia-agent/src/stream_builder/steps/compact.rs`：`StepCompact` check + execute
- `crates/synthia-agent/src/loop_context.rs`：`add_tool_result` 只更新 sidecar
- `crates/synthia-agent/src/utils/conversation_fix.rs` 和 `stream_builder/steps/sample.rs`：两种形状的实际生产者

## 决议链（Decision Chain）

### Q1: 工具结果消息的"形状"是什么？生产中实际使用的是哪个？

**Skeptic 关键发现**（已验证）：

- `LoopContext::add_tool_result` (`loop_context.rs:62-72`) 只更新 `recent_tool_results: Vec<(String, String, bool)>`，**不** push `Message` 到 `ctx.messages`
- `Message::tool()` 是 Shape B 唯一公开构造函数。被调用 6 次，5 次在 tests，1 次在 `utils/message.rs:91`（test helper）
- Shape A 在 test helpers 中使用（`pruning.rs:482-494`, `conversation_fix.rs:556-571`, `compact_truncate_pipeline.rs:41-54`）和 `compactor.rs:697,759` 的"re-emit after compaction"路径

**结论**：生产热路径里 tool results **不**进入 `ctx.messages`。`prune()` → `truncate_messages` cleared-placeholder 链路**当前不会被生产触发**。但**任何**将 tool results 移入 `ctx.messages` 的未来工作会立即撞上 FU.1。

### Q2: FU.6 还要做吗？

**多专家共识**：
- 实用派 (B)：在 `StepCompact::check` 顶部总是调
- 架构派：扩 `PruningConfig` 字段
- 怀疑派：**完全是投机性的**。生产循环不把 tool results 推入 `ctx.messages`，所以 `prune()` 会扫描空列表

**最终决议**（用户确认 "推迟 FU.6（推荐）"）：推迟。等生产循环真的把 tool results 推入 `ctx.messages` 时再 wire，自然成另一 OpenSpec change 的一部分。

### Q3: FU.1 走哪条路？

**三方分歧**：
- 实用派 (B)：标准化为 Shape A，把 `truncate_messages` 的 placeholder 分支改为基于 `is_tool_result` 检测
- 架构派：加 `Message::is_tool_result()` 方法 + `MessageShape` 枚举，把形状知识从 3 个 call site 收拢到一处
- 怀疑派：缺口是理论性的（Q1 已证），但既然集成测试已经触发，统一修复 ROI 仍高

**详细权衡**：

| 选项 | 改动量 | 风险 | 未来扩展 |
|------|--------|------|----------|
| (A) `is_tool_result` 加 `Role::Tool \|\| tool_call_id.is_some()` | ~3 行 | 低，但留下"renderer 用另一种形状门"的对应缺口 | 难（无统一抽象） |
| (B) 标准化为 Shape A | ~20 行 | 中（迁移两个 test helper） | 简单（所有路径用一种形状） |
| (C) 两边都识别两种形状，加助手方法 | ~50 行 + 新 API | 高（4 个形状 × 2 路径 × 2 函数 = 4 个 case） | 好（统一接口） |

**关键发现 (D2-fix)**：实用派提案 (B) 用 `is_tool_result` 替换 `extract_text().is_some()` 作 cleared-placeholder 分支的门，**会漏 Shape B**（`is_tool_result` 只识别 Shape A）。架构派 (C) 的 "Message::is_tool_result()" 助手也只是镜像自由函数，不解决形状分发。

**最终决议 (C-variant)**：在 `truncate.rs` 加私有 fn `replace_first_text_anywhere(content, new_text) -> bool`，**内部**直接处理两种形状分发：
- Shape A: 钻入 `ToolResult.content[0].text` 替换
- Shape B: 替换顶层 `ContentPart::Text.text`
- Multi: 找第一个 `Text` 或 `ToolResult` 替换
- 无匹配: 返回 false（caller 视为 no-op）

renderer 只需 `replace_first_text_anywhere(&mut msg.content, &marker)`，**不**在外部判断形状。`is_tool_result` 自由函数的语义保持纯粹（prune 用），形状分发封装在 `replace_first_text_anywhere` 内。

### Q4: 现有 `set_msg_text` 怎么办？

**最终决议 D3**：保留不动。`set_msg_text` 只对 `ContentPart::Text` 工作，用于 size-based truncation 路径。新增 `replace_first_text_anywhere` **仅**用于 cleared 路径。两条路径互不干扰。

## 设计取捨

### 取捨 R1: 抽象 ROI

`Message::is_tool_result()` 助手方法（架构派）现在只有 1 个调用点（renderer 间接通过 `replace_first_text_anywhere` 内部用）。把它做成方法 vs 自由函数的**好处**：
- 把 Shape A vs Shape B 的知识从 3 个 call site 收拢到 1 个

**坏处**：
- 1 个调用点不足以证明抽象的成本（API surface area + docs）
- 未来 2-3 个调用点时再考虑，符合用户的"先解决 bug + 重复代码，6 个月后再考虑抽象"工作流

**选择**：本地 fn `replace_first_text_anywhere` 即可。等到 ≥3 个调用点再升级为 `Message::is_tool_result()` 方法。

### 取捨 R2: 是否同时修 `is_tool_result`

自由函数 `is_tool_result` 在 `pruning.rs:43-47`，**只**检测 `ContentPart::ToolResult`。**不动**它，因为：
- 它的语义是"消息内容中是否包含 ToolResult part"——清晰
- `prune()` 用它来识别哪些消息需要标记——`Role::User + ContentPart::ToolResult` 是 prune 的目标形状
- 改 `is_tool_result` 让它也认 Shape B 会让 `prune()` 标记本不该标记的消息（Shape B 的 `ContentPart::Text` 文本不是工具结果，可能包含重要 context）

**选择**：`is_tool_result` 不动。renderer 走自己的形状分发路径。

### 取捨 R3: 测试矩阵

| 测试 | 形状 | 路径 | 必要性 |
|------|------|------|--------|
| `replace_first_text_shape_a` | Shape A | 直接 | 单元 |
| `replace_first_text_shape_b` | Shape B | 直接 | 单元 |
| `replace_first_text_multi_mixed` | Multi([Text, ToolResult]) | 直接 | 单元（边角） |
| `replace_first_text_no_match` | `Content::Single(ImageContent)` | 直接 | 单元（无 panic） |
| `truncate_messages_shape_a_cleared` | Shape A + cleared | 集成 | 验证 D2-fix |
| `truncate_messages_shape_b_cleared` | Shape B + cleared | 集成 | 保留现有测试 |
| `pipeline_prune_then_render_shape_a` | Shape A **真生产链路** | 集成 | 验证 Q1 暴露的 gap 真正修复 |

总计：4 单元 + 1 集成（新）+ 现有 1 集成（Shape B）保留 = 6 个新/保留测试。

## 已批准

用户确认："批准设计 + 立即开始实施"

## 输出文件

- `design.md` — 重新组织本档为结构化设计（Context / Goals / Decisions / Risks / Migration / 验证标准）
- `proposal.md` — 动机 + 改动 + 影响（基于 design.md 摘要）
- `specs/` — 2 个 spec delta：`prune-idempotent-marker` + `tool-output-truncate`
- `tasks.md` — 实施步骤
- `plan.md` — 风险缓解 + 验收标准
