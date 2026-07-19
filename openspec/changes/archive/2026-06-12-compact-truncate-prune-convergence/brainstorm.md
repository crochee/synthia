<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: Compact / Truncate / Prune 收敛 + agent_tools.rs 拆分

> 这是 superpowers:brainstorming 的原始输出，保留为决策追溯证据。
> 关联设计：`design.md`，关联实现：`proposal.md` + `specs/*.md` + `tasks.md`。
> 上游输入：opencode/codex 多专家对抗性差距评估（2026-06-12）

---

## 背景

Synthia 是 Rust workspace 实现的 AI Agent 框架，已完成 22+ openspec change。本 change 来自 **2026-06-12 差距评估** 的专家对抗性结论：
- 4 个并行研究子智能体分别调研 codex tools/、codex session/turn、opencode v2 session、opencode compaction/truncate
- 5 位虚拟专家（性能派/架构派/互操作派/实用派/稳定性派）对抗性评审后共识：**优先做 C4 + C1.3 合并提案**

**用户先验选择**（澄清后）：
- 工作粒度：**完整 OpenSpec 流程**（brainstorm → design → proposal → specs → tasks → plan）
- 收敛范围：**C4 压缩/截断/Prune 收敛 + C1.3 拆 agent_tools.rs**（合并提案）
- PRUNE_PROTECT 默认预算：**40K**（与 OpenCode 一致）
- `<previous-summary>` 锚定 trigger：**仅 L1 错**（精确语义）
- bash UTF-8 panic 修复策略：**修补·保持现状**（最小变更）

---

## 决策链

### Q1: Synthia 当前截断/压缩路径的"重复"程度？

**结论**：**4 套 truncate 路径 + 3 套 compaction 路径并存**。

证据（来自 4 个研究子智能体的交叉验证）：

