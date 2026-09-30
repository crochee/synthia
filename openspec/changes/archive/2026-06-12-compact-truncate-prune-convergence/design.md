# Design: Compact / Truncate / Prune Convergence + agent_tools.rs Split

> 输入：`brainstorm.md`（raw 决策日志）
> 输出：架构决策、迁移路径、未决问题
> 关联：`proposal.md`（动机/能力）、`specs/*.md`（具体行为）、`tasks.md`（执行顺序）

---

## Context

Synthia 是用 Rust workspace 实现的 AI Agent 框架，已完成 22+ openspec change。本 change 来自 **2026-06-12 差距评估** 的专家对抗性结论。

**当前状况**（详见 `brainstorm.md` §Q1）：
- 4 套 truncate 路径并存（`synthia-context::truncate`、`synthia-agent::tool_executor::truncate_result`、`synthia-exec::bash_tool::truncate`、`synthia-exec::command_blacklist::truncate_output`）
- **P0 生产风险**：`bash_tool.rs:125, 136` 的 `String::truncate(usize)` 在多字节 UTF-8 末尾触发 panic
- 3 套 compaction 入口（`compaction_service`、`compactor::apply_compaction`、`pruning::*`）且 `apply_compaction` 内部 L1→L2→L3 失败链做 **3 次完整 O(n) `estimate_tokens`**
- 缺 OpenCode 的 4 个核心机制：`time.compacted` 幂等标记、`<previous-summary>` 锚定、PRUNE_PROTECT 40K tail 保护、单遍贪心 select()
- `synthia-agent/src/tools/agent_tools.rs` 1300+ 行单文件承担 8 个职责

**用户决策**（来自 `brainstorm.md` Q1-Q3）：
- **PRUNE_PROTECT = 40K**（与 OpenCode 对齐）
- **`<previous-summary>` 锚定 = 仅 L1 错**（精确语义）
- **bash UTF-8 修复 = 修补·保持现状**（最小变更，不动 max_output_length 语义）

**目标**：在 5 个能力上落地 P0 修复 + 增量改进 — 修 UTF-8 panic、引入 `time.compacted` 幂等标记、`apply_compaction` 单遍化、加 `<previous-summary>` 锚定、拆分 `agent_tools.rs`。

---

## Goals / Non-Goals

### Goals

1. **G1**（P0）：修复 `bash_tool.rs:125, 136` 的 UTF-8 panic 风险，配 regression test
2. **G2**（P1）：在 `synthia_context::Message` 增 `tool_result_cleared_at: Option<Instant>` 字段，渲染层识别后输出占位符
3. **G3**（P1）：引入 `prune()` 单遍扫描函数，PRUNE_PROTECT=40K token tail 保护，遇到 `time.compacted` 标记立即停止
4. **G4**（P2）：`apply_compaction` 改为单次 `estimate_tokens` + 贪心选 level（消除 3×O(n) 重复扫描）
5. **G5**（P2）：`recovery_cascade::try_l4_compact` 共享一次 `original_tokens` 计算
6. **G6**（P2）：`compact_level1` 接受 `previous_summary: Option<String>` 参数，prompt 注入 `<previous-summary>` 标签
7. **G7**（P3）：`agent_tools.rs` 拆为 7 个子文件，公开 API `agent_tools::*` 通过 `mod.rs` 重导出保持不变
8. **G8**：每个能力 ≥ 6 unit tests + ≥ 1 integration test
9. **G9**：公开 API 完全向后兼容
10. **G10**：每个能力独立 commit，可单独 revert

### Non-Goals

- **不**全量收敛 4 套 truncate 到 `synthia_context::truncate`（用户决策"修补·保持现状"）
- **不**合并 3 套 compaction 入口策略（仅修 `apply_compaction` 内部）
- **不**让 `<previous-summary>` 注入全 level（用户决策"仅 L1"）
- **不**重写 `agent_tools.rs` 为 codex 风格 (modular tool spec/handler) — 仅文件拆分
- **不**碰 turn 抽象（C2 候选，6 个月后再评估）
- **不**实现 ACP / 事件总线（C3 候选，6 个月后再评估）
- **不**改 `compaction_service::compact_messages` 入口（保持 0.3 阈值 API）
- **不**改 `pruning::hard_clear` 的 placeholder 文本（保持现状）

---

## Decisions

### D1: bash UTF-8 修复 = `cap_to_char_boundary` 包装

