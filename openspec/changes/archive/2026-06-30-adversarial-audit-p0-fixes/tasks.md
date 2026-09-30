## 1. Prompt Assembly O(n²) 修复（最低成本，最高 ROI）

- [x] 1.1 读取 `crates/synthia-context/src/assembler/pipeline.rs` 行 38-48，确认 `trimmed.remove(0)` 在 while 循环中的 O(n²) 反模式
- [x] 1.2 将 `trimmed.remove(0)` 循环改为 `Vec::drain(start..end)` 一次性移除，或维护 `start_index` 而非 mutate Vec
- [x] 1.3 保持 ProtectionZone trim 语义不变：从最旧消息开始移除直到 `total_tokens <= max_tokens`，保留最近 1 条
- [x] 1.4 运行 `cargo test -p synthia-context` 验证 trim 行为不变
- [x] 1.5 运行 `cargo +nightly fmt --all` 与 `cargo clippy -p synthia-context --all-targets --all-features --tests` 确保无警告

## 2. Guardian/E2E Cache Policy 注入（~$19.7K/年节省）

- [x] 2.1 读取 `crates/synthia-guardian/src/review/reviewer.rs` 行 217、313，确认 `cache_policy: None`
- [x] 2.2 读取 `crates/synthia-agent/tests/e2e_llm_test.rs` 行 120、200，确认 `cache_policy: None`
- [x] 2.3 将 reviewer.rs 的 `cache_policy: None` 改为 `Some(CachePolicy::default())`
- [x] 2.4 将 e2e_llm_test.rs 的 `cache_policy: None` 改为 `Some(CachePolicy::default())`
- [x] 2.5 运行 `cargo test -p synthia-guardian` 与 `cargo test -p synthia-agent --test e2e_llm_test` 验证
- [x] 2.6 运行 `cargo +nightly fmt --all` 与 `cargo clippy` 确保无警告

## 3. Bash 沙箱接入（U1 安全单点失效修复）

- [x] 3.1 读取 `crates/synthia-tool-orchestrator/src/lib.rs` 行 731，确认 `ToolAdapter::execute` 形参 `_sandbox_attempt`（下划线 = 显式忽略）
- [x] 3.2 移除 `_sandbox_attempt` 的下划线前缀，形参命名为 `sandbox_attempt`
- [x] 3.3 读取 `crates/synthia-tool-bash/src/bash_tool/executor.rs` 行 32，确认 `Command::new("bash").arg("-c").arg(command).output()` 从不调用 `SandboxAttempt::wrap()`
- [x] 3.4 在 bash executor 的 `execute_command` 中，执行前调用 `sandbox_attempt.wrap(&mut command)`
- [x] 3.5 在 `crates/synthia-tool-bash/src/bash_tool/trait_impl.rs` 行 108-113 的后台 spawn 路径同样调用 `SandboxAttempt::wrap`
- [x] 3.6 处理沙箱 unavailable 情况：按 `SandboxPolicy::on_unavailable()` 决策，Standard → Deny 而非裸跑
- [x] 3.7 新增测试"沙箱 unavailable 时 bash Deny"：mock 沙箱 unavailable，断言 bash 工具返回 Deny 而非执行
- [x] 3.8 新增测试"沙箱 available 时 wrap 被调用"：mock 沙箱 available，断言 `SandboxAttempt::wrap` 被调用
- [x] 3.9 运行 `cargo test -p synthia-tool-bash` 与 `cargo test -p synthia-tool-orchestrator` 验证
- [x] 3.10 运行 `cargo +nightly fmt --all` 与 `cargo clippy` 确保无警告

## 4. 文件工具路径校验改为 Workspace 包含（U2 安全）

- [x] 4.1 读取 `crates/synthia-permission/src/checker/checker.rs` 行 229-240，确认 `check_path_traversal` 仅 `path.contains("../")`
- [x] 4.2 参考 `codex/codex-rs/core/src/safety.rs` 行 138-193 的 `normalize()` + `can_write_path_with_cwd()` 纯路径运算实现
- [x] 4.3 实现 Rust 版 `normalize_path(path: &Path) -> PathBuf`：折叠 `.` 和 `..` 段，不调 `canonicalize()`
- [x] 4.4 实现 `is_path_in_workspace(normalized: &Path, workspace_root: &Path) -> bool`：前缀包含判定
- [x] 4.5 重写 `check_path_traversal`：用 `normalize_path` + `is_path_in_workspace` 替代 `../` 子串检查
- [x] 4.6 新增测试"绝对路径越界 Deny"：`read_file("/etc/passwd")` 与 `read_file("/home/victim/.ssh/id_rsa")` 必须 Deny
- [x] 4.7 新增测试"相对遍历 Deny"：`read_file("../../../etc/passwd")` 必须 Deny
- [x] 4.8 新增测试"workspace 内路径 Allow"：`read_file("src/main.rs")` 必须 Allow
- [x] 4.9 新增测试"normalize 折叠 dot 段"：`workspace/src/../src/main.rs` 折叠为 `workspace/src/main.rs`
- [x] 4.10 确认 `normalize_path` 不调 `canonicalize()`（防 TOCTOU）
- [x] 4.11 运行 `cargo test -p synthia-permission` 验证
- [x] 4.12 运行 `cargo +nightly fmt --all` 与 `cargo clippy` 确保无警告

## 5. PrefixTracker Hash 范围扩展（可观测性修正）

- [x] 5.1 读取 `crates/synthia-context/src/prefix_tracker/tracker.rs` 行 90-97，确认 `record_pre` 只 hash `system_bytes`
- [x] 5.2 修改 `record_pre` 签名：增加 `tools_schema_bytes: &[u8]` 与 `messages_prefix_bytes: &[u8]` 参数
- [x] 5.3 修改 `compute_hash_bytes`：按 `system_bytes || tools_schema_bytes || messages_prefix_bytes` 顺序拼接后 hash
- [x] 5.4 更新所有 `record_pre` 调用点，传入 tools 与 messages 前缀 bytes
- [x] 5.5 新增测试"tools 变更检测"：system 不变但 tools 变 → hash 改变 → stability_ratio 下降
- [x] 5.6 新增测试"messages 前缀变更检测"：system 与 tools 不变但 messages 前缀变 → hash 改变
- [x] 5.7 新增测试"稳定 prefix 准确报告"：三者均不变 → stability_ratio 接近 1.0
- [x] 5.8 评估 `recent_window` 滑动窗口默认值是否需要调整（hash 范围扩大后稳定性分布可能变化）
- [x] 5.9 运行 `cargo test -p synthia-context` 验证
- [x] 5.10 运行 `cargo +nightly fmt --all` 与 `cargo clippy` 确保无警告

## 6. 集成验证

- [x] 6.1 运行 `cargo test --all` 确保全部测试通过
- [x] 6.2 运行 `cargo +nightly fmt --all` 确保格式一致
- [x] 6.3 运行 `cargo clippy --all-targets --all-features --tests --all` 确保无警告
- [x] 6.4 运行 `openspec validate adversarial-audit-p0-fixes --strict` 确保变更规格有效
- [x] 6.5 更新 `project_memory.md`：将原 P0 清单替换为本变更的修订后优先级，标记 3/4 原 P0 已完成
