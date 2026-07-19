# Error Recovery Cascade — Technical Design

## Context

`synthia-agent/src/error_recovery/` 已有一个 L1-L5 五层恢复框架（`ErrorRecoveryCoordinator` + `RetryStrategy` + `FallbackProvider` + `CompactCoordinator` + `ResetCoordinator`），但 **StreamBuilder 完全未接入这些层**。当前只在 LLM sampling 失败时调用 `handle_error(L2Retry)`，工具执行错误从未触发任何 recovery 层。

Spec 要求：
- L1 Truncate: output > 16KB → head + tail + marker
- L2 Retry: timeout/temporary error, max 2 attempts, exponential backoff
- L3 Fallback: same tool fails 2x → degraded path
- L4 Auto-Compact: context > 80% + pruning insufficient
- L5 Reset: 30 consecutive failures → rebuild session
- 防死锁: 3 recovery cycles → fail-fast; 30s cooldown after L5

## Goals / Non-Goals

**Goals:**
- L1 Truncate 在 tool executor 层接入，超长 output 自动截断
- L2 Retry 在 tool executor 层接入，超时自动重试
- L3 Fallback 在 tool 执行错误后触发，返回降级消息而非升级
- L4 Auto-Compact 在 L3 失败且 context > 80% 时触发
- L5 Reset 实现 session 重建，loop detector reset
- 防死锁机制（cooldown + consecutive cycle counter）

**Non-Goals:**
- 不修改现有 LLM sampling 错误处理路径（已单独工作）
- 不实现 LLM 层的 truncate/retry/fallback（那是 provider 层的事）
- 不改变 `ContextAssembler` 的公开 API
- 不实现 fallback 实际执行替代操作（只返回降级消息）

## Decisions

### D1: Recovery Cascade 触发点

- **选择**: ToolExecutor 层是 L1/L2 的自然接入点；StreamBuilder 的 tool step 错误是 L3/L4/L5 的触发点
- **理由**: L1/L2 属于 tool 执行的一部分（截断 + retry 对上层透明）；L3/L4/L5 需要 session 级别状态，在 StreamBuilder 协调
- **已考虑 alternatives**: 在 StreamBuilder 统一处理 → 违反单一职责

### D2: L1 Truncate 的实现方式

- **选择**: `ToolOutput` 新增 `truncated: bool` + `original_len: usize` 字段；超过 16KB 的 output 在 `ToolExecutor::execute()` 返回前截断为 head(8KB) + tail(8KB) + marker
- **理由**: 截断是执行的一部分，tool executor 自然知道 output 大小；`truncated` 标志让 StreamBuilder 可记录日志
- **已考虑 alternatives**: 在 context 注入前截断 → 需要传递截断需求到 executor，增加耦合

### D3: L2 Retry 的 error 类型判断

- **选择**: 通过 `is_retryable(error: &str) -> bool` 函数判断；匹配 "timeout", "timed out", "connection reset", "temporary failure", "rate limit", "503", "502", "429" 等 pattern
- **理由**: 简单有效，无需引入复杂的 error trait；暂时性错误有明确特征
- **已考虑 alternatives**: 定义 `RetryableError` trait → 过度设计

### D4: L3 Fallback 的执行方式

- **选择**: `FallbackProvider` 返回 `FallbackStrategy`（包含降级消息）；StreamBuilder 将消息作为 tool result 注入 context，记录 `record_success()`（因为 fallback 成功了）
- **理由**: 符合 spec 的 "use degraded path"；不需要真正执行替代操作，避免引入新错误
- **已考虑 alternatives**: 实际执行替代 tool → 实现复杂，fallback 本身可能失败

### D5: L4 Auto-Compact 的触发时机

- **选择**: L3 fallback 完成后检查 `ctx.messages.token_ratio() > 0.8`
- **理由**: 符合 spec 的 L3 → L4 升级语义；compact 是 L3 失败的升级响应，不是每个错误的默认响应
- **已考虑 alternatives**: 每次 error 都检查 context 比率 → 过于频繁

