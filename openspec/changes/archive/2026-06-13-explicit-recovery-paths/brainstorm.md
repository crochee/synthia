<!--
Raw capture of brainstorming for "explicit-recovery-paths".

本檔原樣捕捉 brainstorming 的探索過程與結論。
不強制結構 — 採 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨 → 開放問題）。

下游 artifact (proposal.md, design.md, tasks.md) 從本檔萃取並重新整理。
-->

# Brainstorm: 显式化 L1-L5 恢复路径

> Date: 2026-06-13
> Author: Assistant (driven by user task: "Recovery paths 显式化")
> Schema: superpowers-bridge

---

## 0. 背景（来自上轮 archive/2026-06-12-error-recovery-cascade/retrospective.md 的关键发现）

上轮 `error-recovery-cascade` change 完成了 L1-L5 **实现**，但 retrospective §2 明确指出：

> 🟡 **L4 集成路径在 builder.rs 中只 yield tracing 而非真实压缩**:
> recovery_cascade L4 升级返回 `Escalate` 而非真正执行 `compact_with_fallback()` —
> 由 Phase 4 简化避免侵入 builder.rs；Phase 5 中已部分修复，
> **但 builder.rs 的实际端到端 wiring 仍有小幅差距**

即：**recovery cascade 是"已实现的死代码"**。`run_recovery_cascade` 在 `recovery_cascade.rs` 中完整存在（13 个单元测试通过），但 `stream_builder/builder.rs` 从不调用它。

---

## 1. 现状调查（已完成）

### 1.1 L1-L5 实现 vs 调用矩阵

| Level | 名称 | 实现位置 | 是否在 builder.rs 中调用？ |
|-------|------|---------|-----------------------------|
| L1 | Truncate | `synthia_context::truncate::truncate_messages` | ⚠️ Partial — `StepSample::execute` 在 LLM 调用前做工具消息截断，**但错误路径不触发** |
| L2 | Retry | `ErrorRecoveryCoordinator::handle_error` + `RetryStrategy` | ⚠️ Partial — 只在 LLM sampling 错误时调用，**工具错误不触发** |
| L3 | Fallback | `recovery_cascade::try_l3_fallback` + `FallbackProvider` | ❌ **从不调用** |
| L4 | Auto-Compact | `recovery_cascade::try_l4_compact` (调用 `compact_with_fallback`) | ❌ **从不调用** |
| L5 | Reset | `ResetCoordinator::execute` | ❌ **从不调用** |

### 1.2 工具执行错误的现状

`builder.rs:531-541`：

```rust
let tool_results = match steps.tool_execute.execute(&ctx, tool_calls_to_execute).await {
    Ok(results) => results,
    Err(e) => {
        tracing::error!(error = %e, "Tool execution failed");
        vec![ToolResult {
            tool_name: tool_name_on_error,
            output: e.to_string(),
            is_error: true,
        }]
    }
};
```

工具错误被转换为 `is_error: true` 的结果推入上下文，**不触发任何 recovery**。

### 1.3 LLM sampling 错误的现状

`builder.rs:355-383`：

```rust
Err(e) => {
    tracing::error!(error = %e, "Sampling failed");
    use crate::error_recovery::{RecoveryLevel, RecoveryResult};
    let result = steps.recovery.handle_error(&e.to_string(), RecoveryLevel::L2Retry);
    match result {
        RecoveryResult::Recovered => { continue; }
        RecoveryResult::Escalated(next_level) => {
            // Escalated means we should not retry immediately, continue will cause another call
            // Instead, yield error and end gracefully
            yield AgentEvent::LlmError { error: e.to_string() };
            yield AgentEvent::SessionEnded { ... };
            return;
        }
        RecoveryResult::FailFast(reason) => { ... return; }
    }
};
```

只调了 `handle_error(L2Retry)`，**未调 L3-L5 cascade**。`Escalated` 分支直接 `return` 结束 session。

### 1.4 缺少的状态

- `BuilderSteps` 没有 `ConsecutiveFailureTracker`
- `BuilderSteps` 没有 `ResetCoordinator`
- `BuilderSteps` 没有 `CompactionProvider` 的 accessor
- `BuilderSteps` 也没有 `TokenBudget` 的 accessor
- `AgentEvent` 没有 `RecoveryApplied` 变体

### 1.5 开放性观察

1. 现有 `ErrorRecoveryCoordinator::handle_error` 行为：L2 retry 总是先 escalate，但**这跟 retry strategy "max_retries=2" 矛盾** — `consecutive_errors=1, 2` 都返回 `Escalated(L2Retry)`，第三次才进 L3。L2 retry 应该原地重试，而不是 escalate 后 return。
2. `RecoveryResult::Escalated` 在 builder.rs 中被当作"终止 session"信号使用，但语义上是"升级到下一级"。**这两个语义不一致**。

