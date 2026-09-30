# Brainstorm: Error Recovery Cascade

## Background

### Current State

`synthia-agent/src/error_recovery/` 已有 L1-L5 五层恢复框架：

| Layer | 文件 | 状态 |
|-------|------|------|
| 框架定义 | `mod.rs` (176 行) | ✅ 完整，单元测试 100% |
| L2 Retry | `retry.rs` (86 行) | ✅ 框架完整，指数退避公式正确 |
| L3 Fallback | `fallback.rs` (104 行) | ⚠️ stub 完整，但未接入执行路径 |
| L4 Compact | `compact.rs` (111 行) | ⚠️ stub 完整，但未在 recovery 路径调用 |
| L5 Reset | `reset.rs` (193 行) | ⚠️ stub 完整，但 reset 逻辑为空 |

**核心问题：** StreamBuilder (`builder.rs:355-383`) 只在 LLM sampling 失败时调用 `handle_error(L2Retry)`，工具执行错误从未触发任何 recovery 层。

### Spec Requirements

`openspec/specs/error-recovery/spec.md` 明确了 4 个关键要求：

1. **五层顺序执行**：L1 Truncate (16KB+) → L2 Retry (timeout, ≤2 次) → L3 Fallback (连续 2 次失败) → L4 Auto-Compact (context > 80%) → L5 Reset (30 次连续失败)
2. **防死锁**：3 次 recovery cycle → fail-fast；L5 后 30 秒 cooldown
3. **ReAct loop 错误清理**：退出时 reset circuit_breaker + drain steering channel + 清理资源
4. **降级路径**：web_fetch → cached/网络不可用；bash → 简化命令；read_file → 仅前 100 行

### Gap Analysis

| 层 | Gap | 影响 |
|----|-----|------|
| L1 | 未对 tool output 超长做 truncate | 50KB output 直接入 context，浪费 token |
| L2 | 未对 tool timeout 做 retry | 网络抖动直接失败 |
| L3 | 未对连续 tool 失败做 fallback | 任何工具连续失败 2 次 → 升级到 L4 |
| L4 | 未在 error path 触发 auto-compact | 错误积累时无法自动压缩 |
| L5 | reset scope 实现为空 | 30 次失败后无真正重置 |

---

## Decision Chain

### Q1: Recovery 协调器应管理所有层还是各层独立？

**选项 A：协调器集中管理（推荐）**
- `ErrorRecoveryCoordinator` 是唯一入口
- 各层实现通过 trait 注入
- 优点：状态集中，死锁检测在一个地方
- 缺点：协调器变大

**选项 B：各层独立，StreamBuilder 按顺序调用**
- 每层有独立状态
- StreamBuilder 按 L1→L2→L3→L4→L5 顺序调用
- 优点：简单，每层可独立测试
- 缺点：死锁检测分散

**决策：选项 B（各层独立）** — 现有 `ErrorRecoveryCoordinator` 主要用于死锁检测（cooldown + consecutive_errors），各 recovery 动作在 StreamBuilder 中按序触发。

### Q2: L1 Truncate 在哪层触发？

**选项 A：ToolExecutor 内触发（推荐）**
- tool 执行完成后检查 output 大小
- 超过 16KB → 截断 head + tail + marker
- 优点：tool 执行者知道何时截断

**选项 B：ToolResult 入 context 前触发**
- 在 `StreamBuilder` 的 tool 步骤后检查
- 优点：截断逻辑集中
- 缺点：需要传递截断需求到 tool executor

**决策：选项 A（L1 在 ToolExecutor 层）** — tool executor 自然知道自己的 output，截断属于 tool 执行的一部分。L1 RecoveryLevel 在 spec 中是 "Truncate"，即触发条件是 "output > 16KB"，但实际截断逻辑在 tool executor 内执行。

### Q3: L2 Retry 应在哪个粒度？

**选项 A：ToolExecutor 内嵌 retry（推荐）**
- tool 执行层内部实现 timeout retry
- 最多 2 次，指数退避
- 优点：retry 对上层透明

**选项 B：StreamBuilder 层 retry**
- StreamBuilder 检测 tool 错误后手动重试
- 优点：retry 逻辑可见
- 缺点：需要在 StreamBuilder 中复制 retry 逻辑

**决策：选项 A（Executor 内嵌 retry）** — 符合单一职责，retry 是 tool 执行的一部分。StreamBuilder 只处理 error，不处理 retry 逻辑。

### Q4: L3 Fallback 的触发和执行方式？

**选项 A：FallbackProvider 返回降级消息（推荐）**
- 连续 2 次 tool 失败 → 查询 `FallbackProvider`
- 有 fallback → 返回降级消息作为 tool result
- 无 fallback → 升级到 L4
- 优点：不需要真正执行替代操作

**选项 B：FallbackProvider 实际执行替代操作**
- 有 fallback → 执行替代 tool
- 优点：更强大
- 缺点：实现复杂，可能引入新错误

