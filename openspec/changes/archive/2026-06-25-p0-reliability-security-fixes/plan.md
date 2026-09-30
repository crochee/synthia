# P0 可靠性与安全性修复 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 5 个 P0 级确定性风险：bash 进程组泄漏、L5 Reset 冷却死循环、无全局超时、Guardian 空 transcript bug、Guardian 占位符 request bug。

**Architecture:** 最小改动修复，不重构。每个修复独立可 revert。遵循 TDD：先写失败测试，再修复，再验证。

**Tech Stack:** Rust, tokio, nix/libc (Unix 信号), tracing (日志), tokio::process (进程组)

---

## Task 1: bash 进程组杀（P0-1）

**Files:**
- Modify: `crates/synthia-agent/src/tools/builtins/system_tools.rs:98-127`
- Modify: `crates/synthia-agent/Cargo.toml` (添加 nix 依赖)
- Test: `crates/synthia-agent/src/tools/builtins/system_tools.rs` (同文件测试)

- [ ] **Step 1: 添加 nix 依赖**

修改 `crates/synthia-agent/Cargo.toml`，在 `[dependencies]` 添加：
```toml
nix = { version = "0.29", features = ["signal", "process"] }
```
运行 `cargo check -p synthia-agent` 验证依赖解析。

- [ ] **Step 2: 编写失败测试 — 进程组杀验证**

在 `system_tools.rs` 测试模块添加集成测试（标记 `#[ignore]` 避免 CI 跑）：
```rust
#[tokio::test]
#[ignore = "requires Unix process group support"]
async fn test_bash_timeout_kills_process_group() {
    // 启动一个会 spawn 孙子进程的 bash 命令，超时设为 2s
    // 命令: bash -c "sleep 100 &" (孙子进程 sleep 100)
    // 超时后验证 sleep 100 进程已被杀
    // 用 pgrep -f "sleep 100" 检查（应返回空）
}
```

- [ ] **Step 3: 修改 bash 执行逻辑 — 使用 spawn + process_group**

将 `system_tools.rs:98-111` 的：
```rust
let mut cmd = Command::new("bash");
cmd.arg("-c").arg(command);
sandbox_attempt.wrap(&mut cmd)...;
let output = tokio::select! {
    biased;
    _ = cancellation_token.cancelled() => { return Err(ToolExecutionError::Cancelled); }
    result = tokio::time::timeout(timeout, cmd.output()) => result,
};
```

改为：
```rust
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;

let mut cmd = Command::new("bash");
cmd.arg("-c").arg(command);
cmd.process_group(0);  // 创建新进程组
cmd.stdout(Stdio::piped());
cmd.stderr(Stdio::piped());
cmd.stdin(Stdio::null());

sandbox_attempt.wrap(&mut cmd)...;

let mut child = cmd.spawn().map_err(|e| ...)?;
let child_pid = child.id() as i32;

let output = tokio::select! {
    biased;
    _ = cancellation_token.cancelled() => {
        // 杀整个进程组
        kill_process_group(child_pid);
        drain_io(&mut child, Duration::from_secs(2)).await;
        return Err(ToolExecutionError::Cancelled);
    }
    result = tokio::time::timeout(timeout, child.wait_with_output()) => result,
};

match output {
    Ok(output) => { /* 原有处理 */ }
    Err(_) => {
        // 超时：杀进程组
        kill_process_group(child_pid);
        drain_io(&mut child, Duration::from_secs(2)).await;
        return Err(ToolExecutionError::Permanent(format!(
            "Command timed out after {} seconds", timeout_secs
        )));
    }
}
```

- [ ] **Step 4: 实现 kill_process_group 辅助函数**

在 `system_tools.rs` 添加：
```rust
fn kill_process_group(child_pid: i32) {
    let pgid = Pid::from_raw(child_pid);
    // 先 SIGTERM
    let _ = killpg(pgid, Signal::SIGTERM);
    // 等 3s
    std::thread::sleep(Duration::from_secs(3));
    // 再 SIGKILL
    let _ = killpg(pgid, Signal::SIGKILL);
}

async fn drain_io(child: &mut tokio::process::Child, timeout: Duration) {
    if let Some(stdout) = child.stdout.take() {
        let _ = tokio::time::timeout(timeout, tokio::io::read_to_end(stdout, &mut Vec::new())).await;
    }
    if let Some(stderr) = child.stderr.take() {
        let _ = tokio::time::timeout(timeout, tokio::io::read_to_end(stderr, &mut Vec::new())).await;
    }
    let _ = child.wait().await;
}
```

- [ ] **Step 5: 运行测试验证**