---

## 2. 设计目标（Goals）

**Primary**：将 L1-L5 恢复路径从"实现的死代码"变成"agent loop 错误处理的必经路径"。

**Secondary**：
1. 为每次恢复动作发出可观察事件（`AgentEvent::RecoveryApplied`）
2. 解决 `RecoveryResult::Escalated` 语义不一致问题
3. 工具错误和 LLM 错误都走同一 cascade
4. 不破坏现有 `error-recovery-cascade` specs（它们已经在 archive 中通过）

**Non-goals**：
- ❌ 不重新设计 5 层结构（Truncate/Retry/Fallback/Compact/Reset）
- ❌ 不引入新的 trait 抽象（吸取上轮设计教训：过早抽象 = 浪费代码）
- ❌ 不修改 `CompactResult`、`FallbackStrategy` 等已有数据结构

---

## 3. 决议链（Decision Chain）

### Q1: 工具错误是否应触发 cascade？

**决议**：✅ 是。

**理由**：
- 上轮 archive 的 specs (`specs/tool-fallback`, `specs/auto-compact-on-error`, `specs/session-reset`) 都已经写明工具错误的恢复路径。
- 现状是"spec 承诺，代码不实现" — 这是 dead spec。
- 工具错误（如 bash 命令失败）应该有第二次机会（fallback 描述命令），不应直接污染上下文。

### Q2: 应该在 L1/L2 触发 cascade 还是 L3-L5？

**决议**：L1 和 L2 保持现状（inline 在 sample/tool_execute step），L3-L5 走 cascade。

**理由**：
- L1 truncate 是 sync 写入 `ctx.messages` 的，不适合 cascade 形式（cascade 假设不修改 ctx，直到 L3/L4 真的需要时）。
- L2 retry 是"原地重试"语义（同一 step 再调一次），cascade 是"升级到新级别"语义。
- 这与上轮 `error-recovery-cascade` 的 design 保持一致：
  > "L1 (truncate) and L2 (retry) are handled inline at the tool-execution boundary."

### Q3: 工具错误后的 L1 truncate 应该触发吗？

**决议**：✅ 是，作为 cascade 的入口。

**理由**：
- 工具输出超长（如 cat 一个 100MB 日志）是常见错误。
- 在 L3 fallback 之前先 L1 truncate，能减少 50% 以上的"工具结果太大" 错误。
- 上轮 `specs/tool-output-truncate` 已为此提供 spec。

**实现方式**：当 tool_results 中有 `is_error: true` 且 `output.len() > truncate_cfg.max_bytes` 时，先 truncate 后注入 context。

### Q4: `ErrorRecoveryCoordinator::handle_error` 的 `Escalated` 语义怎么办？

**决议**：保留 `Escalated(RecoveryLevel)`，但 builder.rs 中**不再**把它当终止信号。新的处理：

- L1 入口：`handle_error(L1Truncate)` → truncate + continue
- L2 入口：`handle_error(L2Retry)` → 如果 `should_retry` → 原地重试；否则 escalate 到 L3
- L3-L5：通过 `run_recovery_cascade` 统一处理

**理由**：
- 现有 `ErrorRecoveryCoordinator::handle_error` 的逻辑是正确的（按级 escalate），问题是 builder.rs 错误地终止了 session。
- 修复 consumer 行为，不动 coordinator 行为。

### Q5: 工具错误 vs LLM 错误是否走同一个 cascade？

**决议**：✅ 是，复用 `run_recovery_cascade`。

**理由**：
- `run_recovery_cascade` 已经接受 `(error, tool_name, ctx, tracker, recovery, budget, provider, loop_detector, steering, reset)`，参数化程度足够。
- 区别只在于"是否 LLM 错误" 不影响 cascade 内部逻辑（cascade 不感知错误类型）。
- 共用 cascade = 单一恢复逻辑，避免 split-brain。

### Q6: 每次恢复是否需要事件？

**决议**：✅ 是，添加 `AgentEvent::RecoveryApplied`。

**Schema 草案**：
```rust
AgentEvent::RecoveryApplied {
    level: RecoveryLevel,      // L1/L2/L3/L4/L5
    tool_name: Option<String>, // 触发恢复的工具（None = LLM 错误）
    message: String,            // 恢复动作描述
    iteration: usize,
}
```

**理由**：
- Observability: 调试 agent 行为时需要知道"为什么没崩"
- Telemetry: 可以统计"L4 触发频率"、"L5 触发频率"作为健康指标
- 与现有 `AgentEvent::LoopWarning` 风格一致

### Q7: 是否修改 `RecoveryResult::Escalated` 的语义？

**决议**：❌ 不修改 enum 本身，只改 builder.rs 中的 match 行为。