**决策：选项 A（返回降级消息）** — 简化实现，避免 fallback 本身失败的风险。降级消息告知 LLM 当前工具不可用，让 LLM 自适应。

### Q5: L4 Auto-Compact 如何在 error path 接入？

**选项 A：在 L3 fallback 失败后触发（推荐）**
- L3 完成后检查 `context.token_ratio() > 0.8`
- 超过阈值 → 调用 `compactor.compact_with_fallback()`
- 优点：符合 L3 → L4 升级路径

**选项 B：每次 error 后都检查 context 比率**
- 优点：及时压缩
- 缺点：可能过于频繁触发 compact

**决策：选项 A（fallback 后检查）** — 符合 spec 的 L3 → L4 升级语义。compact 是 L3 失败的升级，不是每个错误的默认响应。

### Q6: L5 Reset 的实现范围？

**选项 A：Session 重建（推荐）**
- 创建新 session，保留 HotMemory
- 丢弃当前 context，重新开始
- 优点：干净，错误状态完全清除

**选项 B：Context 截断，不重建 session**
- 保留 session ID，仅截断 context
- 优点：更轻量
- 缺点：错误状态可能残留

**决策：选项 A（Session 重建）** — spec 明确 "rebuild session"，且 ResetCoordinator 已有 `ResetScope::Conversation/ToolState/Full` 定义。初始实现 `Conversation` scope（仅重置 context）。

### Q7: StreamBuilder 如何改造以支持 cascade？

**选项 A：新增 `run_recovery_cascade()` 方法（推荐）**
- 在 `run_stream()` 内，tool/step 错误后调用 cascade
- cascade 方法内部按 L1→L2→L3→L4→L5 顺序处理
- 优点：最小侵入，不改现有流程

**选项 B：在现有 `handle_error` 路径扩展**
- 扩展现有 `handle_error(L2Retry)` 调用点
- 根据当前 level 决定下一步
- 缺点：现有代码只处理 LLM 错误

**决策：选项 A（新增 recovery cascade 方法）** — 现有 `handle_error` 用于 LLM sampling 错误，新的 cascade 用于 tool 执行错误。两条路径分离，避免混淆。

---

## Design Trade-offs

### Trade-off 1: L1 Truncate 的触发位置

L1 的 spec 定义是 "output > 16KB → truncate head + tail"。截断逻辑应该在 tool executor 内执行，因为：
- 只有 tool executor 知道哪些 output 是自己产生的
- 截断是执行的一部分，不是错误处理

但 StreamBuilder 需要感知截断发生（记录日志、上报指标）。通过 `ToolOutput::truncated()` 标志传递。

### Trade-off 2: Retry 的 error 类型判断

L2 Retry 针对 "timeout/temporary error"，不针对所有错误。需要区分：
- **可重试**：网络超时、临时服务不可用、rate limit
- **不可重试**：参数错误、权限拒绝、文件不存在

通过 `RetryableError` trait 或简单 error message pattern matching 判断。

### Trade-off 3: Recovery 与 loop detection 的交互

L5 Reset 后，loop detector 应该 reset。但 `LoopDetectorSet::reset()` 已经是公开方法，在 L5 执行时调用即可。

---

## Validated Design Summary

### 架构

```
StreamBuilder
  └── tool_execute step
        ├── ToolExecutor
        │     ├── L1: truncate_if_large(output > 16KB)
        │     └── L2: retry_if_timeout(max=2, backoff=2^n)
        └── if error → run_recovery_cascade(error, tool_name)
              ├── L3: FallbackProvider.get_fallback(tool_name)
              │         → 有 fallback → 返回降级消息，record_success()
              │         → 无 fallback → Escalated(L4)
              ├── L4: context.token_ratio() > 0.8 → compact_with_fallback()
              │         → 成功 → record_success()
              │         → 失败 → Escalated(L5)
              └── L5: ResetCoordinator.reset(Conversation)
                        → 重建 session，reset circuit_breaker
```

### 关键文件

- `synthia-agent/src/stream_builder/steps/tool_execute.rs` — L1/L2 接入点
- `synthia-agent/src/stream_builder/steps/recovery_cascade.rs` — L3/L4/L5 cascade 逻辑
- `synthia-agent/src/error_recovery/fallback.rs` — 已有，补全测试
- `synthia-agent/src/error_recovery/compact.rs` — 已有，补全 StreamBuilder 集成
- `synthia-agent/src/error_recovery/reset.rs` — 已有，补全 reset scope 执行

### 验收标准

1. tool 输出 > 16KB → 自动截断 head+tail+marker，LLM 能感知截断
2. tool timeout → 自动 retry 最多 2 次，指数退避
3. 同 tool 连续 2 次失败 → L3 返回降级消息，不升级
4. context > 80% + L3 失败 → 自动 compact，token 减少
5. 30 次连续失败 → L5 session 重建，loop detector reset
6. 3 次 recovery cycle → fail-fast，cooldown 30 秒
