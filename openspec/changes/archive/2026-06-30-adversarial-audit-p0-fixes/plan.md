# Adversarial Audit P0 Fixes Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** 修复对抗性审计发现的 5 个高 ROI 问题：bash 沙箱单点失效、文件工具路径校验薄弱、guardian cache 浪费、prefix_tracker 可观测性盲区、pipeline O(n²) 反模式。

**Architecture:** 5 个独立修复，按 ROI 排序（最低成本最高收益优先）。每个修复独立提交可回滚。U1/U2 涉及安全语义，需新增测试；其余 3 项为纯内部优化，现有测试应保持通过。

**Tech Stack:** Rust（edition 2021）、tokio async runtime、cargo +nightly fmt、cargo clippy、openspec validate

---

## Task 1: Prompt Assembly O(n²) 修复

**文件**：`crates/synthia-context/src/assembler/pipeline.rs`
**验证命令**：`cargo test -p synthia-context`

- [ ] **Step 1:** 读取 `crates/synthia-context/src/assembler/pipeline.rs` 行 38-48，确认 `trimmed.remove(0)` 在 while 循环中的 O(n²) 反模式。理解当前 trim 语义：从最旧消息移除直到 `total_tokens <= max_tokens`，保留至少 1 条
- [ ] **Step 2:** 编写测试 `test_protection_zone_trim_linear_time`：构造 200 条消息（每条略超预算），断言 trim 在合理时间内完成（基准：< 10ms，当前 O(n²) 应 > 50ms）
- [ ] **Step 3:** 重写 trim 逻辑：用 `Vec::drain(0..count)` 一次性移除 `count` 条最旧消息，其中 `count` 由预先计算得出（二分搜索或线性扫描确定要移除多少条）。或维护 `start_index` 而非 mutate Vec，最后 `trimmed = trimmed[start_index..].to_vec()`
- [ ] **Step 4:** 确保语义不变：从最旧消息开始移除直到 `total_tokens <= max_tokens`，保留最近 1 条
- [ ] **Step 5:** 运行 `cargo test -p synthia-context`，确认新测试与现有测试全部通过
- [ ] **Step 6:** 运行 `cargo +nightly fmt --all` 与 `cargo clippy -p synthia-context --all-targets --all-features --tests`，修复所有警告
- [ ] **Step 7:** 提交：`git add crates/synthia-context/src/assembler/pipeline.rs && git commit -m "fix(context): replace O(n²) remove(0) with O(n) drain in ProtectionZone trim"`

---

## Task 2: Guardian/E2E Cache Policy 注入

**文件**：`crates/synthia-guardian/src/review/reviewer.rs`、`crates/synthia-agent/tests/e2e_llm_test.rs`
**验证命令**：`cargo test -p synthia-guardian && cargo test -p synthia-agent --test e2e_llm_test`

- [ ] **Step 1:** 读取 `crates/synthia-guardian/src/review/reviewer.rs` 行 217、313，确认两处 `cache_policy: None`
- [ ] **Step 2:** 读取 `crates/synthia-agent/tests/e2e_llm_test.rs` 行 120、200，确认两处 `cache_policy: None`
- [ ] **Step 3:** 在 reviewer.rs 行 217 将 `cache_policy: None` 改为 `cache_policy: Some(CachePolicy::default())`，确保 `use synthia_provider::CachePolicy;` 已导入（若未导入则添加）
- [ ] **Step 4:** 在 reviewer.rs 行 313 同样改为 `cache_policy: Some(CachePolicy::default())`
- [ ] **Step 5:** 在 e2e_llm_test.rs 行 120、200 同样改为 `cache_policy: Some(CachePolicy::default())`，确保导入 `CachePolicy`
- [ ] **Step 6:** 运行 `cargo test -p synthia-guardian` 确认 guardian 测试通过
- [ ] **Step 7:** 运行 `cargo test -p synthia-agent --test e2e_llm_test` 确认 e2e 测试通过（可能需要 mock provider，若 e2e 依赖真实 API 则跳过此步但确认编译通过）
- [ ] **Step 8:** 运行 `cargo +nightly fmt --all` 与 `cargo clippy`，修复所有警告
- [ ] **Step 9:** 提交：`git add crates/synthia-guardian/src/review/reviewer.rs crates/synthia-agent/tests/e2e_llm_test.rs && git commit -m "fix(guardian): inject CachePolicy::default() to eliminate ~\$19.7K/yr cache miss waste"`

---

## Task 3: Bash 沙箱接入（U1 安全单点失效）

**文件**：`crates/synthia-tool-orchestrator/src/lib.rs`、`crates/synthia-tool-bash/src/bash_tool/executor.rs`、`crates/synthia-tool-bash/src/bash_tool/trait_impl.rs`
**验证命令**：`cargo test -p synthia-tool-bash && cargo test -p synthia-tool-orchestrator`