```bash
cargo test -p synthia-agent -- --ignored test_bash_timeout_kills_process_group
cargo test -p synthia-agent system_tools
```

- [ ] **Step 6: 格式化与 clippy**

```bash
cargo +nightly fmt --all
cargo clippy -p synthia-agent --all-targets --all-features --tests
```

- [ ] **Step 7: Commit**

```bash
git add crates/synthia-agent/src/tools/builtins/system_tools.rs crates/synthia-agent/Cargo.toml
git commit -m "fix: kill process group on bash timeout to prevent orphan leaks

P0-1: bash 工具超时/取消时不杀子进程，孙子进程变孤儿。
使用 process_group(0) + killpg(SIGTERM→SIGKILL) + IO drain。"
```

---

## Task 2: L5 Reset 回退（P0-2）

**Files:**
- Modify: `crates/synthia-agent/src/error_recovery/reset/coordinator.rs:77-96`
- Test: `crates/synthia-agent/src/error_recovery/reset/tests.rs`

- [ ] **Step 1: 编写失败测试 — ToolState 回退**

在 `tests.rs` 添加：
```rust
#[test]
fn test_toolstate_scope_falls_back_to_conversation() {
    let mut coordinator = ResetCoordinator::new();
    // 模拟 consecutive_errors=7 (ToolState 范围)
    // 调用 execute_reset with scope=ToolState
    // 验证 result.success == true
    // 验证 result.scope == Conversation
}

#[test]
fn test_full_scope_falls_back_to_conversation() {
    let mut coordinator = ResetCoordinator::new();
    // 模拟 consecutive_errors=12 (Full 范围)
    // 验证 result.success == true
    // 验证 result.scope == Conversation
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test -p synthia-agent reset::tests::test_toolstate
```
Expected: FAIL (当前返回 failed)

- [ ] **Step 3: 修改 coordinator.rs — 回退逻辑**

将 `coordinator.rs:77-91` 的：
```rust
let result = match scope {
    ResetScope::Conversation => Self::execute_conversation_reset(ctx, loop_detector, steering, recovery),
    ResetScope::ToolState => ResetResult::failed(scope, "ToolState reset not yet implemented"),
    ResetScope::Full => ResetResult::failed(scope, "Full reset not yet implemented"),
};
```

改为：
```rust
let result = match scope {
    ResetScope::Conversation => Self::execute_conversation_reset(ctx, loop_detector, steering, recovery),
    ResetScope::ToolState => {
        tracing::warn!("ToolState reset not implemented, falling back to Conversation");
        Self::execute_conversation_reset(ctx, loop_detector, steering, recovery)
    }
    ResetScope::Full => {
        tracing::warn!("Full reset not implemented, falling back to Conversation");
        Self::execute_conversation_reset(ctx, loop_detector, steering, recovery)
    }
};
```

- [ ] **Step 4: 运行测试验证通过**

```bash
cargo test -p synthia-agent reset::tests
```
Expected: PASS

- [ ] **Step 5: 格式化与 clippy**

```bash
cargo +nightly fmt --all
cargo clippy -p synthia-agent --all-targets --all-features --tests
```

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/error_recovery/reset/coordinator.rs crates/synthia-agent/src/error_recovery/reset/tests.rs
git commit -m "fix: fall back to Conversation reset for unimplemented scopes

P0-2: ToolState/Full reset 未实现时返回 failed，触发 30s 冷却死循环。
改为回退到 Conversation 并 emit warning。"
```

---

## Task 3: 全局 wall-clock 超时（P0-3）

**Files:**
- Modify: `crates/synthia-agent/src/loop_context.rs:9-25,133-135`
- Modify: `crates/synthia-agent/src/agent.rs` (AgentConfig)
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` (传递配置)
- Test: `crates/synthia-agent/src/loop_context.rs` (同文件测试)

- [ ] **Step 1: 编写失败测试 — 超时触发 should_stop**

在 `loop_context.rs` 测试模块添加：
```rust
#[test]
fn test_should_stop_wall_clock_timeout() {
    let mut ctx = LoopContext::new("s".into(), SpanContext::new("s"));
    ctx.session_start = Some(Instant::now() - Duration::from_secs(100));
    // wall_clock_timeout = 50s → 已超时
    assert!(ctx.should_stop_with_timeout(20, Some(Duration::from_secs(50))));
}

#[test]
fn test_should_stop_no_wall_clock_timeout() {
    let mut ctx = LoopContext::new("s".into(), SpanContext::new("s"));
    ctx.session_start = Some(Instant::now());
    ctx.iteration = 5;
    // wall_clock_timeout = None → 不检查时间，只检查 iteration
    assert!(ctx.should_stop_with_timeout(5, None));
    assert!(!ctx.should_stop_with_timeout(20, None));
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test -p synthia-agent loop_context::tests::test_should_stop_wall_clock
```
Expected: FAIL (方法不存在)

