## Context

### Background

2026-06-12 的 `error-recovery-cascade` change 完成了 Synthia 的 L1-L5 错误恢复系统的**实现**：truncate / retry / fallback / compact / reset 5 层策略和 `run_recovery_cascade` 协调器已落地，13 个单元测试通过。但该 change 的 retrospective 明确指出存在实施 gap：

> 🟡 **L4 集成路径在 builder.rs 中只 yield tracing 而非真实压缩** — Phase 4 简化避免侵入 builder.rs；Phase 5 已部分修复，但 builder.rs 的实际端到端 wiring 仍有小幅差距

调研后确认差距是结构性的：

| 级别 | 实现 | `builder.rs` 中是否调用 |
|------|------|--------------------------|
| L1 truncate | `synthia_context::truncate::truncate_messages` | ⚠️ 只在 LLM 前截断 Tool 消息，错误路径不触发 |
| L2 retry | `ErrorRecoveryCoordinator::handle_error` | ⚠️ 只在 LLM sampling 错误时调用，工具错误不触发 |
| L3 fallback | `try_l3_fallback` | ❌ **从未调用** |
| L4 auto-compact | `try_l4_compact` | ❌ **从未调用** |
| L5 reset | `ResetCoordinator::execute` | ❌ **从未调用** |

即：5 层恢复的**实现已经存在**，但 `stream_builder/builder.rs` 这个唯一的 agent loop 入口**没有 wire up** 这些实现。等于"实现已就绪的 dead code"。

### Current pain

1. **工具错误不触发恢复**：`builder.rs:531-541` 将 `tool_execute.execute` 的 `Err(e)` 转换为 `is_error: true` 的结果推入上下文，LLM 看到错误后只能"自求多福"。
2. **LLM 错误只到 L2**：`builder.rs:358` 调 `handle_error(L2Retry)`，但 `Escalated` 分支直接 `return` 结束 session，不进 L3-L5。
3. **无可观察性**：`AgentEvent` 枚举没有 `RecoveryApplied` 变体，外部无法感知"这次 L4 是怎么触发的"。
4. **Spec 承诺不兑现**：archive 的 5 个 specs (`specs/auto-compact-on-error`, `specs/session-reset`, `specs/tool-fallback`, `specs/tool-output-truncate`, `specs/tool-retry`) 描述的语义在代码中**未实现**。

### Stakeholders

- Agent loop 维护者：需要清晰的恢复路径
- Telemetry 团队：需要 `RecoveryApplied` 事件来统计 L4/L5 触发频率
- 上轮 archive specs 持有者：希望 spec 与实现对齐
- 多 agent 框架（synthia-server / synthia-cli）：通过 `AgentEvent` 流观察恢复

---

## Goals / Non-Goals

**Goals:**

- G1 — 把 L1-L5 cascade 显式 wire up 到 `stream_builder/builder.rs` 的两个错误入口（LLM sampling error + tool execution error）
- G2 — 修复 `RecoveryResult::Escalated` 在 `builder.rs` 中被误用为"终止 session"信号的问题
- G3 — 添加 `AgentEvent::RecoveryApplied` 变体，提供可观察的恢复事件
- G4 — 在 tool result 注入 context 前自动 truncate 超大输出（L1 的真正落地）
- G5 — 不修改 `error_recovery/*` 5 个模块的公共 API，保持上轮 archive 的 5 个 specs 仍然有效

**Non-Goals:**

- N1 — 不重新设计 5 层恢复结构（保留 Truncate/Retry/Fallback/Compact/Reset）
- N2 — 不引入新 trait 抽象（吸取上轮"过早抽象 = 浪费代码"教训）
- N3 — 不修改 `RecoveryResult` 枚举本身
- N4 — 不修改上轮 archive 的 5 个 specs
- N5 — 不实现 L5 的 `ToolState` / `Full` scope（继续返回 "not yet implemented" 即可）

---

## Decisions

### D1：cascade 入口位置 — LLM 错误 + 工具错误都走 cascade