- [ ] **Step 1:** 读取 `crates/synthia-tool-orchestrator/src/lib.rs` 行 731，确认 `ToolAdapter::execute` 形参 `_sandbox_attempt`（下划线前缀 = 显式忽略）
- [ ] **Step 2:** 读取 `crates/synthia-tool-bash/src/bash_tool/executor.rs` 行 30-34，确认 `tokio::time::timeout(dur, Command::new("bash").arg("-c").arg(command).output())` 从不调用 `SandboxAttempt::wrap()`
- [ ] **Step 3:** 读取 `crates/synthia-tool-bash/src/bash_tool/trait_impl.rs` 行 108-113，确认后台 spawn 路径同样裸 spawn
- [ ] **Step 4:** 读取 `crates/synthia-sandbox/src/lib.rs` 的 `SandboxAttempt` trait，确认 `wrap(&mut command: &mut Command)` 方法签名与 `on_unavailable()` 决策路径
- [ ] **Step 5:** 编写测试 `test_bash_executor_calls_sandbox_wrap_when_available`：mock `SandboxAttempt` 返回 `Available`，断言 `wrap` 被调用（用 spy 模式或断言 command 的 env/args 包含 bwrap 前缀）
- [ ] **Step 6:** 编写测试 `test_bash_executor_denies_when_sandbox_unavailable_standard_policy`：mock `SandboxAttempt` 返回 `Unavailable`，`SandboxPolicy::Standard`，断言 bash 工具返回 Deny 而非执行
- [ ] **Step 7:** 在 `ToolAdapter::execute` 移除 `_sandbox_attempt` 下划线前缀，命名为 `sandbox_attempt`
- [ ] **Step 8:** 修改 `ToolAdapter::execute` 将 `sandbox_attempt` 传递给 `tool.execute()`（而非丢弃）
- [ ] **Step 9:** 修改 bash executor 的 `execute_command`：在 `Command::new("bash")` 构造后、`.output()` 前，调用 `sandbox_attempt.wrap(&mut command)`。若 `sandbox_attempt` 为 `Unavailable` 且 policy 为 `Standard`，返回 `Err(ToolError::SandboxUnavailable)` 而非执行
- [ ] **Step 10:** 修改后台 spawn 路径（trait_impl.rs 行 108-113）同样调用 `sandbox_attempt.wrap(&mut command)`
- [ ] **Step 11:** 运行 `cargo test -p synthia-tool-bash`，确认新测试通过且现有测试未破坏（现有测试若依赖裸跑需显式用 `SandboxPolicy::Allow` mock）
- [ ] **Step 12:** 运行 `cargo test -p synthia-tool-orchestrator`，确认 orchestrator 测试通过
- [ ] **Step 13:** 运行 `cargo +nightly fmt --all` 与 `cargo clippy`，修复所有警告
- [ ] **Step 14:** 提交：`git add crates/synthia-tool-orchestrator/src/lib.rs crates/synthia-tool-bash/src/bash_tool/ && git commit -m "fix(sandbox): wire SandboxAttempt::wrap into bash executor to close single-point-of-failure"`

---

## Task 4: 文件工具路径校验改为 Workspace 包含（U2 安全）

**文件**：`crates/synthia-permission/src/checker/checker.rs`
**参考**：`codex/codex-rs/core/src/safety.rs` 行 138-193
**验证命令**：`cargo test -p synthia-permission`

- [ ] **Step 1:** 读取 `crates/synthia-permission/src/checker/checker.rs` 行 229-240，确认 `check_path_traversal` 仅 `path.contains("../") || path.contains("..\\")`
- [ ] **Step 2:** 读取 `codex/codex-rs/core/src/safety.rs` 行 138-193 的 `normalize()` + `can_write_path_with_cwd()` 实现，理解纯路径运算算法（不调 `canonicalize()`）
- [ ] **Step 3:** 编写测试 `test_absolute_path_outside_workspace_denied`：`check_path_traversal("/etc/passwd", workspace_root="/workspace")` 返回 Deny
- [ ] **Step 4:** 编写测试 `test_absolute_path_to_ssh_key_denied`：`check_path_traversal("/home/victim/.ssh/id_rsa", workspace_root="/workspace")` 返回 Deny
- [ ] **Step 5:** 编写测试 `test_relative_traversal_denied`：`check_path_traversal("../../../etc/passwd", workspace_root="/workspace")` 返回 Deny
- [ ] **Step 6:** 编写测试 `test_workspace_internal_path_allowed`：`check_path_traversal("src/main.rs", workspace_root="/workspace")` 返回 Allow
- [ ] **Step 7:** 编写测试 `test_normalize_folds_dot_segments`：`normalize_path("workspace/src/../src/main.rs")` == `PathBuf::from("workspace/src/main.rs")`
- [ ] **Step 8:** 编写测试 `test_normalize_does_not_call_canonicalize`：确认 `normalize_path` 不触发文件系统 I/O（可用 `#[cfg(not(test))]` 或文档注释保证）
- [ ] **Step 9:** 实现 `fn normalize_path(path: &Path) -> PathBuf`：遍历 `path.components()`，用栈折叠 `.` 和 `..` 段，不调 `canonicalize()`
- [ ] **Step 10:** 实现 `fn is_path_in_workspace(normalized: &Path, workspace_root: &Path) -> bool`：将两者都 normalize 后做前缀包含判定（`normalized.starts_with(workspace_root_normalized)`）
- [ ] **Step 11:** 重写 `check_path_traversal`：`let normalized = normalize_path(path); if !is_path_in_workspace(&normalized, workspace_root) { return Deny; }`
- [ ] **Step 12:** 运行 `cargo test -p synthia-permission`，确认所有新测试通过且现有测试未破坏
- [ ] **Step 13:** 运行 `cargo +nightly fmt --all` 与 `cargo clippy -p synthia-permission --all-targets --all-features --tests`，修复所有警告
- [ ] **Step 14:** 提交：`git add crates/synthia-permission/src/checker/checker.rs && git commit -m "fix(permission): replace ../ substring check with workspace containment to prevent absolute path escape"`