### D6: L5 Reset 的实现范围

- **选择**: 初始实现 `ResetScope::Conversation`（仅丢弃当前 context messages，重新开始）；调用 `LoopDetectorSet::reset()` + drain steering channel
- **理由**: spec 明确 "rebuild session"；Conversation scope 最干净，错误状态完全清除
- **已考虑 alternatives**: Full scope → 过度杀伤，用户状态可能丢失

### D7: 防死锁机制

- **选择**: `ErrorRecoveryCoordinator` 已有 `consecutive_errors` 计数器；每触发一次 L4/L5 的 `handle_error()` 调用，计数器 +1；3 次 → `RecoveryResult::FailFast`
- **理由**: 复用现有框架，只需要在 L3/L4 触发时正确调用 `handle_error()`
- **已考虑 alternatives**: 新增 `recovery_cycles` 字段 → 不必要，现有计数器已够用

## Risks / Trade-offs

[Risk] L1 truncate 可能丢失关键信息（截断点在中间） → Mitigation: 保留 head + tail 各 8KB，保留首尾内容；marker 明确告知 LLM 发生了截断

[Risk] L2 retry 可能加重 server 负载（指数退避公式为 2*2^n） → Mitigation: max=2 次限制；base delay 2s，最大 8s

[Risk] L3 fallback 返回降级消息，LLM 可能不理解如何处理 → Mitigation: 消息格式为 "Tool 'xxx' temporarily unavailable. Reason: [error]. Suggestion: [fallback action]"

[Risk] L4 compact 需要调用 LLM 生成摘要，compact 本身可能失败 → Mitigation: compact_with_fallback 已有 L1→L2→L3 降级链；失败后升级到 L5

[Trade-off] Fallback 不真正执行替代操作 vs 实际执行 → 接受：简化实现，避免 fallback 本身引入新错误；降级消息已足以让 LLM 自适应

[Trade-off] Recovery cascade 增加了 tool 执行延迟（L2 retry 需要等待） → 接受：只在 timeout/temporary error 时触发，正常路径无额外延迟

## Migration Plan

N/A — 纯内部重构，无 endpoint/DB/配置变更。

1. **Phase 1 (L1 Truncate)**: 修改 `ToolOutput` + `ToolExecutor::execute()`，新增截断逻辑
2. **Phase 2 (L2 Retry)**: 在 `ToolExecutor::execute()` 内嵌 retry 循环 + exponential backoff
3. **Phase 3 (L3 Fallback)**: StreamBuilder tool step 错误 → 调用 `run_recovery_cascade()` → L3 分支
4. **Phase 4 (L4 Auto-Compact)**: L3 fallback 后检查 context ratio，调用 `compact_with_fallback()`
5. **Phase 5 (L5 Reset)**: 实现 `ResetCoordinator::execute(Conversation)` + loop detector reset
6. **Phase 6 (防死锁)**: 验证 `consecutive_errors` 计数器在 L3/L4 正确递增

验收条件：
- `cargo test -p synthia-agent --lib`: 全部通过
- `cargo clippy --all-targets`: 无新警告
- tool 输出 > 16KB → 截断为 head+tail+marker
- tool timeout → 最多 2 次 retry
- 同 tool 连续 2 次失败 → 返回降级消息，不升级到 L4
- context > 80% + L3 失败 → 自动 compact
- 30 次连续失败 → session 重建

## Open Questions

1. L1 truncate 的阈值（16KB）是否应该可配置？
   - 当前：硬编码 16KB
   - 备选：从 config 读取 `tool_output_truncate_threshold`

2. L2 retry 的 max attempts（2次）是否应该可配置？
   - 当前：硬编码 2
   - 备选：从 config 读取 `tool_retry_max_attempts`

3. `is_retryable` 的 error pattern 是否完整？
   - 当前：timeout, connection reset, rate limit, 5xx, 429
   - 可能遗漏：SSL error, DNS failure
