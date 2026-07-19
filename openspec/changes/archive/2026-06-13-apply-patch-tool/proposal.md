## Why

Synthia 当前 7 个内置工具中，文件编辑类（read / write / multi_edit）走的是 find-and-replace 协议。LLM 必须把"我想改这一段"翻译成"old_str + new_str"两次往返，prompt token 消耗高且 context 漂移导致 old_str 不匹配的失败率高。此外多文件修改无原子性保证（多次 write 中途失败会留半完成状态），且没有"重命名"或"创建新文件"的原子操作。本次新增 Anthropic V4A `apply_patch` 工具，单 tool 一次往返处理多文件事务，**顺序应用 + 结构化 applied/failed 报告**（**对齐 codex scenario 015 + opencode 显式 "atomic rollback are not supported yet" 立场**），对齐 opencode / codex / Claude Code 的协议习惯。

## What Changes

**新增 `apply_patch` 工具**
- From: LLM 通过 multi_edit 实现多文件修改，但无原子性、需多次往返、且不支持重命名
- To: LLM 通过单次 apply_patch 调用处理 V4A 格式 patch，**跨文件顺序应用，部分失败时保留已应用并结构化报告**（**不实现 atomic rollback，与 codex/opencode 保持一致**）
- Reason: 与 Anthropic 官方 V4A 规范对齐，token 消耗更低、错误率更低、用户体验更接近 Claude Code
- Impact: 非破坏性。新工具与 multi_edit 并存，LLM 自主选择

**新增 V4A 解析器**
- From: 无 V4A 解析能力，文件编辑只能走 find-replace
- To: 实现 V4A spec 完整解析（Begin/End Patch 标记、4 种 op 头、context/diff 行、End of File 标记）——Move 在解析期接受、运行期由 `enable_move` flag 控制
- Reason: 让 LLM 能用 Anthropic 原生协议操作 Synthia
- Impact: 内部模块，对外只暴露 ApplyPatchTool 一个 Tool trait 实现

**注册到 tool registry**
- From: `register_defaults()` 注册 7 个工具
- To: `register_defaults()` 注册 8 个工具（含 apply_patch）
- Reason: 工具默认对 LLM 可见
- Impact: tool list 长度 +1

**集成 codex 22 个 portable scenario fixture**
- From: 5 个核心 happy path 测试
- To: 22 个 codex portable scenario（`001_add_file` 到 `022_update_file_end_of_file_marker`，含 `015_failure_after_partial_success_leaves_changes` 核心场景） + 3 个自定义测试
- Reason: codex README 明确说明 scenario "meant to be easily portable to other languages or platforms"——是 V4A 协议的事实标准测试集
- Impact: 31 个新测试，确保 V4A 协议 100% 兼容

## Capabilities

### New Capabilities
- `apply-patch-tool`: 暴露 V4A 格式的 apply_patch 内置工具，支持 Update/Add/Delete 三种 op 头 + 解析期接受 Move hunk（运行期默认禁用）；顺序应用多文件操作；部分失败时保留已应用并结构化报告 `AppliedFailure { applied, failed }`

### Modified Capabilities
- (none — 不修改现有 spec 的 requirement，仅在 synthia-tool crate 内新增 builtin tool)

## Impact

**Affected code**:
- `crates/synthia-tool/src/builtin/mod.rs` — 新增 `pub mod apply_patch;` 和 `pub mod v4a;`，导出 `ApplyPatchTool`
- `crates/synthia-tool/src/builtin/apply_patch.rs` — 新增 ApplyPatchTool 实现（约 350 行，含 D2.5 Move 拒绝逻辑 + AppliedFailure 结构）
- `crates/synthia-tool/src/builtin/v4a.rs` — 新增 V4A 解析器（约 250 行，含 Move 解析）
- `crates/synthia-tool/src/registry/registration.rs` — `register_defaults()` 追加 `ApplyPatchTool` 注册
- `crates/synthia-tool/tests/fixtures/codex/<NNN>_<name>/` — 22 个 codex scenario fixture 目录（patch.txt + input/ + expected/）

**Affected APIs**:
- `ToolRegistry` — 新增 `apply_patch` 工具名（向后兼容，旧 multi_edit 调用不受影响）
- `ApplyPatchTool` 结构体 — `enable_move: bool` 字段（serde 默认 false，未来通过 config 暴露）

**Affected systems**:
- Guardian 审批流 — `requires_permission = true`，复用 write policy
- Permission checker — 不变（write policy 已覆盖 apply_patch 语义）
- Concurrency control — `is_concurrency_safe = false`，与 WriteTool 一致

**Dependencies**: 无新增（V4A 解析是纯文本处理，标准库足够）

**Test coverage**:
- 22 个 codex portable scenario（含核心 015 失败语义）
- 6 个 v4a 解析器单元测试
- 3 个自定义集成测试（path traversal / move 禁用 / registry 验证）
- 现有 `cargo test -p synthia-tool` 全部通过
- `cargo test -p synthia-agent` 全部通过（确认注册不影响其他工具）
