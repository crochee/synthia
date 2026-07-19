## Why

synthia 代码实测 10 维度中 6 完整 / 4 部分 / 0 stub，在 P6 不信任 LLM 领域领先（5 层循环检测、tool_result_cleared_at idempotent marker、derive_subagent_permission Deny-only 继承、每 5 轮 self_reflect）。但对比 opencode/codex/pi-mono 仍有差距：H1 `Agent::run_stream` 静默降级丢失 sandbox/approval/retry，H4 `main_loop.rs:191-194` LoopContext iteration 未恢复致 circuit_breaker 失效；P1 前缀一致性缺 SystemContext typed source，P9 可观测性缺 CompactionAnalyticsAttempt + SpanAttributesProcessor on_start，P7 可中断性缺 TurnTransition defect + file mutation queue。本提案一次性补齐高 ROI 差距，遵循"先修复后引入新特性"原则，5 阶段渐进交付 ~3500 行，所有改动遵循 agent_rule.md P1-P10。

## What Changes

### 阶段 1：修复静默风险（前置必做）

**Agent run_stream 编排装配**
- From: `Agent::run_stream` 不调用 `assemble_default_tool_orchestrator`，CLI/Examples 直调时静默失去 sandbox/approval/retry
- To: orchestrator 未注入时自动调用 `assemble_default_tool_orchestrator`
- Reason: H1 静默降级是生产单点失效
- Impact: non-breaking，CLI/Examples 调用方零改动

**LoopContext 恢复完整性**
- From: `main_loop.rs:191-194` 手动恢复 2/4 字段，iteration 重置为 0
- To: 改用 `LoopContext::from_metadata(metadata)` 恢复全部 4 字段
- Reason: H4 iteration 丢失致 circuit_breaker 失效，已超 max_iterations 的会话恢复后又跑满一轮
- Impact: non-breaking，恢复语义对齐 API 设计

### 阶段 2：即时高 ROI 小改动

**Cache Policy 引用相等短路**
- From: 每次 `apply_cache_policy` 重建 request，破坏 P1 前缀一致性
- To: tools/system/messages 三者 Arc 都 `ptr_eq` 时直接返回原引用
- Reason: 零分配、零 cache invalidation，对齐 opencode `applyCachePolicy`
- Impact: non-breaking，性能优化

**File mutation queue per-filepath 串行化**
- From: 文件工具无串行化，并发写同一文件可能损坏
- To: `Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>` per-filepath 串行化，完成后清理
- Reason: 对齐 pi-mono file-mutation-queue，realpath 解析 key 处理 symlink
- Impact: non-breaking，并发语义增强

### 阶段 3：中期补强

**Permission "always" 传播**
- From: 用户选 "always" 后同 session pending 请求仍逐个弹窗
- To: 扫描同 session pending，新规则下 resources 全部 allow 时自动 resolve；"reject" 级联终止同 session 所有 pending
- Reason: 对齐 opencode，避免连续 5 个相似 permission 弹窗
- Impact: non-breaking，UX 优化

**Anchored Summary 8 段式模板**
- From: 自由格式 summary，LLM 输出不可控 + 每次重新生成
- To: Goal / Constraints / Progress(Done/InProgress/Blocked) / Key Decisions / Next Steps / Critical Context / Relevant Files 八段；有 previousSummary 时用 "Update the anchored summary" prompt
- Reason: 对齐 opencode，结构稳定避免 summary drift，增量更新节省 token
- Impact: non-breaking，context compaction 输出格式变更

**Context overflow 检测**
- From: 仅依赖 provider error message，silent overflow 无感知
- To: 21 个 provider-specific 正则 + 3 个排除（throttling/rate limit/too many requests）+ silent overflow 检测（usage.input + cacheRead > contextWindow）+ 孤儿 tool call 合成空 result
- Reason: 对齐 pi-mono，生产环境 silent overflow 是隐患
- Impact: non-breaking，错误检测增强

### 阶段 4：长期架构补强

**TurnTransition defect 乐观重试**
- From: turn 切换无 defect 通道，冲突直接失败
- To: `Result<TurnOutput, ControlFlow<TurnTransition>>` 表达，外层 match ControlFlow 处理重试（上限 3 次）
- Reason: 对齐 opencode Effect.die + catchDefect，Rust 用 ControlFlow 近似
- Impact: non-breaking，重试语义增强

**CompactionAnalyticsAttempt 遥测**
- From: pruning 仅记录 stage，无 attempt 级遥测
- To: 追踪 `active_context_tokens_before` / `trigger` / `reason` / `implementation` / `phase` 5 字段，emit 为 OTel span attributes
- Reason: 对齐 codex，补强 P9 pruning_stage_distribution 指标
- Impact: non-breaking，遥测增强

**SpanAttributesProcessor on_start**
- From: OTel span 属性在 end 时注入，丢失 start 时刻上下文
- To: on_start 注入 6 个属性（session.id / user.id / agent.id / turn.id / gen_ai.system / gen_ai.request.model），剥离 Statsig exporter
- Reason: 对齐 codex，P1-5 roadmap 已要求
- Impact: non-breaking，otel feature 内增强