- [ ] **Step 3: 在 LoopContext 添加 session_start 字段**

在 `loop_context.rs` struct 定义中添加：
```rust
pub struct LoopContext {
    // ... 现有字段 ...
    pub session_start: Option<std::time::Instant>,
}
```

在 `LoopContext::new` 中初始化：
```rust
session_start: Some(std::time::Instant::now()),
```

- [ ] **Step 4: 实现 should_stop_with_timeout 方法**

在 `loop_context.rs` 添加：
```rust
pub fn should_stop_with_timeout(
    &self,
    max_iterations: usize,
    wall_clock_timeout: Option<Duration>,
) -> bool {
    if self.end_reason.is_some() || self.iteration >= max_iterations {
        return true;
    }
    if let (Some(start), Some(timeout)) = (self.session_start, wall_clock_timeout) {
        if timeout > Duration::ZERO && start.elapsed() >= timeout {
            return true;
        }
    }
    false
}
```

保留原 `should_stop` 方法，内部调用 `should_stop_with_timeout(max_iterations, None)`。

- [ ] **Step 5: 在 AgentConfig 添加 session_wall_clock_timeout**

在 `agent.rs` 的 `AgentConfig` struct 添加：
```rust
pub session_wall_clock_timeout: Option<Duration>,
```

默认值在 impl Default 或 builder 中设为 `Some(Duration::from_secs(1800))`。

- [ ] **Step 6: 在 main_loop.rs 传递超时配置**

修改 `should_stop` 调用点，传入 `config.session_wall_clock_timeout`。

- [ ] **Step 7: 运行测试验证通过**

```bash
cargo test -p synthia-agent loop_context::tests
```
Expected: PASS

- [ ] **Step 8: 格式化与 clippy**

```bash
cargo +nightly fmt --all
cargo clippy -p synthia-agent --all-targets --all-features --tests
```

- [ ] **Step 9: Commit**

```bash
git add crates/synthia-agent/src/loop_context.rs crates/synthia-agent/src/agent.rs crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs
git commit -m "fix: add session wall-clock timeout to prevent runaway sessions

P0-3: should_stop 仅检查 max_iterations，无时间限制。
增加 session_wall_clock_timeout（默认 30 分钟），可配置。"
```

---

## Task 4: Guardian 空 transcript 修复（P0-4）

**Files:**
- Modify: `crates/synthia-guardian/src/review/reviewer.rs:82-129`
- Test: `crates/synthia-guardian/src/review/tests.rs`

- [ ] **Step 1: 编写失败测试 — check 接收 conversation**

在 `tests.rs` 添加：
```rust
#[tokio::test]
async fn test_check_with_conversation_context() {
    let config = GuardianConfig::default().enabled(true);
    let reviewer = GuardianReviewer::new(config).with_timeout(Duration::from_secs(45));
    let request = ApprovalRequest::shell("id", vec!["ls".to_string()], "/", None);
    let conversation = vec![Message::user("earlier instruction")];
    let router = create_mock_router();
    // 验证不 panic，且返回 GuardianDecision
    let decision = reviewer.check(&request, &conversation, &router).await;
    assert!(matches!(decision, GuardianDecision::Allow | GuardianDecision::Deny { .. } | GuardianDecision::NeedUserConfirm { .. }));
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test -p synthia-guardian test_check_with_conversation
```
Expected: FAIL (签名不匹配)

- [ ] **Step 3: 修改 check 签名，增加 conversation 参数**

将 `reviewer.rs:82-86` 的：
```rust
pub async fn check(
    &self,
    request: &ApprovalRequest,
    router: &Arc<dyn ModelRouter>,
) -> GuardianDecision {
```

改为：
```rust
pub async fn check(
    &self,
    request: &ApprovalRequest,
    conversation: &[Message],
    router: &Arc<dyn ModelRouter>,
) -> GuardianDecision {
```

- [ ] **Step 4: 修复空 transcript — 传入实际 conversation**

将 `reviewer.rs:101-105` 的：
```rust
let review_prompt = build_review_prompt(
    &collect_transcript_entries(&[]),
    &action_json,
    None,
);
```

改为：
```rust
let review_prompt = build_review_prompt(
    &collect_transcript_entries(conversation),
    &action_json,
    None,
);
```

