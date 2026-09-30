## 1. V4A 解析器实现 (synthia-tool/src/builtin/v4a.rs)

- [x] 1.1 定义 `PatchOp` 枚举（`Update { path, hunks, move_to: Option<PathBuf> }` / `Add { path, content }` / `Delete { path }`）和 `Hunk { context: Vec<String>, insertions: Vec<String>, deletions: Vec<String>, end_of_file: bool }` 数据结构
- [x] 1.2 实现 `parse_v4a(input: &str) -> Result<Vec<PatchOp>, ParseError>` 状态机，识别 `*** Begin Patch` / `*** End Patch` 标记和 3 种 op 头（Update/Add/Delete；Move 单独在 1.3 处理）
- [x] 1.3 实现 Move 解析：`Update { ..., move_to: Some(...) }` 通过 `*** Move to: <path>` 行设置 `move_to`，**解析期总是接受**（V4A 协议兼容），运行期由 D2.5 配置 flag 控制
- [x] 1.4 实现 hunk 行解析（` ` 上下文 / `+` 插入 / `-` 删除 / `*** End of File` 标记 / `@@` 锚点）
- [x] 1.5 定义 `ParseError` 枚举（`MissingBeginMarker` / `MissingEndMarker` / `UnknownOpHeader` / `HunkWithoutUpdate` / `HunkOutOfOrder`）
- [x] 1.6 为 `parse_v4a` 写 6 个单元测试（valid Update / valid Add / valid Delete / Update+Move / 4 种 ParseError 各 1 个）

## 2. ApplyPatchTool 实现 (synthia-tool/src/builtin/apply_patch.rs)

- [x] 2.1 定义 `ApplyPatchTool { enable_move: bool }` 结构体（默认 `enable_move = false`），impl `Default`（`#[derive(Default)]`）
- [x] 2.2 实现 `Tool::name()` / `description()` / `parameters()` / `requires_permission() -> true` / `is_concurrency_safe() -> false`
- [x] 2.3 description 字段写明 "Operations apply sequentially; if a later operation fails, earlier operations remain applied and the failure reports them explicitly. Moves are not supported yet."
- [x] 2.4 实现 `Tool::call(input)` 主流程：**Parse → Resolve paths → `check_path_safety` 全部 → Permission check → Sequential apply + accumulate results**（**无 snapshot / 无 dry-run 状态机**）
- [x] 2.5 解析阶段调 `v4a::parse_v4a`；ParseError 立即返回 `ToolOutput::error`（filesystem 必然未触及）
- [x] 2.6 路径解析 + 安全检查阶段：遍历 `Vec<PatchOp>`，对 Update/Add/Delete 的 path 和 Update.move_to 调 `check_path_safety`；任一失败立即返回 `ToolOutput::error`
- [x] 2.7 顺序应用阶段：按 source order 遍历 `Vec<PatchOp>`，对每个 op 调 `apply_one_op(op, &mut applied_results)`；任一 op 失败时**停止后续 op**，把已成功的 op 加入 `applied: Vec<PatchOp>`，把失败的 op + reason 加入 `failed: (PatchOp, String)`，返回 `AppliedFailure { applied, failed }`
- [x] 2.8 Move 拒绝逻辑：在 2.7 中如果 `enable_move = false` 且任何 op 含有 `move_to: Some(_)`，返回 `ToolFailure("apply_patch moves are not supported yet")`（**必须在顺序应用前检查**，避免半应用状态）
- [x] 2.9 输出格式化：
  - 全部成功 → `ToolOutput::text("Applied N operations: A path1, M path2, D path3, ...")`
  - 部分失败 → `ToolOutput::error("Applied M of N operations. Succeeded: [...]. Failed: <op> — <reason>")`

## 3. Tool Registry 集成

- [x] 3.1 在 `synthia-tool/src/builtin/mod.rs` 新增 `pub mod apply_patch;` 和 `pub mod v4a;`，导出 `ApplyPatchTool`
- [x] 3.2 在 `synthia-tool/src/registry/registration.rs` 的 `register_defaults()` 中追加 `registry.register(ToolEntry::new(Arc::new(ApplyPatchTool::default())));`
- [x] 3.3 验证 `ToolRegistry::list()` 现在返回 8 个工具（含 apply_patch）— test_registers_in_default_registry