- **选择**：在 `builder.rs` 的两个错误分支（`Err(e)` from sample、`Err(e)` from tool_execute）都调 `run_recovery_cascade`
- **理由**：cascade 已经参数化（接受 `error, tool_name, ctx, tracker, recovery, budget, provider, loop_detector, steering, reset`），足以覆盖两种错误源
- **已考虑 alternative**：分两个 cascade 函数（`run_llm_cascade` + `run_tool_cascade`） → 拒绝：重复代码 + 单一恢复逻辑 split-brain 风险

### D2：L1 truncate 的位置 — 在 tool result 注入前

- **选择**：在 `builder.rs:543` 的 `for result in &tool_results` 循环中，对每个 tool result（无论 is_error）做 `truncate_output`，超长则 truncate
- **理由**：L1 truncate 是 sync 写入 `ctx.messages` 的操作，不需要 cascade 的"升级"语义，inline 是最简单
- **已考虑 alternative**：把 truncate 放在 `StepToolExecute::execute` 内部 → 拒绝：与现有 `StepSample` 的 truncate 模式不一致；cascade 触发时需要原始 error 文本

### D3：`BuilderSteps` 持有 cascade 状态

- **选择**：`BuilderSteps` 新增两个字段：`reset: ResetCoordinator` 和 `failure_tracker: ConsecutiveFailureTracker`
- **理由**：与现有的 `recovery: ErrorRecoveryCoordinator` 对称；cascade 内部 `&mut` 借用方便
- **已考虑 alternative**：把 `ConsecutiveFailureTracker` 放在 `LoopContext` → 拒绝：cascade 接收 `&mut ConsecutiveFailureTracker` 的契约已定，迁移成本 > 收益

### D4：`AgentEvent::RecoveryApplied` 用 `level_number: u32` 而非 `RecoveryLevel` enum

- **选择**：事件中放 `level_number: u32` (1-5) + `tool_name: Option<String>` + `message: String` + `iteration: usize`
- **理由**：`RecoveryLevel` 没有 `Serialize` derive，添加 derive 会影响上轮 archive specs；`u32` 是稳定的 wire format
- **已考虑 alternative**：给 `RecoveryLevel` 加 `Serialize` → 拒绝：影响 archive specs 验证

### D5：`run_recovery_cascade` 调用是 async，需要 `builder.rs` 内部转 async

- **选择**：`stream! { ... }` 块本身是 async context（`async_stream`），可以直接 `.await`
- **理由**：`builder.rs` 已经在 `stream!` 块中，是 async 环境
- **已考虑 alternative**：把 cascade 改成 sync → 拒绝：L4 调 `compact_with_fallback` 本身是 async

### D6：cascade 失败后的兜底 — yield `SessionEnded(Error)` 然后 return

- **选择**：`RecoveryAction::FailFast(reason)` → `ctx.set_end_reason(SessionEndReason::Error(reason))` → `yield AgentEvent::SessionEnded` → `return`
- **理由**：与现有 `builder.rs:371-373` 的 L2 escalate 处理模式一致
- **已考虑 alternative**：进入 `panic!` → 拒绝：agent loop 应该 graceful degradation 而非崩溃

### D7：`ErrorRecoveryCoordinator::handle_error` 在 builder.rs 中不再单独使用

- **选择**：在 builder.rs 中**不直接调** `handle_error`，而是把整个错误路径交给 `run_recovery_cascade`
- **理由**：cascade 内部已经包含 retry 逻辑（通过 `RecoveryResult::Escalated(L2Retry)` 的特殊处理）；builder.rs 直接用 cascade 更简洁
- **已考虑 alternative**：保留 handle_error 单独调用的同时调 cascade → 拒绝：两个错误处理路径 split-brain 风险

### D8：`AgentRunConfig` 不新增字段