- **选择**：在 `bash_tool.rs:125, 136` 把 `s.truncate(self.max_output_length)` 替换为 `cap_to_char_boundary(&mut s, self.max_output_length)`，新增私有函数 `fn cap_to_char_boundary(s: &mut String, max_bytes: usize)` 向后扫描到 char boundary
- **理由**：1 行核心修复 + 6 行辅助函数 + 1 个 regression test，消除生产 panic 风险
- **已考虑 alternative**：
  - 替换为 `synthia_context::truncate::truncate_output`（head/tail split）— ❌ max_output_length 语义变化（30K → 16K head/tail）
  - 删 `bash_tool::truncate` 私有逻辑全量统一 — ❌ 工作量超本 change 范围
- **为什么 in-place 修补**：
  - 保留 `max_output_length` 现有语义（仅切字节数，不分 head/tail）
  - 不动 4 套 truncate 收敛（用户决策"修补·保持现状"）
  - 改动面 < 20 行

### D2: `tool_result_cleared_at` 字段位置 = `synthia_context::Message`

- **选择**：在 `synthia_context::Message` 增 `#[serde(default)] pub tool_result_cleared_at: Option<Instant>` 字段
- **理由**：
  - `synthia_context::Message` 已是核心 message 类型，渲染层（`truncate_messages`、`step_sample`）直接访问
  - `serde(default)` 保持旧消息反序列化兼容
  - 不污染 `synthia_provider::Message`（provider 不需要这个语义）
- **已考虑 alternative**：
  - `synthia_session::Message` — ❌ 渲染层需跨 crate 访问
  - `synthia_provider::Message` — ❌ 改 provider 跨 crate，影响面大
  - 单独的 `cleared_messages: HashMap<MessageId, Instant>` side-table — ❌ 引入新概念，复杂
- **关键不变量**：
  - 字段是 `Option<Instant>`，默认 `None` 表示"未清除"
  - `Some(_)` 表示"已清除"——渲染层读到这个字段就跳过原 content 输出 placeholder
  - **物理保留原 content**：仅在渲染时跳过；事件日志/event_log 仍保留完整信息（呼应 P8）

### D3: `prune()` 单遍扫描 + PRUNE_PROTECT=40K + 幂等标记

- **选择**：在 `synthia_context::pruning` 新增 `pub fn prune(messages: &mut Vec<Message>, protect_tokens: u32) -> PruneStats` 函数
- **算法**（单遍反向扫描）：
  ```
  total_kept = 0
  for msg in messages.iter().rev():
      if msg.tool_result_cleared_at.is_some():
          break  // 幂等：遇到已 prune 过的消息立即停止
      if msg.role == Tool && total_kept + token_estimate(msg) > protect_tokens:
          msg.tool_result_cleared_at = Some(Instant::now())
      else:
          total_kept += token_estimate(msg)
  ```
- **保护值 PRUNE_PROTECT = 40 000**（token 估算）
- **理由**：
  - 单遍 O(n)，无 3×O(n) 重复
  - `tool_result_cleared_at` 字段使 `prune()` 幂等
  - 物理保留原 message content（仅追加时间戳），prefix hash 跨 prune 不变（呼应 P1）
- **已考虑 alternative**：
  - 60K — ❌ 用户决策 40K
  - 动态按 model — ❌ 复杂度，本 change 不引入
  - 物理删除 part — ❌ 违反 P8（不丢信息）

### D4: `apply_compaction` 单次 estimate + 贪心选 level

- **选择**：在 `synthia_context::compaction::compactor::apply_compaction` 重构为：
  ```rust
  pub async fn apply_compaction(...) -> Result<...> {
      let original_tokens = estimate_tokens(msgs_to_compact);  // 1 次
      // 贪心：try L1 → check → return or fall through
      // 失败时复用 original_tokens 不再 estimate
  }
  ```