---

## Task 5: PrefixTracker Hash 范围扩展

**文件**：`crates/synthia-context/src/prefix_tracker/tracker.rs`
**验证命令**：`cargo test -p synthia-context`

- [ ] **Step 1:** 读取 `crates/synthia-context/src/prefix_tracker/tracker.rs` 行 90-97，确认 `record_pre(&mut self, system_bytes: &[u8], turn_id: u64)` 只 hash `system_bytes`
- [ ] **Step 2:** 用 `grep -rn "record_pre"` 找到所有调用点，理解调用上下文（system_bytes 从哪来、tools 和 messages 前缀如何获取）
- [ ] **Step 3:** 编写测试 `test_tools_change_detected_by_hash`：system_bytes 不变但 tools_schema_bytes 变 → `record_pre` 返回的 hash 与之前不同
- [ ] **Step 4:** 编写测试 `test_messages_prefix_change_detected_by_hash`：system 与 tools 不变但 messages_prefix_bytes 变 → hash 改变
- [ ] **Step 5:** 编写测试 `test_stable_prefix_reports_high_stability`：三者均不变 → stability_ratio 接近 1.0
- [ ] **Step 6:** 修改 `record_pre` 签名：`fn record_pre(&mut self, system_bytes: &[u8], tools_schema_bytes: &[u8], messages_prefix_bytes: &[u8], turn_id: u64) -> String`
- [ ] **Step 7:** 修改 `compute_hash_bytes`：拼接 `system_bytes || tools_schema_bytes || messages_prefix_bytes` 后 SHA-256 hash。确保拼接顺序固定（system 在前，tools 中间，messages 最后）
- [ ] **Step 8:** 更新所有 `record_pre` 调用点：传入 tools schema bytes（序列化为确定性 JSON）与 messages 前缀 bytes（`tool_result_cleared_at` 之前的内容序列化）
- [ ] **Step 9:** 评估 `recent_window` 默认值：hash 范围扩大后稳定性分布可能变化，若测试显示 stability_ratio 异常下降，调整窗口大小
- [ ] **Step 10:** 运行 `cargo test -p synthia-context`，确认新测试通过且现有测试更新（调用签名已变）
- [ ] **Step 11:** 运行 `cargo +nightly fmt --all` 与 `cargo clippy -p synthia-context --all-targets --all-features --tests`，修复所有警告
- [ ] **Step 12:** 提交：`git add crates/synthia-context/src/prefix_tracker/ && git commit -m "fix(context): extend PrefixTracker hash to cover system+tools+messages for accurate stability_ratio"`

---

## Task 6: 集成验证与文档同步

- [ ] **Step 1:** 运行 `cargo test --all`，确保所有 crate 测试通过
- [ ] **Step 2:** 运行 `cargo +nightly fmt --all`，确保格式一致
- [ ] **Step 3:** 运行 `cargo clippy --all-targets --all-features --tests --all`，确保无警告
- [ ] **Step 4:** 运行 `openspec validate adversarial-audit-p0-fixes --strict`，确保变更规格有效
- [ ] **Step 5:** 更新 `/home/crochee/.trae-cn/memory/projects/-home-crochee-workspace-synthia/project_memory.md`：在 "Multi-Expert Adversarial Analysis Conclusions" 后追加 "### Adversarial Audit Follow-up (2026-06-29)"，记录：
  - 原 P0 清单 3/4 已完成（cache_breaker 删除、applyCachePolicy 实现、bash PermissionChecker 接入），1/4 降级（bubblewrap 工程细节降为 P1-SaaS-only）
  - 新发现的 5 个高 ROI 修复项已捕获为 `adversarial-audit-p0-fixes` change
  - 4 个隐藏 P0（H1/H2/H3/H4）已在各自现有 openspec change 中跟踪，tasks.md 需同步状态
- [ ] **Step 6:** 提交文档更新：`git add openspec/changes/adversarial-audit-p0-fixes/ && git commit -m "docs(openspec): complete adversarial-audit-p0-fixes change artifacts"`