**理由**：
- 上轮 specs 已经定义了 `RecoveryResult` 的语义
- 改动 enum 会导致 archive specs 失效
- 行为修改在 caller 层是最小侵入

---

## 4. 设计取捨（Design Trade-offs）

### 取捨 1：cascade 入口参数 vs 闭包

- **方案 A**：caller 传 `ctx, tracker, recovery, budget, provider, loop_detector, steering, reset`（8 个参数）
- **方案 B**：把这些字段聚合成一个 `RecoveryContext` struct

**选择**：A（沿用现有 `run_recovery_cascade` 签名）

**理由**：
- 现有签名已经在 `recovery_cascade.rs` 完整测试通过（13 个 test cases）
- 重新设计 struct = 改 13 个测试 + 引入新概念
- caller 端参数传递简单且显式（"显式化" 任务的本意）
- 如果将来参数膨胀，再考虑 struct 化（YAGNI）

### 取捨 2：L1 truncate 在 tool_execute step 还是 cascade 入口

- **方案 A**：在 `StepToolExecute::execute` 内截断所有 tool result
- **方案 B**：在 cascade 入口处（`handle_tool_error`）截断 error result

**选择**：A（但用 `is_error: true` 作为触发条件）

**理由**：
- L1 truncate 是不感知错误类型的，应该总是发生（与现有 `StepSample` 一致）
- 不限于 error result — 正常 tool result 也可能超大
- 这也消除了上轮 spec `tool-output-truncate` 描述但未实现的差距

### 取捨 3：cascade 失败后的兜底

- **方案 A**：cascade 失败时 `return` 结束 session
- **方案 B**：cascade 失败时 yield `AgentEvent::SessionEnded(Error)` 然后 return

**选择**：B（用现有 `SessionEndReason::Error`）

**理由**：
- 现有 `builder.rs:371-373` 已经在 L2 escalate 时这么做
- 一致性比创新重要

### 取捨 4：`ConsecutiveFailureTracker` 的归属

- **方案 A**：放在 `BuilderSteps` 中（每个 session 一个）
- **方案 B**：放在 `LoopContext` 中（与 messages 同生命周期）

**选择**：A

**理由**：
- `run_recovery_cascade` 接收 `&mut ConsecutiveFailureTracker`，已实现为可变借用
- 放在 `BuilderSteps` 跟 `recovery: ErrorRecoveryCoordinator` 对称
- L5 reset 时 cascade 内部会 `tracker.record_success` 清空，无需放在 LoopContext

---

## 5. 关键设计点（Key Design Points）

### 5.1 入口点重写

`builder.rs:355-383` 替换为：

```rust
Err(e) => {
    // LLM sampling 错误 → 走 cascade (L3-L5)
    // (L1 truncate 已在 StepSample::execute 入口完成)
    // (L2 retry 已在 handle_error 内完成)
    let mut cascade_ctx = ctx;
    let action = run_recovery_cascade(
        &e.to_string(),
        "llm_sample",  // virtual tool name
        &mut cascade_ctx,
        &mut recovery_tracker,
        &steps.recovery,
        config.context_token_budget.as_ref(),
        config.compaction_provider.as_deref(),
        &mut loop_detectors,
        steps.steering_channel.as_deref().map(|s| s.as_ref()),
        &reset_coordinator,
    ).await;
    // ... 处理 action
}
```

### 5.2 工具错误处理

`builder.rs:531-541` 替换为：

```rust
let tool_results = match steps.tool_execute.execute(&ctx, tool_calls_to_execute).await {
    Ok(results) => results,
    Err(e) => {
        // 工具执行错误 → 走 cascade
        let action = run_recovery_cascade(
            &e.to_string(),
            &tool_name_on_error,
            &mut ctx,
            &mut recovery_tracker,
            &steps.recovery,
            config.context_token_budget.as_ref(),
            config.compaction_provider.as_deref(),
            &mut loop_detectors,
            steps.steering_channel.as_deref().map(|s| s.as_ref()),
            &reset_coordinator,
        ).await;
        // 注入 cascade 消息作为 tool result
        match action {
            RecoveryAction::Recovered(msg) => vec![ToolResult {
                tool_name: tool_name_on_error.clone(),
                output: msg,
                is_error: true,
            }],
            RecoveryAction::FailFast(reason) => {
                ctx.set_end_reason(SessionEndReason::Error(reason.clone()));
                yield AgentEvent::SessionEnded { ... };
                return;
            }
            _ => unreachable!("run_recovery_cascade no longer produces Escalate"),
        }
    }
};
```

### 5.3 BuilderSteps 新增字段

```rust
pub struct BuilderSteps {
    // ... existing fields ...
    pub recovery: ErrorRecoveryCoordinator,
    pub reset: ResetCoordinator,                           // NEW
    pub failure_tracker: ConsecutiveFailureTracker,        // NEW
}
```

