<!--
Raw capture of the brainstorming session for the apply-patch-tool change.

This file is the decision log from the in-conversation brainstorming.
The skill's natural output format is preserved (背景 → 決議鏈 Q1-Qn → 設計取捨).

design.md will reorganize this content into structured sections
(Context, Goals, Decisions, Risks, Migration). Do NOT duplicate.
-->

# Apply Patch Tool — Brainstorming Decision Log

## 背景 (Background)

Synthia 28 个 crate 中 `synthia-tool` 提供 7 个内置工具：read / write / multi_edit / glob / grep / path / web。
其中文件编辑类（read / write / multi_edit）走的是 Synthia 自己的 find-and-replace 协议。
与 opencode / codex / Claude Code 对比，**缺少 Anthropic 官方 V4A `apply_patch` 工具**——
这导致 LLM 在使用 Synthia 工具时必须把"我想改这一段"翻译成"old_str + new_str"两次往返，
增加了 prompt token 消耗和出错概率（context 漂移导致 old_str 不匹配）。

**核心差距**：
- V4A `apply_patch` 是单 tool 一次往返、原子、跨文件
- Synthia `multi_edit` 是单文件、JSON 数组
- Synthia 没有"重命名"或"创建新文件"原子操作

来源：2026-06-13 多专家对抗性 gap 评估，recommend path：
"first fix Gap 3 (test_multi_turn_memory_with_tracking_provider), then AGENTS.md (Gap 1),
followed by 1-month observation before deciding between ACP and Apply Patch."
本次决策是 1 个月观察期的回访：AgentEvent 路径上已有 tool_call_id 修复（同主题），
Apply Patch 风险与 reward 已被真实使用证明明确。

## 決議鏈 (Decision Chain)

### Q1: 范围 (Scope)
- 选项 A: 只支持 `*** Update File:`（占 Claude Code 使用 ~80%）
- 选项 B: Update + Add + Delete（覆盖全量使用场景）<sup>1</sup>
- 选项 C: 全量 V4A 范本（Update + Add + Delete + Move + End of File）<sup>2</sup>

**用户决议：选项 C**（全量 V4A 范本）

理由：Anthropic 官方 V4A 规范边界 case 多（End of File 标记、MOVE 在 UPDATE 之后等），
半实现会遇到 LLM 输出规范内但实现不支持的尴尬情况，导致静默回退到普通 write，掩盖真实问题。
实现成本可控（Move 状态机只是路径重写，多约 30 行）。

### Q2: 原子性 (Atomicity)
- 选项 A: 全有或全无（Anthropic 风格，先 snapshot 再 apply）<sup>1</sup>
- 选项 B: 逐文件提交（实现简单，中途失败留半完成状态）

**用户决议：选项 A**（Anthropic 风格原子性）

理由：与 provider 协议对齐，避免"patch 3 个文件成功了 2 个，第 3 个失败导致仓库破损"的事故。
Snapshot 阶段在内存完成（multiedit 已验证过类似模式），不需要临时文件。

### Q3 (隐含): 与 multi_edit 的关系
- 选项 A: 替换 multi_edit（破坏现有 LLM 工具调用）
- 选项 B: 并存 (additive)，multi_edit 保留给简单场景 <sup>1</sup>

**默认决议：选项 B**（并存）

理由：multi_edit 在 skill 测试中已被使用，替换会引入回归。apply_patch 与 multi_edit
各擅长不同场景（multi_edit = 简单 find-replace；apply_patch = 复杂多文件事务）。

## 設計取捨 (Design Trade-offs)

### 1. V4A 解析器 vs 复用 multi_edit
- 选择：独立实现 V4A 解析器（`v4a.rs` 模块），不试图把 multi_edit 升级到 V4A
- 理由：multi_edit 的 JSON 数组格式不是 V4A 子集（V4A 用 diff 风格行首标记而非 JSON），
  强行扩展会让两者互相干扰。独立模块成本 < 200 行，换来清晰的语义边界。

### 2. 权限 / Guardian 集成
- 选择：复用 `permission/checker.rs` 的 `write` policy（apply_patch 等价于多文件 write）
- 理由：apply_patch 是 write 的超集（都是文件修改），如果 write 已被 approve，apply_patch 不应
  重复要求审批；如果 write 被 deny，apply_patch 必然也被 deny。policy 复用避免重复配置。

### 3. 错误恢复粒度
- 选择：Apply 阶段任一 hunk 不匹配 → 整体回滚（不部分提交）
- 理由：与 Q2 决议一致。LLM 拿到完整错误信息后可重新规划 patch，不必处理半完成状态。

### 4. 并发安全
- 选择：`is_concurrency_safe() -> false`（与 `WriteTool` 一致）
- 理由：mutate FS，必须串行。

### 5. 注册位置
- 选择：在 `register_defaults()` 中追加 `ApplyPatchTool`（与 multi_edit 并列）
- 理由：不是隐藏工具（`is_hidden` = false），与 multi_edit 一样对 LLM 可见。

## 范围外 (Out of Scope)

- **V4A 之外的格式**（如 unified diff、JSON Patch）：留作未来扩展，本次不实现
- **patch 压缩 / 编码优化**：V4A 是文本格式，不做二进制 patch
- **自动 fuzzy match**：hunk 不匹配立刻失败，不做 Levenshtein 距离兜底
  （避免 LLM 静默改错；与 multi_edit 一致）

## 验证标准 (Success Criteria)

5 个核心测试 case（见 tasks.md）：
1. Update 单文件单 hunk（happy path）
2. Add 新文件 + Delete 旧文件（双 op 原子）
3. Move 跨目录（`*** Move to: <newpath>`）
4. Update + hunk 不匹配 → 整体回滚，原文件不变
5. 多文件混合（Update + Add + Delete）→ 全部成功提交

加上：
- `cargo +nightly fmt` 无 diff
- `cargo clippy --all-targets --all-features --tests -p synthia-tool` 无新增 warning
- `cargo test -p synthia-tool` 全部通过

## 风险 (Risks)

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| V4A 边缘 case 实现错误 | 中 | LLM patch 失败 | 5 个核心 case + 真实 fixture 测试 |
| Move 状态机并发问题 | 低 | 文件丢失 | 串行执行 + snapshot 优先写盘顺序 |
| permission policy 误配置 | 中 | LLM 跳过审批 | 显式 `requires_permission = true` + 复用 write |
| 与 multi_edit 工具描述混淆 | 低 | LLM 选错工具 | description 字段明确写"atomic multi-file" |
