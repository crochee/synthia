# Brainstorming: Synthia-Agent 生产级差距分析

## 背景

用户要求分析 synthia-agent 与生产级 AI agent 的差距，包括优化点和重复逻辑。

## 深度探索发现

### 🔴 Critical Bugs

#### 1. Hook Modify 完全失效
**位置**: `stream_builder/builder.rs:319-323`

```rust
Ok(synthia_hook::ToolAction::Modify(new_input)) => {
    let _modified_call = serde_json::to_string(&new_input).unwrap_or_default();
    tracing::debug!(tool=%tool_call.name, "Hook modified tool input");
    // ⚠️ BUG: _modified_call 从未被使用！
}
```

**问题**: hook 返回 `Modify` → 记录日志 → **用原始参数执行工具**。任何修改输入的 hook 都无效。

**影响**: 安全 hook、参数重写 hook 全部失效。

---

#### 2. 工具名称丢失
**位置**: `stream_builder/steps/tool_execute.rs:29`

```rust
tool_name: format!("tool_{}", i),  // 用了索引而非真实名称
```

**问题**: 所有工具都显示为 `tool_0`, `tool_1`... 错误追踪完全失效。

**影响**: 无法区分哪个工具失败，memory/reflection 无法引用具体工具名。

---

#### 3. 错误路径工具名丢失
**位置**: `builder.rs:333`

```rust
tool_name: "error".to_string(),  // 硬编码而非实际工具名
```

---

#### 4. Unsafe unwrap 可能导致 panic
**位置**: `builder.rs:249`

```rust
ctx.end_reason.clone().unwrap()
```

如果 `end_reason` 是 `None`，会 panic。

---

### 🟠 Error Recovery 系统几乎失效

#### 5-Layer Recovery 架构存在但未实现

| Layer | 状态 | 问题 |
|-------|------|------|
| L1Truncate | ❌ 未实现 | 直接 escalation 到 L2 |
| L2Retry | ⚠️ 理论存在 | builder.rs 根本不 retry，直接 escalation |
| L3Fallback | ❌ 未实现 | escalation 后直接结束 session |
| L4Compact | ❌ 未实现 | 同上 |
| L5Reset | ❌ 未实现 | 同上 |

**实际行为**: 错误 → 立刻终止 session，从不尝试恢复。

#### Silent Error Swallowing（6处）

| 位置 | 代码 | 风险 |
|------|------|------|
| builder.rs:110 | `let _ = session_store.ensure_session_dir(...)` | 会话目录创建失败 |
| builder.rs:214 | `let _ = steps.hooks.fire_before_llm(...)` | hook 错误丢失 |
| builder.rs:261 | `let _ = steps.hooks.fire_after_llm(...)` | hook 错误丢失 |
| builder.rs:349 | `let _ = sender.send(MemoryEvent::tool_executed(...))` | 内存事件丢失 |
| builder.rs:385 | `let _ = sender.send(MemoryEvent::session_end(...))` | 会话结束事件丢失 |
| agent_runtime.rs:177 | `let _ = mcp.stop_all().await` | MCP 关闭最佳努力 |

---

### 🟡 Token 追踪：基础设施存在但未连线

#### 已实现
- `AgentConfig::context_token_budget` - 完整配置
- `TokenBudget::check()` - 验证逻辑
- `estimate_messages_token_count()` - 估算函数
- `sample.rs` - 捕获 token usage

#### 未实现
- `ctx.cumulative_tokens` 从未更新
- `TokenBudgetWarning` 事件硬编码 `current_tokens: 0, threshold_tokens: 0`
- 没有任何地方把实际 token 数传给事件

---

### 🔴 重复代码：~1500+ 行死代码

#### Agent 实现（三套）

| 文件 | 行数 | 状态 |
|------|------|------|
| `agent_runtime.rs` | 300 | **死代码** - 无人引用 |
| `agent.rs` | 241 | **死代码** - 无人引用 |
| `agent/core.rs` | 628 | **活跃** - 实际使用 |

#### Reflection 逻辑（两套）
- `react.rs::step_self_reflection()` - 死代码
- `stream_builder/steps/reflect.rs::execute()` - 活跃
- **99% 相同**

#### Compaction（三套）
- `agent/compact.rs`
- `stream_builder/steps/compact.rs`
- `compaction.rs`

#### Tool Registry（两套）
- `registry/` 目录
- `tool_registry.rs` 文件

---

### 🟠 架构问题

#### 1. Session 恢复不是原子操作
`Agent::resume()` 中 `patch_tool_calls_recovery()` 调用没有错误处理。

#### 2. Steering Channel 竞争
`MpscSteeringChannel` 使用 `Mutex<Vec<PriorityMsg>>` + 线性搜索。

#### 3. Agent 结构体字段全是 `pub`
`agent_runtime.rs:45-62` 所有字段 `pub` - 绕过了 builder 模式封装。

---

### 🟢 已完成的好设计

- 30+ 事件类型的完整 event system
- 5-layer recovery 架构定义
- 正确的 `CancellationToken` 集成
- 异步 stream 输出
- 模块化 step 设计
- `AgentConfig` 验证

---

## 优先级决策

### Phase 1（影响生产稳定性）- 必须修复
1. 修复 Hook Modify bug
2. 修复工具名称丢失
3. 修复 unsafe unwrap

### Phase 2（生产可观测性）
4. 实现 token 追踪
5. 替换 silent error swallowing 为结构化日志
6. 实现完整的 error recovery

### Phase 3（技术债务）
7. 删除 1500+ 行死代码
8. 统一 Agent/compaction/tool_registry 实现

---

## 澄清问题记录

无 - 用户要求直接分析，已完成。