- **理由**：
  - 消除 3×O(n) 重复扫描（[compactor.rs:818-876 当前实现](file:///home/crochee/workspace/synthia/crates/synthia-context/src/compaction/compactor.rs#L818-L876) 跑 3 次完整 estimate）
  - 共享 `original_tokens` 给 `recovery_cascade::try_l4_compact`（消除 L4 触发的 3 次 O(n)）
- **关键不变量**：
  - `CompactionResult.original_tokens` 字段语义不变
  - `compacted_tokens` 字段语义不变
  - L1→L2→L3 退化策略保持（仅 estimate 调用次数变化）
- **已考虑 alternative**：
  - 改入口策略（`compaction_service::compact_messages`）— ❌ 本 change 范围外
  - 改 L1/L2/L3 算法本身 — ❌ 仅优化扫描次数

### D5: `compact_level1` 接受 `previous_summary: Option<String>`

- **选择**：在 `synthia_context::compaction::compactor::compact_level1` 签名增加 `previous_summary: Option<&str>` 参数，prompt 模板检查后注入 `<previous-summary>` 块
- **理由**：
  - 显式契约比隐式 state 散落好
  - 可测性：单测可以传 `Some(...)` 或 `None` 验证两种 prompt 模板
- **prompt 模板**（仅在 L1 路径）：
  ```
  [If previous_summary.is_some()]
  "Update the anchored summary below using the conversation history above.
   Preserve still-true details, remove stale details, and merge in the new facts.

   <previous-summary>
   {previous_summary}
   </previous-summary>"
  [Else]
  "Create a new anchored summary from the conversation history above."
  ```
- **关键不变量**：
  - L2/L3 退化路径**不**接受 `previous_summary`（仅 L1 携带）
  - 调用方 `apply_compaction` 负责传递：成功时记录到 session state 作为下次 L1 的 `previous_summary`
- **已考虑 alternative**：
  - 全 level 都携带 — ❌ 用户决策"仅 L1"
  - 内联反思 — ❌ 增加 1 次额外 LLM 调用，本 change 不引入
  - 通过 `CompactionProvider` 隐式存储 — ❌ state 散落

### D6: `agent_tools.rs` 拆分 = 7 子文件 + mod.rs 重导出

- **选择**：拆为以下 7 个文件（[参考 R3 报告](file:///home/crochee/workspace/synthia/.worktrees/...)）：
  ```
  crates/synthia-agent/src/tools/agent/
  ├── mod.rs              # 重导出 + register_builtin_tools
  ├── message_bus.rs      # AgentMessage / MessageBus / SendError / ReceiveError
  ├── instance.rs         # AgentInstance / lifecycle
  ├── manager.rs          # SubagentManager / spawn coordination
  └── tools/
      ├── mod.rs
      ├── spawn.rs        # AgentTool (create_spawn_agent_tool)
      ├── send.rs         # SendMessageTool
      ├── team_create.rs  # TeamCreateTool
      └── team_delete.rs  # TeamDeleteTool
  ```
- **公开 API 保持**：
  - `agent_tools::*` 仍可用（通过 `mod.rs` 重导出 + 旧的 `agent_tools.rs` 重新指向新 mod）
  - **不删** `agent_tools.rs`，改为 shim re-export 旧路径 → 100% 向后兼容
- **理由**：
  - 纯文件拆分，零行为变化
  - 维护性：1300+ 行单文件 → 7 个 < 300 行文件
  - 风险：Low（cargo test 必须全绿证明无行为变化）
- **已考虑 alternative**：
  - 渐进式 5 PR 拆分 — ❌ 节奏拖慢，单 PR 集中做更高效
  - 重写为 codex modular spec/handler 风格 — ❌ 是 R1 大重构，留待后续
- **shim 策略**（G9 向后兼容保障）：
  ```rust
  // crates/synthia-agent/src/tools/agent_tools.rs
  pub use crate::tools::agent::*;  // 重导出全部
  ```

### D7: 实施顺序 = 5 阶段 TDD

- **选择**：5 阶段顺序实施（与 `error-recovery-cascade` 风格一致）
  1. **P0** (S+S)：bash UTF-8 fix + 删 bash_tool::truncate 私有逻辑
  2. **P1** (M+M)：Message 字段 + prune() 函数
  3. **P2** (M+S+M)：apply_compaction 单遍化 + recovery_cascade 共享 + compact_level1 锚定
  4. **P3** (M)：agent_tools.rs 拆分
  5. **Verify** (S)：集成测试 + 整体 cargo test
- **理由**：
  - P0 风险最低、最先做（修 panic）
  - P1 为 P2/P3 提供基础（time.compacted 标记）
  - P2 是性能优化，可独立验证 benchmark
  - P3 是纯文件拆分，零行为变化，最后做兜底
  - 5 阶段独立 commit，可单独 revert

### D8: 字段语义常量定义

- **选择**：在 `synthia_context::compaction` 模块定义 `pub const PRUNE_PROTECT_TOKENS: u32 = 40_000;`
- **理由**：
  - 单一来源（与 OpenCode `PRUNE_PROTECT` 对齐）
  - 配置化留待后续（避免本 change 引入 config 字段扩散）
- **未来可配置**：通过 `CompactionConfig` 注入，留待后续 change

---

## Risks / Trade-offs

### R1: bash UTF-8 修复可能改变截断边界

- **风险**：`cap_to_char_boundary` 可能在多字节字符中间"多走几步"找到 boundary，导致实际截断长度 ≤ max_output_length
- **缓解**：regression test 验证：1) 切点在多字节字符中间 → 不 panic；2) 实际截断字节数 ≤ max_output_length
- **接受理由**：trade-off 偏向"不 panic"远好于"精确字节数"