- [ ] **Step 5: 更新所有 check 调用方**

搜索所有 `reviewer.check(` 调用，添加 conversation 参数：
```bash
grep -rn "\.check(" crates/synthia-guardian/src/review/
```
更新 `tests.rs` 中的调用。

- [ ] **Step 6: 运行测试验证通过**

```bash
cargo test -p synthia-guardian
```
Expected: PASS

- [ ] **Step 7: 格式化与 clippy**

```bash
cargo +nightly fmt --all
cargo clippy -p synthia-guardian --all-targets --all-features --tests
```

- [ ] **Step 8: Commit**

```bash
git add crates/synthia-guardian/src/review/reviewer.rs crates/synthia-guardian/src/review/tests.rs
git commit -m "fix: pass conversation context to Guardian check fast path

P0-4: check() 传入空 transcript，跨轮次 prompt injection 防护失效。
增加 conversation 参数，对齐 review() 实现。"
```

---

## Task 5: Guardian 占位符 request 修复（P0-5）

**Files:**
- Modify: `crates/synthia-guardian/src/review/reviewer.rs:131-168`

- [ ] **Step 1: 编写失败测试 — NeedUserConfirm 使用实际 request**

在 `tests.rs` 添加：
```rust
#[tokio::test]
async fn test_check_need_user_confirm_uses_actual_request() {
    // 构造一个 risk_score 在 50-80 之间的场景
    // 验证 NeedUserConfirm 的 request 字段是实际 request 而非 "temp"
    let config = GuardianConfig::default().enabled(true);
    let reviewer = GuardianReviewer::new(config).with_timeout(Duration::from_secs(45));
    let request = ApprovalRequest::shell("real-id", vec!["rm -rf /tmp/test".to_string()], "/tmp", None);
    let conversation = vec![];
    let router = create_mock_router_returning_medium_risk();
    let decision = reviewer.check(&request, &conversation, &router).await;
    if let GuardianDecision::NeedUserConfirm { req, .. } = decision {
        assert_ne!(req.id, "temp");  // 不是占位符
        assert_eq!(req.id, "real-id");  // 是实际 request
    }
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test -p synthia-guardian test_check_need_user_confirm_uses_actual_request
```
Expected: FAIL (当前用占位符)

- [ ] **Step 3: 修改 call_llm_internal 接收 request 参数**

将 `reviewer.rs:131-168` 的：
```rust
async fn call_llm_internal(
    &self,
    prompt: &str,
    router: &Arc<dyn ModelRouter>,
) -> anyhow::Result<GuardianDecision> {
    // ...
    Ok(self.make_guardian_decision(
        assessment,
        &ApprovalRequest::shell("temp", vec![], "/", None),  // 占位符
    ))
}
```

改为：
```rust
async fn call_llm_internal(
    &self,
    prompt: &str,
    request: &ApprovalRequest,
    router: &Arc<dyn ModelRouter>,
) -> anyhow::Result<GuardianDecision> {
    // ...
    Ok(self.make_guardian_decision(assessment, request))
}
```

- [ ] **Step 4: 更新 check 中 call_llm_internal 的调用**

在 `check` 方法中：
```rust
let result = timeout(
    self.timeout,
    self.call_llm_internal(&review_prompt, request, router),
).await;
```

- [ ] **Step 5: 运行测试验证通过**

```bash
cargo test -p synthia-guardian
```
Expected: PASS

- [ ] **Step 6: 格式化与 clippy**

```bash
cargo +nightly fmt --all
cargo clippy -p synthia-guardian --all-targets --all-features --tests
```

- [ ] **Step 7: Commit**

```bash
git add crates/synthia-guardian/src/review/reviewer.rs crates/synthia-guardian/src/review/tests.rs
git commit -m "fix: pass actual request to make_guardian_decision

P0-5: call_llm_internal 用占位符 ApprovalRequest::shell(\"temp\",...)，
用户确认指向错误动作。改为透传实际 request。"
```

---

## Task 6: 最终验证

- [ ] **Step 1: 全量格式化**

```bash
cargo +nightly fmt --all
```

- [ ] **Step 2: 全量 clippy**

```bash
cargo clippy --all-targets --all-features --tests --all
```
Expected: 无警告

- [ ] **Step 3: 全量测试**

```bash
cargo test --all
```
Expected: 全部通过

- [ ] **Step 4: 全量编译**

```bash
cargo build --all
```
Expected: 编译通过

- [ ] **Step 5: 最终 commit（如有格式化修正）**

```bash
git add -A
git commit -m "chore: format and clippy fixes for P0 reliability fixes"
```