- **选择**：`run_recovery_cascade` 需要的 `budget`、`provider` 通过 `config.context_token_budget.as_ref()` 和 `config.compaction_provider.as_deref()` 传入
- **理由**：避免 config 表面膨胀；如果 `compaction_provider` 字段不存在，本 change 实施时再添加
- **已考虑 alternative**：直接添加 `compaction_provider: Option<Arc<dyn CompactionProvider>>` 字段 → 接受：本任务实施时**会**添加，因为 cascade 需要

---

## Risks / Trade-offs

- [Risk] **修改 `builder.rs` 引入新 bug** → Mitigation: TDD — 先写 integration test 验证 cascade 被调用，再 wire up
- [Risk] **`run_recovery_cascade` 是 async，stream block 中调用增加复杂度** → Mitigation: `async_stream` 已经支持 `.await`，无需特殊处理
- [Risk] **L1 truncate 与 L4 cascade 中的 truncate 重复** → Mitigation: L1 用 `synthia_context::truncate::truncate_output` (per-message)，L4 用 `compact_with_fallback` (full-context)，粒度不同不重复
- [Risk] **cascade 调用阻塞整个 agent loop** → Mitigation: L3/L4 失败 fallback 到 L5 reset；L5 失败进入 fail-fast + yield SessionEnded
- [Risk] **每次 tool result 都做 truncate 检查有性能开销** → Mitigation: `truncate_output` 是 O(n) 字符串扫描 + 阈值检查（默认 30KB），绝大多数 result < 1KB 走 fast path
- [Trade-off] **`AgentEvent::RecoveryApplied` 用 `level_number: u32` 而非 `RecoveryLevel` enum** → 接受理由：避免给 `RecoveryLevel` 加 `Serialize`，保持 archive specs 不变
- [Trade-off] **BuilderSteps 多 2 个字段（`reset`, `failure_tracker`）** → 接受理由：3 个相关字段聚在一起（recovery/reset/tracker）逻辑清晰，YAGNI 范围内
- [Trade-off] **不实现 L5 `ToolState` / `Full` scope** → 接受理由：上轮 spec 也只描述 `Conversation` scope；其他 scope 留给未来按需实现

---

## Migration Plan

本 change 不涉及部署变更（纯 crate 内部修改）。

部署步骤：
1. 在 worktree 中实现 5-7 个 micro-task（TDD-first）
2. 验证：`cargo test -p synthia-agent` 全绿
3. 验证：3 个新 integration test 覆盖 cascade 被调用的场景
4. Archive OpenSpec change 到 `openspec/changes/archive/2026-06-13-explicit-recovery-paths/`

Rollback 策略：
- 单一 commit，可直接 `git revert`
- 不涉及数据库迁移 / API endpoint 变化

验收条件：
- [ ] 工具错误触发 cascade（L3 fallback message 出现在 tool result 中）
- [ ] LLM 错误连续 3 次触发 L5 reset（messages 被清空）
- [ ] 超大 tool result (>30KB) 触发 L1 truncate，event `RecoveryApplied { level_number: 1 }` 被 yield
- [ ] 上轮 archive specs 仍然通过 `openspec validate`（不修改 `error_recovery/*` 公共 API）
- [ ] 新增 integration test 数量 ≥ 3

---

## Open Questions

1. **`AgentRunConfig` 是否需要新增 `compaction_provider` 字段？**
   - 推测：需要，因为 `run_recovery_cascade` 接受 `Option<&dyn CompactionProvider>`
   - 决议：实施时调研 `AgentRunConfig::default()` 的初始化路径，必要时添加
2. **`StepSample` 的 `truncate_cfg` 是否提升到 `BuilderSteps`？**
   - 现有：`truncate_cfg: TruncateConfig` 是 `StepSample` 私有字段
   - 推测：tool result truncate 复用同一 config
   - 决议：实施时再决定，**先**用 `TruncateConfig::default()`，与 StepSample 一致
3. **cascade 触发的 `AgentEvent::RecoveryApplied` 是否在每次 L1 truncate 都 yield？**
   - 推测：是的，可观察性优先
   - 决议：实施时验证高频 L1 是否造成 event spam，必要时加 throttle