## 4. Codex Portable Scenario 集成测试 (synthia-tool/tests/codex_scenarios.rs)

> **完成状态（2026-06-13）**：22 个 codex portable scenarios 全部移植完成。
> 全部通过 fixture-based 测试运行器 `crates/synthia-tool/tests/codex_scenarios.rs`（参照 codex `tests/suite/scenarios.rs`）：
> - 22 个目录从 `codex-rs/apply-patch/tests/fixtures/scenarios/` 复制到 `crates/synthia-tool/tests/fixtures/codex/`
> - 每个 scenario 生成独立 `#[tokio::test]` 函数 (`codex_scenario_<NNN>_<name>`)
> - 运行器使用 `ApplyPatchTool { enable_move: true }`（D2.5 默认 `false` 在 production 保留，test 显式 opt-in 启用 Move 以覆盖 004/010 场景）
> - 实际产出：synthia-tool lib 67 tests + codex_scenarios 24 tests = **91 测试通过**
> - 此外修复了 3 个 parser/runtime 缺陷以达成 100% 兼容性：
>   1. `Hunk` 重构为 `lines: Vec<HunkLine>`（`Context`/`Insertion`/`Deletion`）保留源序 — 修复 scenario 021（` line1 / -line2 /  line3` 上下文/删除交错）
>   2. `apply_hunks` 支持 pure addition hunk（旧 `old_text` 为空时改为 append `new_text`）— 修复 scenario 016
>   3. `apply_one` 在 `Add` 时 `create_dir_all(parent)` — 修复 scenario 002 (`nested/new.txt`)
>   4. 移除 `Add` 路径的 "file already exists" 拒绝（与 codex 011 一致：`*** Add File:` 允许覆盖）
>
> **保留的 inline 单元测试**（`src/builtin/apply_patch.rs` mod tests）— 用于覆盖 non-scenario 行为（path traversal、move-disabled 拒绝、空 hunk 拒绝、registry、permission、concurrency）：

- [x] 4.1 集成 codex scenario 001 `add_file` — `codex_scenario_001_add_file` ✅
- [x] 4.2 集成 codex scenario 002 `multiple_operations` — `codex_scenario_002_multiple_operations` ✅
- [x] 4.3 集成 codex scenario 003 `multiple_chunks` — `codex_scenario_003_multiple_chunks` ✅
- [x] 4.4 集成 codex scenario 004 `move_to_new_directory` — `codex_scenario_004_move_to_new_directory` ✅ (via `enable_move = true` in runner)
- [x] 4.5 集成 codex scenario 005 `rejects_empty_patch` — `codex_scenario_005_rejects_empty_patch` ✅
- [x] 4.6 集成 codex scenario 006 `rejects_missing_context` — `codex_scenario_006_rejects_missing_context` ✅
- [x] 4.7 集成 codex scenario 007 `rejects_missing_file_delete` — `codex_scenario_007_rejects_missing_file_delete` ✅
- [x] 4.8 集成 codex scenario 008 `rejects_empty_update_hunk` — `codex_scenario_008_rejects_empty_update_hunk` ✅
- [x] 4.9 集成 codex scenario 009 `requires_existing_file_for_update` — `codex_scenario_009_requires_existing_file_for_update` ✅
- [x] 4.10 集成 codex scenario 010 `move_overwrites_existing_destination` — `codex_scenario_010_move_overwrites_existing_destination` ✅ (via `enable_move = true` in runner)
- [x] 4.11 集成 codex scenario 011 `add_overwrites_existing_file` — `codex_scenario_011_add_overwrites_existing_file` ✅
- [x] 4.12 集成 codex scenario 012 `delete_directory_fails` — `codex_scenario_012_delete_directory_fails` ✅
- [x] 4.13 集成 codex scenario 013 `rejects_invalid_hunk_header` — `codex_scenario_013_rejects_invalid_hunk_header` ✅
- [x] 4.14 集成 codex scenario 014 `update_file_appends_trailing_newline` — `codex_scenario_014_update_file_appends_trailing_newline` ✅
- [x] 4.15 集成 codex scenario 015 `failure_after_partial_success_leaves_changes` — `codex_scenario_015_failure_after_partial_success_leaves_changes` ✅ (**核心场景**)
- [x] 4.16 集成 codex scenario 016 `pure_addition_update_chunk` — `codex_scenario_016_pure_addition_update_chunk` ✅
- [x] 4.17 集成 codex scenario 017 `whitespace_padded_hunk_header` — `codex_scenario_017_whitespace_padded_hunk_header` ✅
- [x] 4.18 集成 codex scenario 018 `whitespace_padded_patch_markers` — `codex_scenario_018_whitespace_padded_patch_markers` ✅
- [x] 4.19 集成 codex scenario 019 `unicode_simple` — `codex_scenario_019_unicode_simple` ✅
- [x] 4.20 集成 codex scenario 020 `delete_file_success` — `codex_scenario_020_delete_file_success` ✅
- [x] 4.20b 集成 codex scenario 020 `whitespace_padded_patch_marker_lines` — `codex_scenario_020_whitespace_padded_patch_marker_lines` ✅
- [x] 4.21 集成 codex scenario 021 `update_file_deletion_only` — `codex_scenario_021_update_file_deletion_only` ✅
- [x] 4.22 集成 codex scenario 022 `update_file_end_of_file_marker` — `codex_scenario_022_update_file_end_of_file_marker` ✅

