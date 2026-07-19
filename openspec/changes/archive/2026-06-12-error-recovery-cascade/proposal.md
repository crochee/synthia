# Error Recovery Cascade — Proposal

## 1. Why

`synthia-agent` 已有完整的 L1-L5 五层恢复框架，但 StreamBuilder **完全没有接入工具执行错误的恢复路径**。当前只有 LLM sampling 失败时调用 `handle_error(L2Retry)`，工具执行错误（如超时、超长输出、连续失败）从未触发任何 recovery 层。

这导致：
- 工具输出 50KB → 直接入 context，浪费 token
- 网络超时一次 → 立即失败，没有 retry
- 工具连续失败 2 次 → 升级到 L4 compact，而不是先尝试降级
- 30 次连续失败 → 框架存在但无真正 session 重建

## 2. What Changes

### L1 Truncate（工具输出截断）
- `ToolOutput` 新增 `truncated: bool` + `original_len: usize` 字段
- `ToolExecutor::execute()` 返回前检查 output > 16KB → 截断为 head(8KB) + tail(8KB) + marker
- Marker 格式: `[... output truncated: showed 16384 of {original_len} bytes ...]`

### L2 Retry（工具超时重试）
- `ToolExecutor::execute()` 内嵌 retry 循环
- `is_retryable(error) -> bool` 匹配 "timeout", "connection reset", "rate limit", "5xx", "429"
- Exponential backoff: 2s → 4s → 8s，最多 2 次

### L3 Fallback（降级路径）
- StreamBuilder tool step 错误 → 调用 `run_recovery_cascade()`
- L3: `FallbackProvider::get_fallback(tool_name)` → 返回降级消息
- 降级消息作为 tool result 注入 context，调用 `record_success()`

### L4 Auto-Compact（自动压缩）
- L3 fallback 完成后检查 `ctx.token_ratio() > 0.8`
- 超过阈值 → 调用 `compact_with_fallback()`
- 成功 → `record_success()`；失败 → 升级到 L5

### L5 Reset（会话重建）
- `ResetCoordinator::execute(Conversation)` → 丢弃 context messages，重新开始
- 调用 `LoopDetectorSet::reset()` + drain steering channel

### 防死锁
- 复用 `ErrorRecoveryCoordinator.consecutive_errors` 计数器
- 每触发一次 L4/L5 的 `handle_error()`，计数器 +1
- 3 次 → `RecoveryResult::FailFast`，进入 30s cooldown

## 3. Capabilities

### New Capabilities
- `tool-output-truncate`: 工具输出 > 16KB 自动截断 head+tail+marker
- `tool-retry`: 超时/临时错误自动重试最多 2 次
- `tool-fallback`: 工具连续失败 2 次返回降级消息
- `auto-compact-on-error`: L3 失败 + context > 80% 时自动压缩
- `session-reset`: 30 次连续失败重建 session

### Modified Capabilities
- `tool-execution`（已有）: 接入 L1 truncate + L2 retry
- `error-recovery`（已有）: 接入 L3/L4/L5 cascade 逻辑
- `loop-detection`（已有）: L5 时 reset circuit breaker

## 4. Impact

**代码改动**：
- `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs` — L1/L2 接入
- `crates/synthia-agent/src/stream_builder/steps/recovery_cascade.rs` — L3/L4/L5 cascade（新建）
- `crates/synthia-agent/src/error_recovery/fallback.rs` — 补全测试
- `crates/synthia-agent/src/error_recovery/compact.rs` — 补全 StreamBuilder 集成
- `crates/synthia-agent/src/error_recovery/reset.rs` — 补全 reset scope 执行
- `crates/synthia-agent/src/stream_builder/builder.rs` — 接入 recovery cascade

**依赖关系**：
- `synthia-agent` → `synthia-context::Compactor`（已有）
- `synthia-agent` → `synthia-guardian::LoopDetectorSet`（已有）

**测试**：
- 新增 unit tests ≥ 20 个（L1 truncate 4 个，L2 retry 6 个，L3 fallback 4 个，L4 compact 3 个，L5 reset 3 个）
- 新增 integration tests ≥ 2 个（tool timeout retry，tool 连续失败 fallback）
- 保留所有现有 tests 通过

**性能影响**：
- L1 truncate: < 1ms/次，O(n) 截断
- L2 retry: 最多增加 14s（2s + 4s + 8s），仅在 timeout 时触发
- L4 compact: 同现有 compact 开销
- 正常路径（无错误）：零额外开销

**行为变化**：
- 工具超长输出现在会截断，LLM 看到 marker
- 工具超时现在会重试，失败率降低
- 工具连续失败现在会返回降级消息，而不是直接 compact
- 30 次连续失败现在会真正重建 session
