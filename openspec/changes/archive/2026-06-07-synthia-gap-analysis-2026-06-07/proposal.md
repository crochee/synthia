# 1. Why

经过对 Synthia (Rust) / OpenCode (TypeScript) / Codex-Rust 三个项目的三方对抗性分析（详见 `brainstorm.md`），Synthia 当前的真实状况是：

- **骨架完整**（compaction / pruning / loop detection / hook / guardian / 持久化 / 多 agent / 文件 skill / telemetry 全具备）
- **横向收敛严重不足**——同一概念在 3-5 个 crate 中各自实现，最高原则 P1（KV-Cache 前缀一致性）和 P6（不信任 LLM）落不到地
- **存在 3 个 Critical 硬 bug**：并发调度失效、prefix 不可观测、`trim_to_budget` O(n²)

`brainstorm.md` 已记录从"差距清单"→"用户选择战略"→"执行粒度"的完整决策链。本 change 落地"战略 1：基础收敛"，聚焦四个 P0 能力补齐：

| # | 能力 | 目标 |
|---|------|------|
| C1 | Prompt 组装收敛 | 5 套 prompt 构建路径 → 1 套 `ContextAssembler` |
| C2 | 工具并发安全 | `Tool` trait 补 `is_concurrency_safe` + 修 `agent/step.rs` 硬编码 bug |
| C3 | PrefixTracker 真正接入 | 每次 LLM 调用前后记录 prefix_hash，cache 命中率可观测 |
| C4 | Token 计数单一化 | 15 个文件 1 套 token 计数 trait |

# 2. What Changes

**Prompt 组装收敛 (C1)**
- From: 5 套实现并存 — `context::assembler` (876 行, 全功能) / `context::prompt::builder` / `context::system_context` / `agent::stream_builder::context_builder` (33 行, 最简) / `agent::builder` 占位
- To: 全部走 `ContextAssembler` 一种入口；`StreamBuilder` 不再私有 `ContextBuilder`
- Reason: 同一概念 5 套实现，5 个不同调用点；新 section 加完要 5 处改；p1 (prefix 稳定) 难以保证
- Impact: 内部重构，public API `ContextAssembler` 不变；agent crate 删一个 `ContextBuilder` 私有结构

**Tool 并发安全 (C2)**
- From: `synthia-tool/src/traits.rs::Tool` 没有 `is_concurrency_safe`；`agent/step.rs:194-200` 硬编码 `false`；`requires_permission()` 被丢弃
- To: `Tool` trait 新增 `is_concurrency_safe() -> bool` 默认 `false`；所有 builtin 显式声明（`read=parallel-safe`, `glob=parallel-safe`, `grep=parallel-safe`, `bash=unsafe`, `write=unsafe`）
- Reason: `parallel_task_dispatch_test` 通过，但实际调度器拿到 `false` 全部走 Serial，parallel 失效
- Impact: 公开 trait 扩方法，向后兼容（旧实现自动默认 `false`）

**PrefixTracker 真正接入 (C3)**
- From: `context::prefix_tracker` 全部是孤岛 API（`compute_prefix_hash` / `record_prefix` / `stability_ratio`），无任何调用方；`telemetry::context_trace` 另写一套
- To: `StreamBuilder::run` 每次 LLM 调用前调 `prefix_tracker.record(system_snapshot)`，调用后调 `prefix_tracker.record_post(response)`，差异上报 `telemetry::prefix_stability_ratio` 指标
- Reason: P1 prefix 稳定是最高约束，但当前不可观测 = 不可优化；OpenCode `llm.ts:103-128` 2 段式 + Codex `compact.rs:204-218` 是可参考实现
- Impact: 新增一项 `prefix_stability_ratio` 指标（OTel `codex.prefix.stability` 风格）；无行为变化

**Token 计数单一化 (C4)**
- From: 15 个文件实现 token 估算，精确度差异 5-10×；`context::traits::estimate_message_tokens` (pub) 和 `context::estimator::estimate_message_tokens` (pub(crate)) 功能完全相同
- To: 收所有估算到 `synthia-provider::TokenCounter` trait 一个方法 `count_messages(&[Message]) -> u32`；`synthia-context` 只持有 `Arc<dyn TokenCounter>` 引用，不实现
- Reason: 15 处实现 → 5-10× 精确度差 → compaction 触发时机飘忽
- Impact: `synthia-context` 增加对 `synthia-provider` 的依赖（已是 workspace 内部依赖），public API 变化极小（`estimate_message_tokens` 仍 pub，但变 thin wrapper）

# 3. Capabilities

## New Capabilities

- `convergent-prompt-assembly`：将 5 套 prompt 构建路径收敛为 `ContextAssembler` 单入口；agent 私有 `ContextBuilder` 删除
- `tool-concurrency-trait`：`Tool` trait 扩 `is_concurrency_safe`，所有 builtin 显式声明；`agent/step.rs` 硬编码 bug 修复
- `prefix-tracker-wiring`：`PrefixTracker` 接入 `StreamBuilder` LLM 调用生命周期；新增 `prefix_stability_ratio` 指标
- `token-counter-unification`：所有 token 估算统一走 `synthia-provider::TokenCounter` trait；删除 `context::estimator` 重复

## Modified Capabilities