**保留的 inline 单元测试**（不映射到 codex scenario；覆盖本地非场景行为）：
- [x] 4.31 `test_add_file`, `test_update_file`, `test_delete_file`, `test_multiple_operations_all_succeed`, `test_partial_failure_leaves_applied` — 基础 round-trip
- [x] 4.32 `test_move_rejected_by_default` — D2.5 默认 `enable_move = false` 行为
- [x] 4.33 `test_path_traversal_blocked` — 路径安全
- [x] 4.34 `test_rejects_empty_patch` — 解析器拒绝
- [x] 4.35 `test_registers_in_default_registry` — registry 验证
- [x] 4.36 `test_requires_permission` — 元数据（permission / concurrency / name）
- [x] 4.37 `test_add_overwrites_existing` — `*** Add File:` 允许覆盖（替代旧的 `test_add_overwrites_existing_blocked`）
- [x] 4.38 `test_delete_directory_blocked` — 拒绝删除目录

## 5. 验收与提交

- [x] 5.1 运行 `cargo +nightly fmt --all`，确保无 diff
- [x] 5.2 运行 `cargo clippy --all-targets --all-features --tests -p synthia-tool`，无新增 warning（21 个 warning 全部 pre-existing，与本 change 无关）
- [x] 5.3 运行 `cargo test -p synthia-tool`，**96 测试通过**（synthia-tool lib 67 + codex_scenarios 24 + registry_test 5）
- [x] 5.4 运行 `cargo test -p synthia-agent --lib`，**496 测试通过**（确认注册不影响其他工具）
- [x] 5.5 在 `openspec/changes/apply-patch-tool/` 下确认 6 个 artifact 完整（brainstorm.md / design.md / proposal.md / specs/apply-patch-tool/spec.md / tasks.md / review.md）
- [x] 5.6 提交到 git：`feat(tool): port all 22 codex apply-patch scenarios + enable Move via flag`

## 后续跟踪（out of scope）

- ~~22 个 codex portable scenario 完整移植~~ ✅ 完成（2026-06-13）
- `path.rs` 的 `safe_canonicalize` 实现可进一步优化（当前 O(n) 组件遍历）
- `apply_patch.rs` 的 `find_hunk` 当前仅支持基本 fallback；可加入 fuzzy matching（codex 有 `seek_sequence.rs` 163 行专门做 hunk context 匹配算法）
- 启用 `enable_move` 在 production 仍需 D2 atomic rollback 设计 + 跨文件系统 mv 兼容性测试 + Guardian policy 扩展（保留 D2.5 默认 disabled 决策）
