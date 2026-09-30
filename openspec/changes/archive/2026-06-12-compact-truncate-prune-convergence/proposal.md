## Why

Synthia 当前有 **4 套 truncate 路径 + 3 套 compaction 入口** 并存。其中 `bash_tool.rs:125, 136` 的 `String::truncate(usize)` 是 **P0 生产 panic 风险**（多字节 UTF-8 末尾）。同时 `apply_compaction` L1→L2→L3 失败链跑 **3 次完整 O(n) `estimate_tokens`**，`recovery_cascade::try_l4_compact` 单次触发 **3 次 O(n)**，n=10K 时退化明显。Synthia 还缺 OpenCode 的 4 个核心机制：`time.compacted` 幂等标记、`<previous-summary>` 锚定、PRUNE_PROTECT 40K tail 保护、单遍贪心 select。

本 change 优先做 **修 panic + 增量改进**（C4 + C1.3 合并提案），遵循"先修 critical bug + 删重复代码，6 个月后再考虑 trait 抽象"的项目约定。C2 turn 抽象与 C3 ACP 是更深的结构性 gap，留待 2026-12 再评估。

**预期收益**：消除 UTF-8 panic 风险 + 改善 KV cache 命中率（10× 成本放大）+ 多次 compaction 决策不蒸发 + `agent_tools.rs` 1300+ 行单文件拆为 7 个 < 300 行文件。

## What Changes

**bash UTF-8 panic 修复**
- From: `bash_tool.rs:125, 136` 直接 `String::truncate(usize)`，多字节 UTF-8 末尾 panic
- To: 私有 `cap_to_char_boundary` 函数向 char boundary 退缩，零 panic 风险
- Reason: 修 P0 生产 bug
- Impact: non-breaking, 行为等价（实际截断字节数 ≤ max_output_length）

**time.compacted 幂等机制**
- From: `pruning::micro_compact` 直接改写 message content，prefix hash 变化 → KV cache miss
- To: `Message::tool_result_cleared_at: Option<Instant>` 字段，渲染层识别后输出 placeholder
- Reason: 物理保留原 content，prefix hash 跨 prune 不变（呼应 P1 原则）
- Impact: non-breaking（`#[serde(default)]` 旧消息零影响）

**prune() 单遍扫描 + PRUNE_PROTECT=40K**
- From: 缺工具级 prune 入口
- To: `synthia_context::pruning::prune(messages, 40_000)` 单遍反向扫描，遇 `tool_result_cleared_at` 立即停止
- Reason: 改善长 session token 累积 + 幂等性
- Impact: additive（新函数，旧行为不变）

**apply_compaction 单遍化 + 贪心选 level**
- From: L1→L2→L3 失败链跑 3 次 `estimate_tokens`，单次 L4 触发 3×O(n)
- To: 单次 `estimate_tokens` + 共享 `original_tokens` 给 `recovery_cascade::try_l4_compact`
- Reason: 消除 3×O(n) 重复扫描
- Impact: non-breaking（保留 L1→L2→L3 退化策略，仅优化 estimate 调用次数）

**`<previous-summary>` 锚定（仅 L1）**
- From: `compact_level1` 独立总结，多次 compaction 后早期决策蒸发
- To: `compact_level1(messages, provider, previous_summary: Option<&str>)` + prompt 模板分支
- Reason: 多次 compaction 累积决策不蒸发
- Impact: non-breaking（参数为 `Option`，默认 `None` 行为同现）

**agent_tools.rs 拆分**
- From: 1300+ 行单文件承担 8 个职责
- To: 7 个子文件（message_bus/instance/manager + tools/{spawn,send,team_create,team_delete}）
- Reason: 维护性，零行为变化
- Impact: non-breaking（保留 `agent_tools.rs` shim 重导出全部公开符号）

## Capabilities

### New Capabilities

- `bash-utf8-safe-truncate`: bash_tool UTF-8 安全截断（`cap_to_char_boundary` 包装 + regression test）
- `prune-idempotent-marker`: `time.compacted` 幂等标记机制（`Message` 字段 + 渲染层识别 + `prune()` 单遍扫描）
- `compaction-single-pass`: `apply_compaction` 单遍化 + `<previous-summary>` 锚定（仅 L1）+ `recovery_cascade` 共享 `original_tokens`
- `agent-tools-split`: `agent_tools.rs` 1300+ 行拆为 7 个子文件（公开 API 通过 shim 重导出保持）

### Modified Capabilities

- `context-compaction`: 增加 `<previous-summary>` 锚定要求到 L1 失败路径（"compaction SHALL preserve prior summary when L1 succeeds"）
- `tool-output-truncate`: 增加 UTF-8 安全要求（"truncation SHALL NOT panic on multi-byte UTF-8 character boundaries"）

## Impact

**代码影响**：
- `crates/synthia-context/`：`Message` 结构 + `pruning` 模块新函数 + `compaction` 模块重构
- `crates/synthia-exec/`：`bash_tool.rs` 私有函数替换
- `crates/synthia-agent/`：`tools/agent_tools.rs` → 7 子文件 + shim
- 测试文件新增：`bash_utf8_panic.rs`、`prune_idempotent.rs`、`compaction_single_pass.rs`、`compact_truncate_pipeline.rs`（integration）

**API 影响**：完全向后兼容
- `Message` 新字段 `#[serde(default)]` 旧消息零影响
- `agent_tools::*` 路径保留（shim 重导出）
- `apply_compaction` / `compact_level1` 签名变化是内部 API（pub 但限 agent crate 内使用）

**依赖影响**：无新增外部依赖
- 使用 `std::time::Instant`（已用）
- 使用 `serde::Serialize/Deserialize`（已用）
- 使用 `parking_lot::Mutex`（已用）

**测试影响**：
- 新增 5+ 单元测试（每个 spec ≥ 6）
- 新增 1 integration test
- 现有 `bash_tool` / `compactor` / `agent_tools` 单测必须 100% 通过
- 0 个现有测试需修改

**部署影响**：无
- 纯代码重构 + bug 修复
- 无数据库 schema 变化
- 无 endpoint 变化
- 无配置变化

**风险**：
- `apply_compaction` 重构（Med 风险）— 行为锁定 + 所有现有单测通过
- `Message` 字段（Low 风险）— `serde(default)` 兼容
- `bash_tool` 修复（None 风险）— regression test 锁定
- `agent_tools` 拆分（Low 风险）— shim + 全 workspace 测试通过