**truncate 路径（4 套并存）**：
- `synthia-context::truncate::truncate_output`（[truncate.rs:93-152](file:///home/crochee/workspace/synthia/crates/synthia-context/src/truncate.rs#L93-L152)）— 唯一规范化入口，head/tail + 磁盘 spill + UTF-8 边界
- `synthia-agent::tool_executor::truncate_result`（[truncate.rs:44-82](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tool_executor/truncate.rs#L44-82)）— 遗留路径，已声明 deprecation
- `synthia-exec::bash_tool::execute_command`（[bash_tool.rs:120-150](file:///home/crochee/workspace/synthia/crates/synthia-exec/src/bash_tool.rs#L120-150)）— **直接 `String::truncate()` 触发 UTF-8 panic**
- `synthia-exec::command_blacklist::truncate_output`（[command_blacklist.rs:178-187](file:///home/crochee/workspace/synthia/crates/synthia-exec/src/command_blacklist.rs#L178-187)）— UTF-8 安全

**compaction 路径（3 套并存）**：
- `synthia-context::compaction_service::compact_messages`（[compaction_service.rs:14-67](file:///home/crochee/workspace/synthia/crates/synthia-context/src/compaction_service.rs#L14-67)）— 0.3 阈值入口
- `synthia-context::compaction::compactor::apply_compaction`（[compactor.rs:790-877](file:///home/crochee/workspace/synthia/crates/synthia-context/src/compaction/compactor.rs#L790-877)）— L1→L2→L3 3 次 `estimate_tokens` 重复扫描
- `synthia-context::pruning::{soft_trim, hard_clear, micro_compact}`（[pruning.rs:207-326](file:///home/crochee/workspace/synthia/crates/synthia-context/src/pruning.rs#L207-326)）— 3 种降级路径

**关键差距（OpenCode 实现，Synthia 缺）**：
- `time.compacted` 幂等标记 — 物理保留 part，仅写时间戳；渲染层识别后输出 `[Old tool result content cleared]`
- `<previous-summary>` 锚定 — 多次 compaction 累积决策不蒸发
- PRUNE_PROTECT 预算（40K token tail 保护）
- 单遍贪心 select() 算法（O(n) 而非 3×O(n)）

### Q2: 哪些是 P0 生产风险？

**结论**：**bash_tool.rs:125, 136 的 UTF-8 panic 是 P0**。

证据：
- 第 125/136 行：`s.truncate(self.max_output_length)` 直接调 `String::truncate(usize)`
- 当 shell 输出以多字节 UTF-8 结尾（中文/Emoji），且 `max_output_length` 刚好落在多字节字符中间 → **panic: `byte index N is not a char boundary`**
- `max_output_length` 是构造时一次性设置（默认 30 000），实际输出字节长度任意；不存在边界检查
- **修复成本**：1 行 —— 改为 `cap_to_char_boundary(&mut s, self.max_output_length)` 或调 `synthia_context::truncate::truncate_output`

**用户决策**（Q3 答复）：**修补·保持现状**（最小变更，不动 max_output_length 语义，不重构 4 套 truncate）。

### Q3: PRUNE_PROTECT 预算设多少？

**结论**：**40K**（与 OpenCode 一致）。

证据对比：

| 方案 | 优 | 劣 |
|------|----|----|
| 40K（OpenCode） | 5-8 个中型 tool 调用被保护；200K model 损耗 20% | 128K model 损耗 31% |
| 60K | 多次 compaction 损失更少 | 128K model 损耗 47% |
| 动态按 model | 最优 | 实现复杂 |

**用户决策**（Q1 答复）：**40K**——优先与上游对齐，便于未来追平 OpenCode 的 `PRUNE_PROTECT` 行为。

### Q4: `<previous-summary>` 锚定 trigger？

**结论**：**仅 L1 错**（精确语义）。

证据：
- L1 = LLM summary（高质量）；L2 = 结构化截断（中等）；L3 = marker-only（保底）
- 仅 L1 携带 `previous_summary` 字段 → LLM 在生成新 summary 时引用 `<previous-summary>` 标签
- L2/L3 退化路径不携带 → 不污染中等/低质量 summary 路径

**用户决策**（Q2 答复）：**仅 L1 错**——精确语义，容错由 L1 自身成功概率承担。

### Q5: agent_tools.rs 拆分的粒度？

**结论**：**拆为 7 个子文件**（按职责切分）。

证据：
- 当前 [agent_tools.rs:1-46856 字节](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/agent_tools.rs) = 1300+ 行单文件
- 8 个职责混杂：`MessageBus` / `AgentInstance` / `SubagentManager` / `TeamCreateTool` / `AgentTool` / `SendMessageTool` / `ToolDefinition` / `register_builtin_tools`
- Codex 参照 [codex-rs/tools/src/](file:///home/crochee/workspace/codex/codex-rs/tools/src/)：23+ 文件每个 spec 独立

**提议拆分**（[refactor from R3 报告](file:///home/crochee/workspace/synthia/.worktrees/...)）：
- `tools/agent/message_bus.rs` — AgentMessage / MessageBus / SendError
- `tools/agent/instance.rs` — AgentInstance / lifecycle
- `tools/agent/manager.rs` — SubagentManager / spawn coordination
- `tools/agent/tools/spawn.rs` — AgentTool (create_spawn_agent_tool)
- `tools/agent/tools/send.rs` — SendMessageTool
- `tools/agent/tools/team_create.rs` — TeamCreateTool
- `tools/agent/tools/team_delete.rs` — TeamDeleteTool
- `tools/agent/mod.rs` — 重新导出 + register_builtin_tools

**风险**：Low（纯文件拆分，公开 API `agent_tools::*` 通过 `mod.rs` 重导出保持不变）。

### Q6: 多专家对抗性评审的共识是什么？

| 专家 | 立场 | 与最终选择的关系 |
|------|------|------------------|
| 🔴 性能派 (A) | P0 = bash UTF-8 panic；`time.compacted` 改善 KV cache 命中率 | ✅ 完全采纳 |
| 🏗️ 架构派 (B) | C2 turn 抽象是结构性 gap，但 M-M-L 工作量 | ⏸️ 6 个月后再评估 |
| 🌐 互操作派 (C) | C3 ACP 是单点高收益 | ⏸️ 6 个月后再评估 |
| 🛠️ 实用派 (D) | C4 + C1.3 合并，P0 + 清理 | ✅ 完全采纳 |
| 📊 稳定性派 (E) | 4 周 4 变更后需要"纯修复+清理" | ✅ 完全采纳 |

**共识**：**C4 + C1.3 是当前最值得做的专项**。C2 / C3 暂不启动。

### Q7: 实施顺序？

**结论**（用户已选）：**5 阶段 TDD 顺序**（与 error-recovery-cascade 保持一致风格）。

| 阶段 | 任务 | 工作量 | 风险 |
|------|------|--------|------|
| **P0** | bash_tool UTF-8 panic 修复 + regression test | S | None |
| **P0** | 删 `bash_tool::truncate` 私有逻辑（统一语义） | S | Low |
| **P1** | `Message` 加 `tool_result_cleared_at: Option<Instant>` + 渲染层识别 | M | Low |
| **P1** | 引入 `prune()` 单遍扫描 + PRUNE_PROTECT=40K + 幂等标记 | M | Med |
| **P2** | `apply_compaction` 改单次 estimate + 贪心选 level | M | Low |
| **P2** | `recovery_cascade::try_l4_compact` 共享 `original_tokens` | S | None |
| **P2** | `compact_level1` 接受 `previous_summary: Option<String>` + `<previous-summary>` 锚定 | M | Low |
| **P3** | 拆 `agent_tools.rs` → 7 子文件 + 公开 API 保持 | M | Low |

总工作量：S+S+M+M+M+S+M+M = **约 3-4 天**（单人或 1-2 人 subagent 并行）

### Q8: 工作粒度？

**结论**：**完整 OpenSpec 流程**（用户先验选择）：
- 写 `brainstorm.md`（本文档）
- 写 `design.md`（结构化架构）
- 写 `proposal.md`（动机/能力/影响）
- 写 `specs/*/spec.md`（4-5 个 spec）
- 写 `tasks.md`（5 阶段任务分解）
- 写 `plan.md`（实施计划 + TDD micro-tasks）
- 实施时：`git worktree` 隔离 → 5 阶段 subagent-driven-development → verify → retrospective → archive

---

## 设计 trade-offs

### T1: bash UTF-8 修复 = in-place 还是替换为统一 truncate？

| 方案 | 优 | 劣 |
|------|----|----|
| in-place（cap_to_char_boundary 包装） | 1 行改动，零行为变化 | 仍是 4 套 truncate 之一 |
| 替换为 synthia_context::truncate::truncate_output | 消灭 1 套 | max_output_length 语义变（30K → 16K head/tail split） |
| **采用** | **in-place 修补** | **用户决策：保持现状** |

### T2: PRUNE_PROTECT 字段位置？

| 方案 | 优 | 劣 |
|------|----|----|
| `synthia_provider::Message` | 离 LLM 最近，渲染层直接访问 | 改 provider 跨 crate |
| `synthia_session::Message` | 业务语义明确 | 渲染层需走 cross-crate 访问 |
| `synthia_context::Message`（已存在） | 零成本访问 | 与 session message 类型不一致 |
| **采用** | **`synthia_context::Message`** | **与现有 trait 解耦，最小变更** |

### T3: `<previous-summary>` 注入位置？

| 方案 | 优 | 劣 |
|------|----|----|
| `compact_level1` 接受 `previous_summary: Option<String>` | 显式契约 | 多 1 个参数 |
| 通过 `CompactionProvider` 隐式存储 | 不污染 L1 签名 | state 散落 |
| **采用** | **L1 函数签名显式参数** | **可读性 + 可测性** |

### T4: agent_tools.rs 拆分顺序？

| 方案 | 优 | 劣 |
|------|----|----|
| 一次性 7 文件 | 工作集中 | diff 大，merge 冲突风险 |
| 渐进式（先 MessageBus，再 ToolDefinition，再各 Tool） | diff 小，可逐步 review | 5 个 PR，节奏拖慢 |
| **采用** | **一次性 7 文件** | **本 change 内集中完成，零行为变化** |

---

## Open Questions

- OQ1: `prune()` 函数放在 `synthia-context::pruning` 还是新建 `synthia-context::prune`？
  - 倾向：`pruning` 子模块（已有，软扩张）
- OQ2: `Message::tool_result_cleared_at` 字段是否同时影响 `ToolOutput` 类型？
  - 倾向：仅影响 `Message` 渲染层（`ToolOutput` 保持不变）
- OQ3: `apply_compaction` 改单次 estimate 后，`CompactionResult` 的 `original_tokens` 字段语义是否变化？
  - 倾向：不变化（仍然是 1 次 estimate 的值，只是调用次数从 3 减到 1）

→ 在 `proposal.md §8` 中给出倾向答案，spec 实现时若推翻需更新 design。

---

## 不在本次范围的项（明确拒绝）

| 项 | 拒绝理由 |
|---|---|
| 4 套 truncate 全量收敛到 synthia_context::truncate | 用户决策"修补·保持现状"；4 套 → 1 套是更大的 refactor，留待后续 |
| 3 套 compaction 入口合并 | 本 change 仅修 `apply_compaction` 内部单遍化；入口策略保持分散 |
| `<previous-summary>` 注入全 level | 用户决策"仅 L1" |
| agent_tools.rs 重写为 codex 风格 (modular tool spec/handler) | 本 change 仅做文件拆分，行为不变；spec/handler 分离是 R1 大重构，留待后续 |
| C2 turn 抽象 / C3 ACP | 6 个月后再评估（专家共识：当前不是阻塞 gap） |

---

## 验证策略

- 每个 spec 配 ≥ 6 个 unit tests（行为锁定）
- 每个 spec 配 ≥ 1 个 integration test（端到端）
- bash UTF-8 panic 修复配 1 个 regression test（输入多字节 UTF-8 + 切点在中段）
- `agent_tools.rs` 拆分后**所有现有测试必须通过**（零行为变化验证）
- `cargo clippy --all-targets --all-features --tests --all` 无 warning
- 公开 API 完全向后兼容（`agent_tools::*` 路径不变）
- 每个阶段独立 commit，可单独 revert
- 5 个能力独立 commit，可单独 revert

---

## 与项目约定的一致性

✅ **P1 (前缀稳定)**：通过 `time.compacted` 幂等标记实现（与 rules.md 一致）
✅ **P6 (不丢信息)**：物理保留 part，仅追加时间戳（呼应 P8）
✅ **P8 (Transform Never Lose)**：时间戳字段 `serde(default)`，旧消息零影响
✅ **失败模式 fail-closed**：bash UTF-8 panic 改为 graceful truncation
✅ **Zero Behavior Change**：`agent_tools.rs` 拆分纯文件级
✅ **工程约定（project_memory）**：先修 critical bug + 删重复代码，6 个月后再考虑 trait 抽象
✅ **Rust 编码规范**：每个阶段后跑 `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all`
