<!--
Raw capture of brainstorming output.

本档原样捕捉 brainstorming skill 的产出，不强制结构。
基于多专家对抗性分析（架构、性能可靠性、安全生产化三个视角）的决策日志。
-->

# P0 可靠性与安全性修复 - 决策日志

## 背景

基于对 synthia、opencode、codex 三个 AI agent 代码库的深度对抗性审查，识别出 5 个 P0 级确定性风险。这些风险不是概率性问题，而是**确定性**的——在生产中必然发生，且已有代码证据。

本次 change 聚焦于 P0 修复，高优先级架构差距和生产化能力作为后续阶段处理。

## P0 风险清单（按修复优先级）

### P0-1: bash 工具不杀进程组（资源泄漏）

**证据**：`crates/synthia-agent/src/tools/builtins/system_tools.rs:105-111`

```rust
let output = tokio::select! {
    biased;
    _ = cancellation_token.cancelled() => { return Err(ToolExecutionError::Cancelled); }
    result = tokio::time::timeout(timeout, cmd.output()) => result,
};
```

超时或取消时 future 被 drop，但 tokio Command 默认不 kill 子进程（无 `kill_on_drop`），且无 `setsid`/`setpgid`/进程组杀。孙子进程（如 `bash -c "sleep 1000 &"`）变孤儿被 init 收养。

**对比**：codex 有 `kill_child_process_group` + `IO_DRAIN_TIMEOUT_MS=2s`；opencode 有 `forceKillAfter: Duration.seconds(3)`。

**后果**：每次 bash 超时泄漏一个进程，长跑 server 必崩。

### P0-2: L5 ResetCoordinator 半实现（可靠性陷阱）

**证据**：`crates/synthia-agent/src/error_recovery/reset/coordinator.rs:84-90`

`determine_scope` 根据 `consecutive_errors` 选择 scope：0-5→Conversation，6-10→ToolState，10+→Full。但 `ToolState` 和 `Full` 两个分支返回 `"not yet implemented"`。

**后果**：连续错误超过 5 次后，系统进入"30s 冷却→FailFast"循环。而 6-10 次错误恰恰是最需要"部分重置"来恢复的场景。

### P0-3: 无全局 wall-clock 超时

**证据**：`crates/synthia-agent/src/loop_context.rs:133-135` `should_stop` 仅检查 `max_iterations`（默认 20），结合 `MAX_TIMEOUT_SECS=3600`，理论上一个 session 可跑 20 小时。

**对比**：opencode 有 `DEFAULT_TIMEOUT_MS=2分钟`；codex 有 Guardian 90s 超时。

### P0-4: Guardian 快速路径空 transcript bug（安全漏洞）

**证据**：`crates/synthia-guardian/src/review/reviewer.rs:101-105`

```rust
let review_prompt = build_review_prompt(
    &collect_transcript_entries(&[]),  // 传入空切片！
    &action_json,
    None,
);
```

Guardian `check()` 快速路径传入空切片，在**无任何对话上下文**下审查动作。藏在早期对话轮次中的 prompt injection 不会被纳入审查。

**后果**：攻击者在第 1 轮注入隐藏指令，第 N 轮触发恶意工具调用时，Guardian 快速路径看不到注入上下文，可能给出偏低 risk_score → Allow。

### P0-5: Guardian 占位符 request bug

**证据**：`crates/synthia-guardian/src/review/reviewer.rs:164-167`

```rust
Ok(self.make_guardian_decision(
    assessment,
    &ApprovalRequest::shell("temp", vec![], "/", None),  // 占位符！
))
```

`make_guardian_decision` 在 NeedUserConfirm 分支用占位符 request，**丢弃了实际 request 上下文**。用户收到的确认请求可能指向错误动作。

## 设计决策链

### Q1: P0 修复的范围如何界定？

**决策**：聚焦于 5 个确定性风险，不扩展到高优先级架构差距（如 apply_patch 4 级模糊匹配、工具并发执行等）。理由：
1. P0 是确定性风险，必须立即修复
2. 架构差距是"改进"而非"修复"，可后续处理
3. 控制 change 范围，降低回归风险

### Q2: bash 进程组杀的实现方式？

**备选方案**：
- A: `.kill_on_drop(true)` — 最简单，但只杀直接子进程，不杀孙子进程
- B: `process_group(0)` + `killpg` — Unix 标准做法，杀整个进程组
- C: `setsid` + `killpg` — 与 B 类似，但更显式