### R2: `tool_result_cleared_at` 序列化 schema 变化

- **风险**：旧消息 JSON 缺新字段，反序列化可能失败
- **缓解**：`#[serde(default)]` 默认值 `None`，旧消息 0 影响
- **验证**：单元测试反序列化"无 `tool_result_cleared_at` 字段的旧 JSON" → 成功 + 字段为 `None`

### R3: `apply_compaction` 重构可能改变退化路径行为

- **风险**：原代码 L1 失败 → 跑 L2 → check 预算 → 失败 → 跑 L3；重构后如果贪心选 level 顺序变化，可能影响某些 edge case
- **缓解**：保留 L1→L2→L3 顺序不变，仅把"每次都 estimate"改为"首次 estimate + 复用"
- **验证**：所有现有 `apply_compaction` 单测必须通过（行为锁定）

### R4: `<previous-summary>` 注入可能让 prompt 超出 LLM 上下文

- **风险**：累积多次 compaction 后 `previous_summary` 体积膨胀
- **缓解**：调用方在 `apply_compaction` L1 成功后**截断** `previous_summary` 到 4K 字符（OpenCode 同等行为）
- **本 change 不实现**：截断留作 follow-up 优化，本 change 仅注入标签

### R5: `agent_tools.rs` 拆分可能漏掉某个 use path

- **风险**：synthia-cli / synthia-server 通过 `agent_tools::AgentTool` 等路径 import，拆分后路径变化
- **缓解**：
  - **保留** `agent_tools.rs` 作为 shim，pub use 重导出全部
  - `cargo test --workspace` 必须全绿作为拆分成功的标志
  - 任何漏掉的 use path 编译会失败 → 修复 1 行 mod.rs 重导出即可

### R6: 5 阶段并行实施可能 conflict

- **风险**：多个 subagent 同时改 `synthia_context::compaction` 引起 merge 冲突
- **缓解**：tasks.md 强制 P0→P1→P2→P3→Verify 顺序，**不并发**
- **决策**：单人或 1-2 人 subagent 串行

### R7: `prune()` 函数位置选择

- **风险**：放在 `pruning` 子模块（已有）vs 新建 `prune` 模块
- **缓解**：选 `pruning`（软扩张）；OQ1 倾向答案确认
- **接受理由**：现有 `soft_trim_content` / `hard_clear_content` / `micro_compact` 都在 `pruning`，新函数 `prune()` 加入即可

### R8: 公开 API 100% 兼容约束

- **Trade-off**：通过 shim 重导出 `agent_tools::*` 多一层 indirection，编译时 0 损耗（pub use 是编译期宏）
- **接受理由**：100% 向后兼容 > 1 层 indirection 审美

---

## Migration Plan

### 阶段 1：bash UTF-8 panic 修复（约 0.5 天）

- D1：`bash_tool::execute_command` 中 `s.truncate(self.max_output_length)` → `cap_to_char_boundary(&mut s, self.max_output_length)`
- 私有函数 `fn cap_to_char_boundary(s: &mut String, max_bytes: usize)` 在 `bash_tool.rs` 文件内
- 加 regression test：`tests/bash_utf8_panic.rs` — 输入多字节 UTF-8（中文 + emoji 混合），切点强制落在中段
- 跑 `cargo test -p synthia-exec` 必须绿
- **commit 1**: `fix(exec): cap_to_char_boundary for bash_tool UTF-8 panic`

### 阶段 2：time.compacted 幂等机制（约 1.5 天）

- D2：`synthia_context::Message` 增 `tool_result_cleared_at: Option<Instant>` 字段
- 单元测试：序列化/反序列化向后兼容
- D3：新增 `synthia_context::pruning::prune(messages, protect_tokens) -> PruneStats`
- 单元测试：单遍扫描 + 幂等停止 + 40K 保护
- 渲染层识别：在 `truncate_messages` 或 `step_sample` 之前的 message 渲染点，识别 `tool_result_cleared_at.is_some()` → 输出 `"[Old tool result content cleared at {ISO8601}]"`
- **commit 2**: `feat(context): Message.tool_result_cleared_at + prune() with PRUNE_PROTECT=40K`

