## Context

synthia 当前实现完整度 10 维度中 6 完整 / 4 部分 / 0 stub / 0 未实现（代码实测），在 P6 不信任 LLM 领域领先（5 层循环检测、tool_result_cleared_at idempotent marker、derive_subagent_permission 只继承 Deny、每 5 轮 self_reflect）。

但与三家生产级 AI agent 对比仍有差距：
- **opencode**（TS/Effect）：在 P1 前缀一致性（SystemContext typed source）、P2 Append-Only（Event Sourcing + Durable/Ephemeral 二分）、P7 可中断性（Steer vs Queue + TurnTransition defect）三个最高优先级原则领先
- **codex**（Rust）：在 P9 可观测性（LogDbLayer + CompactionAnalyticsAttempt + SpanAttributesProcessor）领先，Hooks 系统（10 事件 + 信任状态机）是 synthia 完全缺失
- **pi-mono**（TS）：在 P4 渐进降级（split-turn + file operation tracking）、P7 可中断性（file mutation queue）部分领先

**代码实测发现的静默风险**（必须先修）：
- H1：`Agent::run_stream` 不调用 `assemble_default_tool_orchestrator`，CLI/Examples 直调时静默失去 sandbox/approval/retry
- H4：`main_loop.rs:191-194` 只手动恢复 LoopContext 的 2/4 字段，`iteration` 重置为 0，已超 max_iterations 的会话恢复后又跑满一轮

**约束**：
- project_memory 是意图清单，与代码分开维护（用户决策 Q1）
- 先修复后引入新特性（用户决策 Q2）
- "More all as tool" 边界：系统提示词、权限策略、session 不 tool 化，其他可 tool 化（用户决策 Q3）
- SystemContext ~800 行工程量接受（用户决策 Q4）
- 不引入 SQLite（Phase 0 硬约束）
- 不做 seccomp（landlock 已是 fallback）

## Goals / Non-Goals

**Goals:**

1. 修复 H1/H4 静默风险，消除 run_stream 降级与 LoopContext iteration 丢失
2. 借鉴 opencode 的高 ROI 特性：Cache Policy 引用相等短路、Permission "always" 传播、Anchored Summary、TurnTransition defect、SystemContext typed source
3. 借鉴 codex 的高 ROI 特性：CompactionAnalyticsAttempt、SpanAttributesProcessor on_start
4. 借鉴 pi-mono 的高 ROI 特性：File mutation queue、Context overflow 检测
5. Tool 化改造：Guardian as Tool、Compaction as Tool（保留系统层兜底）
6. 所有改动遵循 agent_rule.md P1-P10 原则，特别是 P1 前缀一致性

**Non-Goals:**

1. 不引入 SQLite（推迟到 P3 决策）
2. 不做 seccomp（landlock 已是 fallback 的 fallback）
3. 不移植 codex Hooks 全量（20 个 schema 文件复杂度过高，只取精简版）
4. 不移植 codex Goal 扩展（synthia 5 层循环检测已覆盖死循环防护）
5. 不移植 codex LogDbLayer（与 Phase 0 硬约束冲突）
6. 不移植 codex ExecPolicy DSL（bash blacklist + sandbox 已覆盖 90%）
7. 不移植 opencode PTY ticket（synthia 非 PTY 优先场景）
8. 不移植 pi-mono Branch summarization（synthia session tree 功能未完整）
9. 不移植 pi-mono Output guard（synthia 非 TUI 优先）
10. 不 tool 化系统提示词、权限策略、session（用户决策 Q3）
11. 不更新 project_memory 数字（用户决策 Q1，project_memory 是意图清单）

## Decisions

### D1：H1 修复采用自动装配方案

- **选择**：`Agent::run_stream` 内部自动调用 `assemble_default_tool_orchestrator`（当 orchestrator 未注入时）
- **理由**：CLI/Examples 调用方不应承担注入 orchestrator 的责任，自动装配符合"简洁"原则
- **已考虑 alternative**：
  - Option B（panic fail-fast）：被拒，会破坏现有 CLI 调用路径
  - Option C（文档标注）：被拒，文档无法阻止误用

### D2：H4 修复采用 from_metadata 完整恢复

- **选择**：`main_loop.rs:191-194` 改用 `LoopContext::from_metadata(metadata)`，恢复全部 4 字段（iteration / end_reason / cumulative_tokens / context_token_limit）
- **理由**：API 已完整实现，主路径应复用；iteration 恢复后若已超 max_iterations，下个迭代自然触发 stop
- **已考虑 alternative**：
  - 只恢复 iteration 不恢复 end_reason：被拒，end_reason 影响 doom_loop 检测的连续计数
  - 不恢复 iteration（保持现状）：被拒，circuit_breaker 失效

### D3：Cache Policy 引用相等短路用 Arc::ptr_eq

