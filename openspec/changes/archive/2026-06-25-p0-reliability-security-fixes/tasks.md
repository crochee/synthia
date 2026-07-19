## 1. bash 进程组杀（P0-1）

- [x] 1.1 在 `system_tools.rs` 中为 `Command::new("bash")` 添加 `.process_group(0)`，创建新进程组
- [x] 1.2 将 `cmd.output()` 改为 `cmd.spawn()` + 手动 wait，获取 child pid 用于 killpg
- [x] 1.3 实现进程组终止函数：`killpg(SIGTERM)` → 3s → `killpg(SIGKILL)`，使用 `nix::sys::signal` 或 `libc::killpg`
- [x] 1.4 在 timeout 和 cancellation 分支调用进程组终止函数
- [x] 1.5 实现 IO 排空逻辑：kill 后 drain pipe output 2s（`IO_DRAIN_TIMEOUT`）
- [x] 1.6 添加 `nix` 或 `libc` 依赖到 `synthia-agent` 的 Cargo.toml（如尚未存在）
- [x] 1.7 编写单元测试：验证 `process_group(0)` 被调用、验证 killpg 逻辑（mock 或集成测试）
- [x] 1.8 编写集成测试：执行 `bash -c "sleep 1000 &"`，超时后验证无孤儿进程（`pgrep -f "sleep 1000"` 返回空）

## 2. L5 Reset 回退（P0-2）

- [x] 2.1 在 `coordinator.rs` 的 `execute_reset` 中，将 `ResetScope::ToolState` 分支改为调用 `execute_conversation_reset` 并 emit warning log
- [x] 2.2 将 `ResetScope::Full` 分支改为调用 `execute_conversation_reset` 并 emit warning log
- [x] 2.3 确保回退成功时不触发 `start_cooldown()`（当前 `if !result.success` 逻辑已满足，验证即可）
- [x] 2.4 编写单元测试：`consecutive_errors=7` 时 reset 成功（不触发 cooldown）
- [x] 2.5 编写单元测试：`consecutive_errors=12` 时 reset 成功（不触发 cooldown）
- [x] 2.6 编写单元测试：验证 warning log 被输出（使用 `tracing` test capture）

## 3. 全局 wall-clock 超时（P0-3）

- [x] 3.1 在 `LoopContext` struct 中添加 `session_start: Option<Instant>` 字段
- [x] 3.2 在 `LoopContext::new` 中初始化 `session_start = Some(Instant::now())`
- [x] 3.3 在 `AgentConfig` 中添加 `session_wall_clock_timeout: Option<Duration>` 字段，默认 `Some(Duration::from_secs(1800))`（30 分钟）
- [x] 3.4 修改 `should_stop` 签名增加 `wall_clock_timeout: Option<Duration>` 参数，或改为接收 `&AgentConfig`
- [x] 3.5 在 `should_stop` 中检查 `session_start.elapsed() >= wall_clock_timeout`，超时返回 true
- [x] 3.6 在 `main_loop.rs` 的循环条件中传入 wall_clock_timeout 配置
- [x] 3.7 实现 80% 超时警告事件：在 `should_stop` 或循环中检查是否达到 80%，emit `Warning` 事件（仅一次）
- [x] 3.8 添加 `SessionEndReason::Timeout` 变体（如尚不存在）
- [x] 3.9 编写单元测试：超时后 `should_stop` 返回 true
- [x] 3.10 编写单元测试：`wall_clock_timeout=None` 时不检查时间
- [x] 3.11 编写单元测试：80% 超时时发出 warning 事件

## 4. Guardian 空 transcript 修复（P0-4）

- [x] 4.1 修改 `GuardianReviewer::check` 签名，增加 `conversation: &[Message]` 参数
- [x] 4.2 将 `collect_transcript_entries(&[])` 改为 `collect_transcript_entries(conversation)`
- [x] 4.3 更新 `review/tests.rs` 中所有 `check()` 调用，传入测试 conversation
- [x] 4.4 编写测试：`check()` 传入非空 conversation 时，review prompt 包含 transcript 条目
- [x] 4.5 编写测试：`check()` 传入空 conversation 时，review prompt 包含空 transcript（不 panic）

## 5. Guardian 占位符 request 修复（P0-5）

- [x] 5.1 修改 `call_llm_internal` 签名，增加 `request: &ApprovalRequest` 参数
- [x] 5.2 将 `make_guardian_decision` 调用中的占位符 `ApprovalRequest::shell("temp", ...)` 替换为实际 `request`
- [x] 5.3 更新 `check` 方法中 `call_llm_internal` 的调用，传入 `request`
- [x] 5.4 更新所有 `call_llm_internal` 的调用方（搜索确认）
- [x] 5.5 编写测试：`make_guardian_decision` 返回 `NeedUserConfirm` 时，`request` 字段是实际 request 而非占位符

## 6. 验证与格式化

- [x] 6.1 运行 `cargo +nightly fmt --all` 格式化所有修改的文件
- [x] 6.2 运行 `cargo clippy --all-targets --all-features --tests --all` 修复所有警告
- [x] 6.3 运行 `cargo test -p synthia-agent` 确保所有测试通过
- [x] 6.4 运行 `cargo test -p synthia-guardian` 确保所有测试通过
- [x] 6.5 运行 `cargo test -p synthia-context` 确保所有测试通过
- [x] 6.6 运行 `cargo build --all` 确保编译通过