### 阶段 3：apply_compaction 单遍化（约 1 天）

- D4：`apply_compaction` 改单次 `estimate_tokens`
- D5：`compact_level1` 接受 `previous_summary: Option<&str>` 参数 + prompt 模板分支
- 单元测试：所有现有 `apply_compaction` / `compact_level1` 单测通过
- 单元测试：`previous_summary` prompt 注入正确
- `recovery_cascade::try_l4_compact` 共享 `original_tokens`
- **commit 3**: `perf(context): apply_compaction single-pass + previous_summary anchor`

### 阶段 4：agent_tools.rs 拆分（约 1 天）

- D6：创建 7 子文件，迁移代码
- 保留 `agent_tools.rs` 作为 shim `pub use agent::*;`
- `cargo test --workspace` 全绿作为拆分成功标志
- **commit 4**: `refactor(agent): split agent_tools.rs into 7 modules (1300→<300 lines each)`

### 阶段 5：Verify 集成测试（约 0.5 天）

- Integration test：`tests/compact_truncate_pipeline.rs`
  - 端到端：tool 输出 > 16K → bash UTF-8 安全截断 → message 进入 LLM → 多次 compaction → 决策保留
- `cargo clippy --all-targets --all-features --tests --all` 无 warning
- `cargo +nightly fmt --all`
- **commit 5**: `test: integration tests for compact/truncate/prune pipeline`

### 阶段 6：Archive + Retrospective

- `openspec archive compact-truncate-prune-convergence --yes`
- 写 `retrospective.md`（commit 范围 + 偏差 + 经验）
- 同步 5 个新 spec 到 `openspec/specs/`

### 回滚

- 5 个 commit 独立，可单独 revert
- D2 字段 `serde(default)` 旧消息无破坏
- D6 shim 重导出公开 API 无破坏
- 任何中间状态可单独 revert

---

## Open Questions

- **OQ1**: `prune()` 函数放在 `synthia-context::pruning` 还是新建 `synthia-context::prune`？
  - 倾向：`pruning` 子模块（已有，软扩张，OQ1 答复）
  - 理由：现有 `soft_trim_content` / `hard_clear_content` / `micro_compact` 都在 `pruning`，命名一致

- **OQ2**: `Message::tool_result_cleared_at` 字段是否同时影响 `ToolOutput` 类型？
  - 倾向：仅影响 `Message` 渲染层（`ToolOutput` 保持不变，OQ2 答复）
  - 理由：`ToolOutput` 是工具执行的瞬时返回，不进 LLM context；`Message` 才是 LLM 可见

- **OQ3**: `apply_compaction` 改单次 estimate 后，`CompactionResult` 的 `original_tokens` 字段语义是否变化？
  - 倾向：不变化（仍然是 1 次 estimate 的值，只是调用次数从 3 减到 1，OQ3 答复）
  - 理由：原代码 3 次 estimate 值相同（输入未变），所以语义无变化

- **OQ4**: `previous_summary` 截断到 4K 字符是否本 change 实现？
  - 倾向：**不实现**（OQ4 留作 follow-up）
  - 理由：本 change 优先做核心机制；截断优化是次要风险
  - 风险评估：R4 缓解留作 follow-up

- **OQ5**: `prune()` 是否在 `StepCompact` 之前自动调用？
  - 倾向：**不**（保持现有 `try_compact` 流程不变）
  - 理由：本 change 仅提供 `prune()` 函数 + spec 锁定；调用方集成留作 follow-up
  - 风险评估：避免侵入 `stream_builder/steps/compact.rs`

---

## Verification

- 每个 spec 配 ≥ 6 unit tests（行为锁定）
- 每个 spec 配 ≥ 1 integration test（端到端）
- bash UTF-8 panic 修复配 1 个 regression test（输入多字节 UTF-8 + 切点在中段）
- `agent_tools.rs` 拆分后**所有现有测试必须通过**（零行为变化验证）
- `cargo clippy --all-targets --all-features --tests --all` 无 warning
- 公开 API 完全向后兼容（`agent_tools::*` 路径不变）
- 每个阶段独立 commit，可单独 revert
- 5 个能力独立 commit，可单独 revert