- **选择**：在 `apply_cache_policy` 中，当 tools/system/messages 三者的 Arc 都 ptr_eq 时，直接返回原引用
- **理由**：零分配、零 cache invalidation，对齐 opencode `applyCachePolicy` 引用相等短路
- **已考虑 alternative**：
  - 内容 hash 比较：被拒，hash 计算本身有成本
  - 不做短路（保持现状）：被拒，每次都重建 request 破坏 P1

### D4：File mutation queue 用 tokio::sync::Mutex + per-filepath 粒度

- **选择**：`Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>`，per-filepath 串行化，完成后清理 Map
- **理由**：tokio::sync::Mutex 适配 async runtime，per-filepath 粒度避免全局阻塞，清理防内存泄漏
- **已考虑 alternative**：
  - std::sync::Mutex：被拒，持有跨 await 点会阻塞 runtime
  - 全局单 Mutex：被拒，不同文件串行化过度
  - dashmap：被拒，引入新依赖

### D5：Permission "always" 传播扫描同 session pending

- **选择**：用户对某 permission 选 "always" 后，扫描同 session 所有 pending 请求，新规则下 resources 全部 allow 时自动 resolve；"reject" 级联终止同 session 所有 pending
- **理由**：避免用户连续看到 5 个相似 permission 弹窗，UX 优化
- **已考虑 alternative**：
  - 不传播（保持现状）：被拒，UX 差
  - 全局传播（跨 session）：被拒，跨 session 隔离是安全要求

### D6：Anchored Summary 采用 8 段式模板 + 增量更新

- **选择**：Goal / Constraints / Progress(Done/InProgress/Blocked) / Key Decisions / Next Steps / Critical Context / Relevant Files 八段；有 previousSummary 时用 "Update the anchored summary" prompt
- **理由**：结构稳定避免 summary drift，增量更新比重新生成更节省 token
- **已考虑 alternative**：
  - 自由格式 summary：被拒，LLM 输出不可控
  - 每次重新生成：被拒，token 浪费 + drift

### D7：Context overflow 检测用 21 正则 + silent overflow

- **选择**：21 个 provider-specific 正则 + 3 个排除（throttling/rate limit/too many requests）+ silent overflow 检测（usage.input + cacheRead > contextWindow）
- **理由**：生产环境 silent overflow 是隐患，provider 错误消息覆盖面广
- **已考虑 alternative**：
  - 只检测 silent overflow：被拒，部分 provider 不返回 usage
  - 只用正则：被拒，silent overflow 无 error message

### D8：TurnTransition defect 用 Result<_, ControlFlow> 近似

- **选择**：Rust 中用 `Result<TurnOutput, ControlFlow<TurnTransition>>` 表达，外层 match ControlFlow 处理重试
- **理由**：Rust 无 Effect-TS 的 typed error channel，ControlFlow 是 std 类型，语义匹配（Continue/Break）
- **已考虑 alternative**：
  - 自定义 error type：被拒，增加类型复杂度
  - panic + catch_unwind：被拒，panic 不应用于控制流

### D9：CompactionAnalyticsAttempt 追踪 5 字段

- **选择**：追踪 `active_context_tokens_before` / `trigger` / `reason` / `implementation` / `phase`，emit 为 OTel span attributes
- **理由**：补强 P9 pruning_stage_distribution 指标，对齐 codex
- **已考虑 alternative**：
  - 只追踪 trigger：被拒，无法定位性能瓶颈
  - 追踪全量字段：被拒，过度遥测增加成本

### D10：SpanAttributesProcessor 剥离 Statsig 分支

- **选择**：移植 codex SpanAttributesProcessor，on_start 注入 6 个属性（session.id / user.id / agent.id / turn.id / gen_ai.system / gen_ai.request.model），剥离 Statsig exporter
- **理由**：synthia 明确不做 Statsig，OTLP gRPC/HTTP 已足够
- **已考虑 alternative**：
  - 保留 Statsig 分支：被拒，引入不必要依赖
  - 不移植：被拒，P1-5 roadmap 已要求

### D11：SystemContext typed source 用 trait + Eq

- **选择**：定义 `Source` trait（key / load / baseline / update / removed），`Snapshot` 持久化 encoded value，reconcile 用 `PartialEq` 比较返回 `Unchanged / Updated / ReplacementReady / ReplacementBlocked`
- **理由**：Rust 无 Effect-TS Schema/Equivalence 一等公民，trait + Eq 是最简等价
- **已考虑 alternative**：
  - serde_json::Value 比较：被拒，弱类型
  - 引入 serde-diff crate：被拒，新依赖
- **注意**：SystemContext 不 tool 化（属于系统提示词，用户决策 Q3）

### D12：Guardian as Tool 保留每 5 轮兜底

