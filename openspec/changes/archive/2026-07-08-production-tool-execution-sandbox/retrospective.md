# 回顾：production-tool-execution-sandbox

## 完成情况

- 所有 43 项任务已标记完成。
- 新增/修改文件（核心）：
  - `crates/synthia-tool-orchestrator/src/lib.rs`（编排器核心实现）
  - `crates/synthia-tool-orchestrator/src/tests.rs`（新增 mock 测试）
  - `crates/synthia-permission/src/approval.rs`（审批服务与缓存）
  - `crates/synthia-sandbox/src/lib.rs` / `composite.rs` / `backends/*.rs`（沙箱管理器）
  - `crates/synthia-tool/src/builtin/read.rs`、`write.rs`、`apply_patch/`、`glob.rs`、`grep.rs`（文件工具）
  - `crates/synthia-tool/src/events.rs`（文件变更事件）
  - `crates/synthia-agent/src/enhanced_dispatch.rs`、`tool_executor/mod.rs`（标记 deprecated）
  - `crates/synthia-agent/src/tools/builtins/file_tools.rs`、`search_tools.rs`（包装器测试）

## 做得好的地方

1. **代码实现已大幅超前于 tasks 状态**：开始执行时发现 Task 2-5 的核心实现已经存在，只需补充测试与集成点，避免了重复造轮子。
2. **测试补齐有重点**：针对最容易回归的路径补充测试（路径遍历、BOM 处理、行范围读取、审批超时、沙箱不可用降级、事件转发）。
3. **安全策略一致**：`OnUnavailable::Deny` 失败封闭、`HeadlessApprovalService` deny-by-default、文件工具统一 `check_path_safety`，与项目 fail-closed 原则保持一致。

## 遇到的问题

1. **tasks.md 与实际代码状态不同步**：tasks.md 中大量任务仍标记为未完成，但实际代码已实现。需要先调查再决定补充项，而不是直接按 tasks 重新实现。
2. **worktree 提交流程被中断**：用户两次取消 `git commit` 命令，导致分支上没有 commit，merge 时显示 "Already up to date"。后来明确允许后才完成提交与合并。
3. **主工作区存在未提交的 bash_permission.rs 改动**：在 worktree 外也修改了同一个测试文件，需要先提交到 master 才能继续 merge。

## 经验教训

- 执行 OpenSpec change 前应先快速核对代码实际状态，避免被 tasks.md 的过时状态误导。
- worktree 中未提交的改动不会自动进入分支；finishing-a-development-branch 的 merge 步骤之前必须确认分支已提交。
- 跨工作区编辑同一文件容易产生“幽灵改动”，需要 `git status` 仔细检查。

## 后续可改进项

- 考虑把 `external_directory` 权限作为显式 `Permission` 变体，支持授权访问工作区外目录（当前仅通过工作区边界强制拒绝）。
- 为 `OnUnavailable::Prompt` 增加真正的用户提示层（当前直接降级为无沙箱执行并审计）。
- 当 `otel` feature 稳定后，可在 orchestrator 生命周期事件中补充 span 属性。