### 5.4 新 AgentEvent 变体

```rust
pub enum AgentEvent {
    // ... existing variants ...
    RecoveryApplied {
        level: RecoveryLevel,
        tool_name: Option<String>,
        message: String,
        iteration: usize,
    },
}
```

### 5.5 L1 truncate 在 tool result 入口

在 `for result in &tool_results` 循环中（`builder.rs:543`）插入：

```rust
for result in &tool_results {
    // L1 truncate: 如果 tool result 超过配置阈值，先 truncate
    let mut output = result.output.clone();
    let truncate_result = synthia_context::truncate::truncate_output(
        &output,
        &truncate_cfg,
    );
    if truncate_result.was_truncated {
        output = truncate_result.content;
        yield AgentEvent::RecoveryApplied {
            level: RecoveryLevel::L1Truncate,
            tool_name: Some(result.tool_name.clone()),
            message: format!("Truncated tool output ({} → {} bytes)",
                             result.output.len(), output.len()),
            iteration: ctx.iteration,
        };
    }
    // ... yield ToolCallCompleted with truncated output ...
}
```

---

## 6. 风险与缓解（Risks & Mitigations）

| 风险 | 缓解 |
|------|------|
| 修改 builder.rs 引入新 bug | 严格 TDD：先写测试，再 wire up |
| cascade 调用阻塞 agent loop | cascade 内部已经 async + 不持锁；同 LLM 错误处理 |
| `RecoveryLevel` 不是 `Serialize`（影响 AgentEvent） | 新建一个 `RecoveryLevelDto` 或在 `AgentEvent` 中只暴露 `level_number: u32` |
| L5 reset 误清空 `ctx.messages` | 沿用现有 `ResetCoordinator` 的 30s cooldown 保护；L5 已经过 L3/L4 兜底 |
| 性能：每次 tool result 都 truncate 检查 | `truncate_output` 是 O(n) 字符串扫描，threshold 默认 30KB，大多数 result < 1KB 走 fast path |

---

## 7. 开放问题（Open Questions）

1. **AgentRunConfig 是否需要新增 `compaction_provider` 字段？**
   - 现状：cascade 需要 `Option<&dyn CompactionProvider>`，但 `AgentRunConfig` 没有这个字段。
   - 推测：从 `synthia_context::compaction` 模块的初始化处传入。
   - 决议：实施时调研 `AgentRunConfig` 现有字段，必要时添加。

2. **`RecoveryLevel` 序列化**：
   - 上轮 spec 写它是 `pub enum RecoveryLevel { L1Truncate, L2Retry, ... }`，没有 derive Serialize。
   - 决议：在 `AgentEvent::RecoveryApplied` 中只放 `level_number: u32`（如 1-5），避免 derive。
   - 这样也保持 event 序列化稳定。

3. **L1 truncate 的 config 从哪来？**
   - `StepSample` 用 `TruncateConfig::default()`，但 `tool_execute` 没有 truncate 概念。
   - 决议：复用 `TruncateConfig::default()`，由 `StepSample` 的 `truncate_cfg` 字段统一（移到 `BuilderSteps` 或 `AgentConfig`）。
   - 简化决策：直接 `TruncateConfig::default()`，与 `StepSample` 保持一致。

4. **是否需要新增 `RecoveryLevel::L0` 表示"无错误"**？
   - 决议：❌ 不需要。`RecoveryApplied` 事件只在真实恢复时 yield。

---

## 8. 与上轮 archive 的关系

- ✅ **不修改** `error_recovery/recovery_cascade.rs` 的 `run_recovery_cascade` 签名和实现
- ✅ **不修改** 5 个 `error_recovery/*` 模块的公共 API
- ✅ **不修改** archive specs (`specs/auto-compact-on-error`, `specs/session-reset`, `specs/tool-fallback`, `specs/tool-output-truncate`, `specs/tool-retry`)
- 🔧 **修改** `stream_builder/builder.rs`：wire up cascade
- 🔧 **修改** `events.rs`：新增 `AgentEvent::RecoveryApplied`
- 🔧 **修改** `BuilderSteps`：新增 `reset` 和 `failure_tracker` 字段
- ➕ **新增** spec `specs/recovery-cascade-wiring`：规定 cascade 在哪些错误点必须被调用

---

## 9. 下一步

1. **proposal.md** — 提炼本档 Q1-Q7 决议为 "what & why"
2. **design.md** — 重组本档 §3-§5 为结构化设计
3. **specs/recovery-cascade-wiring/spec.md** — formal requirements + scenarios
4. **tasks.md** — 5-7 个 micro-tasks（TDD-first）
5. **plan.md** — 执行顺序
6. **verify.md + retrospective.md** — 验证和复盘