**SystemContext typed source**
- From: 系统提示词构建无 typed source 管理，cache_breaker 已移除后无正向方案
- To: `Source` trait（key / load / baseline / update / removed）+ `Snapshot` 持久化 + reconcile 用 `PartialEq` 比较，返回 Unchanged / Updated / ReplacementReady / ReplacementBlocked
- Reason: 对齐 opencode SystemContext，最大架构差距，~800 行
- Impact: non-breaking，系统提示词构建路径重构（不 tool 化，属系统层）

### 阶段 5：Tool 化改造

**Guardian as Tool**
- From: self_reflect 仅每 5 轮硬编码触发
- To: 暴露 `self_reflect` tool 供 LLM 主动调用 + 保留每 5 轮兜底
- Reason: 用户决策 Q3 "more all as tool"，与 P6 不信任 LLM 折衷
- Impact: non-breaking，新增 tool 注册

**Compaction as Tool**
- From: compaction 仅上下文超阈值自动触发
- To: 暴露 `compact_context` tool + tool description 中提供 `<context_tokens>X</context_tokens>` hints + 保留自动触发兜底
- Reason: 同上，LLM 自主 + 系统兜底
- Impact: non-breaking，新增 tool 注册

## Capabilities

### New Capabilities

- `agent-resume-correctness`: H1 run_stream 自动装配 tool orchestrator + H4 LoopContext from_metadata 完整恢复，消除会话恢复静默风险
- `cache-policy-short-circuit`: Arc::ptr_eq 引用相等检查，tools/system/messages 三者未变时零分配返回原引用
- `file-mutation-queue`: per-filepath tokio::sync::Mutex 串行化，realpath 解析 key 处理 symlink，完成后清理 Map
- `permission-always-propagation`: 同 session pending 请求扫描，"always" 自动 resolve、"reject" 级联终止
- `anchored-summary`: 8 段式模板（Goal/Constraints/Progress/Decisions/Next Steps/Critical Context/Relevant Files）+ 增量更新 prompt
- `context-overflow-detection`: 21 provider-specific 正则 + 3 排除 + silent overflow 检测 + 孤儿 tool call 合成空 result
- `turn-transition-control`: Result<_, ControlFlow> 表达 defect，外层 catchDefect 等价 match，重试上限 3 次
- `compaction-telemetry`: CompactionAnalyticsAttempt 5 字段追踪（active_context_tokens_before / trigger / reason / implementation / phase）
- `otel-span-processor`: SpanAttributesProcessor on_start 注入 6 属性，剥离 Statsig 分支，OTLP gRPC/HTTP exporter
- `system-context-source`: Source trait + Snapshot 持久化 + reconcile/replace 状态机（Unchanged/Updated/ReplacementReady/ReplacementBlocked）
- `guardian-tool`: self_reflect 暴露为 tool + 每 5 轮兜底机制
- `compaction-tool`: compact_context 暴露为 tool + tool description token hints + 自动触发兜底

### Modified Capabilities

无（openspec/specs 当前为空，所有能力均为新建）

## Impact

**受影响 crates**：
- `synthia-agent` — D1 run_stream 自动装配、D2 main_loop LoopContext 恢复、D12 Guardian tool 注册、D13 Compaction tool 注册
- `synthia-context` — D6 Anchored Summary 模板、D9 CompactionAnalyticsAttempt、D11 SystemContext Source trait + Snapshot
- `synthia-provider` — D3 Cache Policy 引用相等短路、D7 Context overflow 检测（21 正则 + silent overflow）
- `synthia-tool` / `synthia-tool-exec-base` — D4 File mutation queue（ToolAdapter 层集成）、D12/D13 tool 注册
- `synthia-permission` — D5 "always" 传播到同 session pending
- `synthia-telemetry` — D9 CompactionAnalyticsAttempt span attributes、D10 SpanAttributesProcessor on_start
- `synthia-guardian` — D12 self_reflect tool 暴露
- `synthia-tool-orchestrator` — D1 复用 `assemble_default_tool_orchestrator`

**API 变更**：
- 新增 `Source` trait + `Snapshot` 类型（synthia-context）
- 新增 `CompactionAnalyticsAttempt` struct（synthia-telemetry）
- 新增 `SpanAttributesProcessor`（synthia-telemetry，otel feature）
- 新增 `self_reflect` / `compact_context` tool 定义（synthia-tool）
- `LoopContext::from_metadata` 调用点从手动 2 字段恢复改为完整 4 字段（synthia-agent main_loop.rs）
- `Agent::run_stream` 增加 orchestrator 缺失时自动装配分支

**依赖**：
- 无新增第三方依赖（tokio::sync::Mutex、Arc::ptr_eq、std::ops::ControlFlow 均为 stdlib/tokio 已有）
- otel feature 仍为可选 cargo feature，默认禁用

**系统影响**：
- 系统提示词构建路径在 D11 后由 SystemContext Source 驱动（不 tool 化，属系统层）
- 文件工具并发语义在 D4 后变为 per-filepath 串行化
- Context compaction 在 D6 后输出 8 段式结构化 summary
- Permission UX 在 D5 后减少弹窗数量

**约束**：
- 不引入 SQLite（Phase 0 硬约束）
- 不做 seccomp（landlock 已是 fallback）
- 不 tool 化系统提示词、权限策略、session（用户决策 Q3）
- 不更新 project_memory 数字（用户决策 Q1，project_memory 是意图清单与代码分开维护）