**决策**：采用 B（`process_group(0)` + `killpg`）。理由：
1. codex 和 opencode 都用进程组杀
2. `kill_on_drop` 不够，孙子进程会变孤儿
3. `process_group(0)` 是 tokio::process 的原生支持（`Command::process_group`）

**实现要点**：
- `Command::new("bash").process_group(0)` — 创建新进程组
- 超时/取消时 `killpg(pgid, Signal::SIGTERM)` → 等 3s → `killpg(pgid, Signal::SIGKILL)`
- IO 排空 2s（借鉴 codex 的 `IO_DRAIN_TIMEOUT_MS`）

### Q3: L5 Reset 未实现 scope 如何处理？

**备选方案**：
- A: 实现 ToolState 和 Full reset — 工作量大，需要定义"ToolState"和"Full"的语义
- B: 未实现时回退到 Conversation — 最小改动，消除冷却死循环
- C: 移除 determine_scope，统一用 Conversation — 简化设计

**决策**：采用 B（未实现时回退到 Conversation）。理由：
1. 最小改动，降低回归风险
2. 消除"6+错误进入冷却死循环"的确定性风险
3. 保留 determine_scope 的设计，未来可实现 ToolState/Full
4. 添加 warning 日志，标记"回退到 Conversation"

### Q4: 全局 wall-clock 超时放在哪一层？

**备选方案**：
- A: 放在 `should_stop` 中检查 — 与 max_iterations 并列
- B: 放在 `SessionController` 层 — 与 idle_timeout 并列
- C: 放在 `Agent::run_stream` 入口 — 最外层

**决策**：采用 A（放在 `should_stop` 中检查）。理由：
1. `should_stop` 是循环的退出条件，最自然的位置
2. 与 max_iterations 并列，语义一致
3. 可配置（通过 AgentConfig）

**默认值**：30 分钟（session 级）。理由：
1. opencode 的工具超时是 2 分钟，但 session 级更长
2. 30 分钟足够完成大多数任务
3. 可通过配置覆盖

### Q5: Guardian 空 transcript bug 如何修复？

**备选方案**：
- A: `check()` 接收并传入实际 conversation — 最彻底，但改签名
- B: `check()` 从 session 读取最近 N 轮 conversation — 不改签名，但需要 session 访问
- C: 文档化"快速路径仅做无上下文启发式判断"并降级决策权重 — 不修 bug，降低影响

**决策**：采用 A（`check()` 接收并传入实际 conversation）。理由：
1. bug 的根源是"快速路径缺乏上下文"，必须修复
2. 改签名是正确的，`check()` 本应需要 conversation
3. 借鉴 codex 的 transcript 预算（消息 10K tokens、工具 10K tokens、单条 2K/1K tokens），防止注入撑爆

**实现要点**：
- `check()` 签名增加 `conversation: &[Message]` 参数
- 传入最近 N 轮 conversation（有 token 预算限制）
- 对齐 `review()` 的实现

### Q6: Guardian 占位符 request bug 如何修复？

**决策**：`make_guardian_decision` 接收实际 `request` 参数而非占位符。理由：
1. 占位符是明显的 bug，用户确认请求指向错误动作
2. 修复简单：将 `request` 透传到 `make_guardian_decision`

## 设计取捨

### 取捨 1: 修复 vs 重构

**决策**：本次 change 只做修复，不做重构。理由：
1. P0 是确定性风险，必须快速修复
2. 重构（如统一 PermissionChecker/CommandBlacklist）是高优先级，但不是 P0
3. 控制 change 范围，降低回归风险

### 取捨 2: 最小改动 vs 完美实现

**决策**：采用最小改动。理由：
1. P0 修复的目标是"消除确定性风险"，不是"完美实现"
2. L5 Reset 采用"回退到 Conversation"而非"实现 ToolState/Full"
3. Guardian 修复 bug，不重构 transcript 预算（预算作为后续改进）

### 取捨 3: 测试策略

**决策**：每个修复必须有测试。理由：
1. P0 修复不能引入回归
2. Guardian bug 本应被测试发现
3. 进程组杀需要集成测试验证

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 进程组杀可能误杀父进程 | `process_group(0)` 创建新进程组，不影响父进程 |
| L5 回退到 Conversation 可能丢失工具状态 | 添加 warning 日志；未来实现 ToolState |
| 全局超时可能中断长任务 | 默认 30 分钟，可配置；超时前发 warning |
| Guardian check() 改签名影响调用方 | 搜索所有调用点，同步更新 |