- `tool-execution`（已有）：需补充 `is_concurrency_safe` 在并发调度路径的使用约束
- `context-management`（已有）：需补充 `ContextAssembler` 作为唯一入口
- `token-budget-observability`（已有）：需新增 `prefix_stability_ratio` 指标
- `precise-token-counting`（已有）：需删除 `context::estimator` 模块的重复实现

# 4. Impact

**代码改动**：
- `crates/synthia-tool/src/traits.rs` + 4 个 builtin 文件（read/glob/grep/bash/write）— 新增方法
- `crates/synthia-agent/src/agent/step.rs:194-200` — 修硬编码 bug
- `crates/synthia-agent/src/stream_builder/builder.rs` + `context_builder.rs` — 改用 ContextAssembler，删除私有 ContextBuilder
- `crates/synthia-context/src/assembler.rs` — 增强为唯一入口（已存在的 876 行）
- `crates/synthia-context/src/{traits.rs, estimator.rs, injector.rs, prompt_layer.rs, compactor.rs, compactor/*.rs, prompt/*, compaction_service.rs, system_context.rs}` — 收敛 token 计数
- `crates/synthia-context/src/prefix_tracker.rs` — 接入 stream_builder
- `crates/synthia-telemetry/src/{context_trace.rs, agent_metrics.rs}` — 接收 prefix hash + 上报 stability ratio
- `crates/synthia-provider/src/{token_counter.rs, openai.rs, anthropic.rs}` — 暴露单一 trait

**依赖关系**：
- `synthia-context` → `synthia-provider`（增加 trait 引用；同 workspace，无循环）
- `synthia-telemetry` → `synthia-context::PrefixTracker`（已存在的反向引用需梳理）

**测试**：
- 新增 unit tests ≥ 24 个（4 个能力各 ≥ 6）
- 新增 integration tests ≥ 4 个（端到端：prefix 稳定性、并发调度、token 计数收敛、prompt 组装）
- 保留所有现有 e2e tests 通过

**性能影响**：
- `trim_to_budget` 优化 O(n²)→O(n log n) 是 critical bug 顺带修
- `prefix_tracker` 是哈希 + 单比较，开销 < 1ms/turn，可忽略
- token 计数 trait 抽象增加 1 次虚函数调用，可忽略

**行为变化**：
- 公开 API：`Tool::is_concurrency_safe` 是新增默认方法（向后兼容）
- 内部行为：parallel tool 调度恢复（之前 Serial，现在按 trait 声明走）
- 观测：新指标 `prefix_stability_ratio` 暴露给 telemetry

# 5. Non-Goals

明确不在本次 change 范围内（避免范围蔓延）：

- **不修** `assembler::trim_to_budget` 的 O(n²) 算法本身（critical 但属于 "P0 性能修复" change，下一个迭代）
- **不修** `read_history` 无界 Vec / bash UTF-8 panic / 4 套 truncate 收敛（属于 "P1 安全稳定" change）
- **不修** `pruning::hard_clear` 静默丢内容（属于 "P8 不丢信息" change，需 event log 改造）
- **不动** Permission 粒度升级（argv token / regex）—— 已在 `permission-merge` spec 中部分处理
- **不合并** 3 套 compaction（`context::Compactor` / `memory::Compactor` / `compaction_service`）—— 单独 change
- **不动** Guardian / Rollout / Plugin —— 已有 spec 覆盖

# 6. Risks

- [R1] `Tool` trait 扩方法会破坏所有 `impl Tool` 的下游 → **缓解**：默认实现返回 `false`，旧代码无感
- [R2] 收敛 prompt 组装可能漏掉某条调用路径的语义差异 → **缓解**：每个旧路径先写 e2e test 锁定行为再删
- [R3] `synthia-context → synthia-provider` 依赖方向变化可能引入循环 → **缓解**：同 workspace 内可解决；用 `Arc<dyn TokenCounter>` 注入
- [R4] `prefix_tracker` 接入后若发现 cache 命中率确实很低（<50%），会暴露 P1 未落地的真实问题 → **缓解**：这是好事，暴露问题才能优化；不阻塞本 change
- [R5] 4 个能力并发实施可能 conflict → **缓解**：tasks.md 排序强制 C4 → C2 → C1 → C3 顺序

# 7. Migration Plan

**阶段 1：trait 引入（无破坏）**
- C2 增 `Tool::is_concurrency_safe` 默认方法
- C4 增 `synthia-provider::TokenCounter` trait
- 全部 `impl Tool` 走默认（`false`），全 `estimate_message_tokens` 仍可用

**阶段 2：迁移 + 验证**
- C2 builtin 显式标注
- C4 context crate 改用 trait，删除 `estimator` 重复
- 全部 cargo test 通过

**阶段 3：收敛（可能 break 内部）**
- C1 删除 agent 私有 `ContextBuilder`，改用 `ContextAssembler`
- C3 `PrefixTracker` wire 到 `StreamBuilder::run`

**回滚**：
- 每个能力独立 commit，可单独 revert
- 公开 API 完全向后兼容，回滚无用户感知

# 8. Open Questions

- OQ1: `ContextAssembler` 是否需要新加 public method 暴露"section by name"查询？（stream_builder 自反思用）— 倾向加，spec 里写出
- OQ2: `prefix_stability_ratio` 的窗口大小（rolling 多少 turn）？— 倾向 20 turn，spec 里给出
- OQ3: `TokenCounter` 的 `count_messages` 是单条 message 还是整个 batch？— 倾向 batch，调用方循环更简单