- **选择**：暴露 `self_reflect` tool，LLM 在需要时主动调用；同时保留每 5 轮自动触发兜底（防 LLM 不调用）
- **理由**：这是"more all as tool"与"P6 不信任 LLM"的折衷
- **已考虑 alternative**：
  - 完全 tool 化（无兜底）：被拒，LLM 可能不调用
  - 不 tool 化（保持现状）：被拒，违背"more all as tool"

### D13：Compaction as Tool 保留自动触发兜底

- **选择**：暴露 `compact_context` tool，tool description 中提供 token 数 hints；同时保留上下文超阈值自动触发兜底
- **理由**：同 D12，LLM 自主 + 系统兜底
- **已考虑 alternative**：
  - 完全 tool 化：被拒，LLM 无法感知 token 数
  - 不 tool 化：被拒，违背"more all as tool"

## Risks / Trade-offs

- [Risk] SystemContext ~800 行工程量大，可能引入回归 → Mitigation: 分阶段实施，先 trait + 单 source（environment），再扩展
- [Risk] File mutation queue 死锁（同文件嵌套调用）→ Mitigation: per-filepath 粒度细，且 tokio::sync::Mutex 可跨 await；测试覆盖嵌套场景
- [Risk] Guardian as Tool LLM 不调用 → Mitigation: 保留每 5 轮兜底（D12）
- [Risk] Compaction as Tool LLM 不调用 → Mitigation: 保留自动触发兜底（D13）
- [Risk] Anchored Summary 模板对不同 provider 兼容性 → Mitigation: 测试覆盖 Anthropic/OpenAI/Google 三家
- [Risk] TurnTransition ControlFlow 语义不完全等价 Effect defect → Mitigation: 文档标注差异，单元测试覆盖重试场景
- [Risk] Permission "always" 传播可能误 resolve → Mitigation: 严格匹配 resources 全部 allow 条件，测试覆盖边界
- [Risk] Context overflow 21 正则维护成本 → Mitigation: 正则集中管理，加注释标注来源
- [Trade-off] 不移植 codex Hooks 全量 → 接受理由：synthia"简洁"原则优先，精简版（PreToolUse/PostToolUse + trust）已覆盖 80% 场景
- [Trade-off] 不引入 SQLite → 接受理由：Phase 0 硬约束，文件系统 + ripgrep 已足够
- [Trade-off] SystemContext 不 tool 化 → 接受理由：用户决策 Q3，系统提示词属于系统层

## Migration Plan

5 阶段渐进，每阶段独立可验证：

**阶段 1：修复静默风险**（~40 行）
- D1 H1 自动装配 + D2 H4 from_metadata 恢复
- 验收：`cargo test -p synthia-agent` 通过 + 新增 H1/H4 回归测试
- Rollback：revert 阶段 1 commit

**阶段 2：即时高 ROI 小改动**（~100 行）
- D3 引用相等短路 + D4 file mutation queue
- 验收：`cargo test -p synthia-provider -p synthia-tool` 通过 + 引用短路单测 + file mutation queue 并发测试
- Rollback：revert 阶段 2 commit

**阶段 3：中期补强**（~600 行）
- D5 Permission "always" 传播 + D6 Anchored Summary + D7 Context overflow 检测
- 验收：`cargo test -p synthia-permission -p synthia-context -p synthia-provider` 通过 + 各特性单测
- Rollback：revert 阶段 3 commit

**阶段 4：长期架构补强**（~1550 行）
- D8 TurnTransition + D9 CompactionAnalytics + D10 SpanAttributesProcessor + D11 SystemContext
- 验收：`cargo test --workspace` 通过 + otel feature 测试 + SystemContext 集成测试
- Rollback：revert 阶段 4 commit（注意 SystemContext 涉及 system prompt 构建，需验证回归）

**阶段 5：Tool 化改造**（~300 行）
- D12 Guardian as Tool + D13 Compaction as Tool
- 验收：`cargo test -p synthia-agent -p synthia-tool` 通过 + tool 注册测试 + 兜底机制测试
- Rollback：revert 阶段 5 commit

**总体验收**：`cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all` 零警告 + `openspec validate --strict` 通过

## Open Questions

1. **SystemContext Source trait 的 Eq 实现粒度**：每个 source 自定义 Eq，还是用 serde_json::Value 笼统比较？（设计阶段需决定）
2. **TurnTransition ControlFlow 的重试上限**：最多重试几次防止无限循环？（建议 3 次）
3. **Anchored Summary 模板的 provider 兼容性**：是否需要针对不同 provider 微调 prompt？（需测试验证）
4. **File mutation queue 与 apply_patch 的集成点**：在 ToolAdapter 层还是 tool 内部？（建议 ToolAdapter 层，统一覆盖）
5. **Guardian as Tool 的兜底轮次**：保持每 5 轮，还是动态调整？（建议保持 5 轮，与现状一致）
6. **Compaction as Tool 的 token hints 格式**：在 tool description 中如何暴露 token 数？（建议 "<context_tokens>X</context_tokens>" XML 标签）
