## Context

synthia-agent 是项目的 AI agent 实现，当前存在多个生产级问题影响稳定性和可观测性。代码库包含 ~1500 行重复/死代码，三套并行的 Agent 实现，两套 reflection 逻辑，以及多个 critical bugs。

**当前问题**:
- Hook Modify 功能完全失效
- 工具名称丢失导致错误追踪无效
- Token 追踪未连线，事件发送硬编码 0 值
- 5-layer error recovery 只有 L2 理论存在，其他层未实现
- 6+ 处 silent error swallowing
- 大量 unsafe unwrap/panic

**约束**:
- 必须向后兼容，不能破坏现有 API
- 需要覆盖现有测试
- 某些死代码可能仍有外部引用

## Goals / Non-Goals

**Goals:**
1. 修复 Hook Modify bug - 使 hook 实际能修改工具输入
2. 修复工具名称丢失 - 保留真实工具名而非 `tool_N`
3. 修复 unsafe unwrap - 消除 panic 风险
4. 实现 token 追踪 - 让 TokenBudgetWarning 发送真实值
5. 实现结构化错误处理 - 替代 silent error swallowing
6. 删除 ~1500 行死代码 - 减少维护负担

**Non-Goals:**
- 不重构整个 agent 架构
- 不实现完整的 5-layer recovery（Phase 2）
- 不删除仍在使用的 legacy agent/core.rs
- 不修改事件系统的基础架构

## Decisions

### D1：修复 Hook Modify Bug

- **選擇**：修改 `builder.rs` 中 `ToolAction::Modify` 的处理逻辑，将修改后的输入实际用于工具执行
- **理由**：当前 `Modify` 分支只是记录日志然后用原始输入执行，完全无效
- **已考慮 alternative**：
  - 在 `StepToolExecute` 层面支持 Modify（需要改更多文件）
  - 在 hook 系统层面验证 Modify 被应用（只加日志，无实际效果）

**实现方案**：
```rust
Ok(synthia_hook::ToolAction::Modify(new_input)) => {
    let modified_tool_call = ToolCall {
        name: new_input.get("name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| tool_call.name.clone()),
        input: new_input.get("input")
            .cloned()
            .unwrap_or_else(|| tool_call.input.clone()),
        ..tool_call.clone()
    };
    tracing::debug!(tool=%tool_call.name, "Hook modified tool input");
    modified_calls.push(modified_tool_call);
}
```

---

### D2：修复工具名称丢失

- **選擇**：在 `tool_execute.rs` 中使用 `zip` 将原始 tool_calls 的 name 与执行结果关联
- **理由**：直接使用输入时的工具名，而非用索引生成伪名
- **已考慮 alternative**：
  - 在 provider 层 `ToolResult` 添加 name 字段（需要改 provider API）
  - 用 `tool_use_id` 做 lookup（需要额外的数据结构）

**实现方案**：
```rust
let outputs = self.tool_registry.run_with_context(tool_calls.clone(), context).await?;
Ok(tool_calls.into_iter().zip(outputs).map(|(call, o)| ToolResult {
    tool_name: call.name,  // 使用原始名称
    output: o.content.iter().filter_map(|p| p.text()).collect::<Vec<_>>().join("\n"),
    is_error: o.is_error.unwrap_or(false),
}).collect())
```

---

### D3：修复 unsafe unwrap

- **選擇**：使用 `unwrap_or_else` 或提前检查 `end_reason`
- **理由**：`unwrap()` 在 `end_reason` 为 `None` 时会 panic
- **已考慮 alternative**：
  - 使用 `expect()` 加详细错误信息（仍是 panic）
  - 返回 `Result` 类型（需要改函数签名）

**实现方案**：
```rust
// 当前代码
ctx.end_reason.clone().unwrap()
// 改为
ctx.end_reason.clone().unwrap_or(SessionEndReason::Error("Unknown".to_string()))
```

---

### D4：实现 Token 追踪

- **選擇**：在 `sample.rs` 返回后更新 `ctx.cumulative_tokens`，在事件中传递实际值
- **理由**：基础设施已存在，只需"连线"
- **已考慮 alternative**：
  - 在 `compact.rs` 单独计算（重复计算）
  - 在 `builder.rs` 循环外累计（丢失 per-iteration 详情）

**实现方案**：
1. `sample.rs` 返回 `SamplingResult` 后，在 `builder.rs` 中调用 `ctx.cumulative_tokens += usage.total()`
2. 事件中 `current_tokens` 改为 `ctx.cumulative_tokens`，`threshold_tokens` 改为 `budget.hard_limit`

---

### D5：替换 Silent Error Swallowing

- **選擇**：将 `let _ =` 改为结构化日志 + 指标
- **理由**：silent swallowing 丢失错误信息，难以排查问题
- **已考慮 alternative**：
  - 改为返回 `Result`（需要改函数签名，影响较大）
  - 使用 `tracing::warn!` 但不返回错误（接受当前限制）

**实现方案**：
```rust
// 当前
let _ = steps.hooks.fire_before_llm(&mut agent_ctx).await;
// 改为
if let Err(e) = steps.hooks.fire_before_llm(&mut agent_ctx).await {
    tracing::warn!(error = %e, "before_llm hook failed");
    // 继续执行，不阻塞主流程
}
```

---

### D6：删除死代码

- **選擇**：删除 `agent_runtime.rs`、`agent.rs`、`react.rs::step_self_reflection()`
- **理由**：这些文件无人引用或已被 `stream_builder` 替代
- **已考慮 alternative**：
  - 保留作为参考文档（增加维护负担）
  - 移到 `deprecated/` 目录（仍需维护）

**删除范围**：
- `src/agent_runtime.rs`（300 行）- 无人引用
- `src/agent.rs`（241 行）- 无人引用
- `src/react.rs::step_self_reflection()`（约 60 行）- 已被 `steps/reflect.rs` 替代

## Risks / Trade-offs

[Risk] 修改 Hook Modify 逻辑可能破坏现有 hook 行为 → Mitigation: 添加集成测试验证 Modify 实际生效

[Risk] 删除死代码可能仍有未知依赖 → Mitigation: 先用 `cargo build` 验证无编译错误

[Risk] Token 追踪修改可能影响性能 → Mitigation: token 计算已有缓存，影响可控

[Trade-off] 结构化错误处理会产生更多日志 → 接受：生产环境需要可观测性

## Migration Plan

1. **Phase 1（Critical Bugs）**
   - 修复 Hook Modify bug
   - 修复工具名称丢失
   - 修复 unsafe unwrap
   - 验证：`cargo test -p synthia-agent`

2. **Phase 2（Token & Error）**
   - 实现 token 追踪
   - 替换 silent error swallowing
   - 验证：`cargo test -p synthia-agent && cargo clippy`

3. **Phase 3（Cleanup）**
   - 删除死代码
   - 验证：`cargo build && cargo test`

Rollback：git revert 单个 commit

## Open Questions

1. `agent/core.rs` 仍在使用，是否需要重构到 `stream_builder` 架构？
2. 5-layer error recovery 是否需要在 Phase 1 一并实现？
3. 是否有其他模块依赖 `react.rs` 中的 `step_self_reflection`？