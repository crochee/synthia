# Synthia 现状批判性复核（对抗性审视）

> 对照 `openspec/changes/_inbox/synthia-current-architecture.md`（baseline 2026-07-12）+ 5 份 `.omc/research/` 审查 + 代码现场验证。
> 目的：为后续 OpenSpec change 的"现状与动机"段提供对抗性优先级与可行性判断。

---

## 1. Baseline G1–G20 的对抗性重排序

**评分维度**：
- **影响 (I)**：直接破坏 P1–P10 原则，或触发功能失效。
- **成本 (C)**：单 PR (<200 LOC, 1 crate) / 跨 crate / 跨接口破坏性。
- **紧迫性 (U)**：现在不修会否出生产事故。
- **借鉴明确性 (R)**：opencode/codex/pi-mono 有现成可直接抄的方案。

### 1.1 严重 / 必须修（Phase 1）

| # | Gap | I | C | U | R | 评估 | 证据 |
|---|-----|---|---|---|---|------|------|
| **G1** | `AgentRunConfig` 9 个字段在 `main_loop` 解构为 `_` | 高 | 中 | 高 | 中 | **保留 baseline 判断**。但 baseline 把"丢弃"当成同质问题：实际 `model_router` 是被 `sample_llm_and_cascade` 消费的（`iteration.rs`），`approval_service` / `guardian_coordinator` / `tool_orchestrator` 是被 `StepToolExecute` 消费的（main_loop 注释明示）。**真正"未消费"的是**：`subagent_session_factory`（注释自承 "Not yet consumed"）、`fork_policy`、`context_assembler`、`extension_manager`。**应按"未消费"细化**，不要一锅端。 | `main_loop.rs:124-162`；`run_config.rs:38-100` |
| **G3** | `CachePolicyApplier` 缺 `user_id` namespace | 高 | 低 | **极高** | 高 | **升为 Phase 1 首位**。这是 baseline 自承的"硬约束违例"——`run_config.rs:43-47` 注释引用 `user-id-namespace-and-bash-permission-gate` OpenSpec change，但 `cache_policy::CachePolicyApplier::apply` 当前签名（baseline §7.1）无 `user_id`。**跨用户 cache 污染是合规事故**，不是性能问题。 | baseline §7.2；`run_config.rs:43-47` 注释 |
| **G2** | `FailPolicy` 默认 `FailOpen` 与硬约束相悖 | 高 | 极低 | 中 | 高 | **保留**。toolsystem-review 也独立指出 "Hook 失败默认放行，可能绕过安全检查"。**单行默认值改动 + 一个测试即可**。 | baseline §2.2；`agent-logic-review.md:186-192` |
| **G4** | `AgentHook` 5/7 未触发（只剩 `on_before_llm` / `on_after_llm`） | 高 | 中 | 中 | 中 | **保留**。`main_loop.rs:439-441` / `588-590` 是仅有的两个 fire 点。`StepToolExecute`（`tool_execution.rs`）没有 hook 注入点——这是真正要补的接口。 | baseline §2.2；main_loop.rs |
| **G5** | Skill snapshot 不在 `PrefixTracker::compute_hash_bytes` | 高 | 中 | 中 | 低 | **保留**。`tracker.rs:77-87` 是三段拼接，缺第四段。**这是 P1 "skill 激活 → system prompt 变 → hash 不变"的精确漏洞**。opencode 没有等价物（pi-mono 也没有四段 hash），需要重新设计。 | baseline §4.2；`tracker.rs:77-87` |

### 1.2 中等 / 应该修（Phase 2）

| # | Gap | I | C | U | R | 评估 | 证据 |
|---|-----|---|---|---|---|------|------|
| **G6** | 双 hook 系统（`AgentHook` vs `HookRunner`）并存 | 中 | 高 | 低 | 中 | **降级**。这是架构债务但不是事故源——`HookRunner` 限定 plugin manifest 外部子进程（`hook_runner/execute.rs`），`AgentHook` 是 in-process 生命周期，**语义不重合**。完整统一涉及 `synthia-plugin` 接口破坏，**可在 Phase 2 末段或 Phase 3 处理**。 | baseline §3.2 |
| **G11** | `previous_summary` 截断 4000 char hardcoded | 中 | 低 | 中 | 高 | **保留**。opencode 用 `SummaryConfig { max_tokens: usize }`。单字段替换。 | baseline §10.2 |
| **G12** | `transition_to` 每次 save_metadata | 中 | 低 | 低 | 高 | **保留**。dirty-flag + batch flush 是教科书操作。 | baseline §8.2 |
| **G13** | `StartApprovalTimeout` 仅 logging | **高** | 低 | 中 | 中 | **升一档**。`StateEnterEffect::StartApprovalTimeout` 真实定时器未启动，**approval 路径上 LLM 真的能"卡死"**。这是 P7 中断性硬伤。 | baseline §8.2；`machine.rs:106-110` |
| **G14** | `pruning/stages.rs` 三阶段未串联 | 中 | 中 | 中 | 低 | **保留**。`do_compact_step` 已知接 compact，未接 pruning。 | baseline §10.2 |
| **G15** | `PluginManifest.hooks: serde_json::Value` 无 schema | 低 | 低 | 低 | 高 | **降为 Phase 3**。schemars 是 nice-to-have；运行时已有 9 种 `PluginError` 兜底。 | baseline §10.3 |
| **G19** | `parent_depth` 无硬上限 | 中 | 极低 | 中 | 中 | **升为 Phase 2**。`subagent/config.rs` 已有 `parent_depth` 参数但缺 `max_subagent_depth`。递归爆栈是 P7 中断性 + P6 不信任 LLM 双违反。单 PR 可修。 | baseline §6.2 |

### 1.3 低 / 可做（Phase 3）

| # | Gap | 评估 | 证据 |
|---|-----|------|------|
| **G7** | `is_concurrency_safe` 默认 `false` | **过分夸大**。保守默认是正确决策——ReadTool 在生产里被恶意 LLM 操纵并发可能耗尽 FD。opencode 也是保守默认。**应做**：写一个并发安全矩阵 audit 而非改默认值。 | baseline §1.2 |
| **G8** | `CommandBlacklist` 命名误导 | **保留**但**降为低**。重命名 PR 1 个。 | baseline §1.2 |
| **G9** | `Embedding` placeholder | **保留**。依赖外部 crate（fastembed 等），独立 PR。 | baseline §4.2 |
| **G10** | `ToolRegistry::Clone` 走快照 | **过分夸大**。当前行为在 P10（文件即记忆）下是 feature：可序列化快照用于 fork / audit。改 Arc 共享会失去 fork 语义。**应保留快照**，加 `try_clone_shared()` 替代。 | baseline §1.2；`registry.rs:392-406` |
| **G16** | `run_child` 默认返回 "not implemented" | **保留**。`factory.rs:93-101` 是 server-override 模式，不是 bug。 | baseline §6.2 |
| **G17** | `MonitorTool` 未实现 `Tool` trait | **保留**。低风险迁移。 | baseline §1.3 |
| **G18** | `truncate_summary` 500 char hardcoded | **保留**。走 config。 | baseline §6.2 |
| **G20** | skill usage 无自动降级 | **保留**。 | baseline §10.3 |

### 1.4 Baseline 未识别的新 Gap（从 5 份审查提炼）

| 新 # | Gap | 来源 | 建议 |
|------|-----|------|------|
| **N1** | `Permission::RequireConfirm` / `RequireExplicit` 被当 deny 处理 | toolsystem-review §3 / 高风险 | **Phase 1**。这是工具系统 P6 的真漏洞——用户预期"会确认"却拿到"拒绝"。 |
| **N2** | OAuth token 持久化同步 `std::fs`，并发竞争 | mcp-protocol-review §OAuth / 高优先级 | **Phase 2**。`tokio::fs` 化或 `spawn_blocking`。 |
| **N3** | `server_capabilities` 未解析 | mcp-protocol-review §端点 / 高优先级 | **Phase 2**。影响资源/prompts 端点条件化启用。 |
| **N4** | HTTP/SSE 无重连、进程退出未监控 | mcp-protocol-review / 高优先级 | **Phase 2**。生产可靠性。 |
| **N5** | `MetricsServer::Drop` 不 await 任务 | production-readiness / 高风险 | **Phase 1**。僵尸任务 + 资源泄漏。 |
| **N6** | `memory_background_task` handle 丢失 | production-readiness / 高风险 | **Phase 1**。同上。 |
| **N7** | stub 工具未实现（`builtins/file_tools.rs` 等） | toolsystem-review / 高风险 | **Phase 2**。功能完整性。 |
| **N8** | `JSON-RPC id: u64` 收窄（不允许 string/null） | mcp-protocol-review / 低 | **Phase 3**。合规。 |
| **N9** | LoopDetectionConfig 硬编码阈值 | agent-logic-review / 中风险 | **Phase 2**。配置化。 |
| **N10** | 内存 messages Vec 无上限 | agent-logic-review / 高风险 | **Phase 2**。P8 信息不丢需要 + P10 文件即记忆需要 max_messages_size 守护。 |

---

## 2. §10.4 "Tool 化清单"的逐个可执行评估

baseline 提出 10 个抽象 → Tool 化建议。逐个判定：

### 2.1 `SubagentSessionFactory → SubagentTool` — **已有部分实现，需要的是工厂注入**

代码现场：`tools/agent_tools/agent_tool.rs`、`messaging_tools.rs`（SendMessage / TeamCreate / TeamDelete）、`lifecycle_tools.rs`（Handoff / AgentStatus / RegisterAgent）、`team.rs`（SubagentManager）**已经存在**。`AgentTool` 是 `pub use` 公开的。

**真正缺的**：main_loop `_subagent_session_factory: _`（`main_loop.rs:153`）的工厂注入——LLM 已经能看到 `Agent` tool 描述，但 tool 内部若想 `create_child` 必须拿到 factory。

**决策**：
- ✘ 不是"包成 Tool"的问题——Tool 已经存在。
- ✓ 修复 factory 注入（解构时把 `_` 去掉 → `let subagent_factory = run_config.subagent_session_factory.clone();`），传到 `StepToolExecute`。
- **优先级**：Phase 1。

### 2.2 `ExtensionManager → ExtensionTool` — **是回归，且违反 P3 原则**

`tools/dynamic_provider/extension_manager.rs` 已经存在，且**ExtensionManager 是动态注册机制**（按需添加 tool provider），不是 LLM 可见的能力。

**Tool 化的后果**：把动态注册降级为 LLM 调用——LLM 每次想要工具都要先调 `ExtensionTool` 再调具体工具，**浪费一轮 LLM 调用 + 增加 P5 末尾复述开销 + 破坏 P3 懒加载**（动态本应"按需"，tool 化后变成"询问-执行"）。

**决策**：
- ✘ **回归**。opencode / pi-mono 的 extension / plugin 都走"运行时注册到 ToolRegistry"，不暴露为 Tool。
- ✓ 在 `component_assembly.rs` 显式注册（baseline §4.2 G4-1 路径正确）。
- **优先级**：不修。

### 2.3 `ForkPolicy → ForkTool` — **错配概念**

`ForkPolicy`（`control/fork_policy.rs`）是**配置策略 enum**（`InheritAll` / `LastNTurns` / `SinceStep` / `ByTag` / `Empty` / `SystemOnly`），不是"分叉会话的能力"。LLM 调 fork tool 改自己的 `fork_policy` 没有语义意义——`fork_policy` 在 spawn 子 agent 时由父 agent 一次性设定，不是子 agent 自身的能力。

**决策**：
- ✘ **不该 Tool 化**。这是策略不是能力。
- ✓ 真正该修的是 `main_loop.rs:140` 的 `_fork_policy: _`——在 spawn child path 上消费它（与 `SubagentSessionFactory` 一起）。
- **优先级**：Phase 1（解构串接）。

### 2.4 `load_skill → Tool` — **半成品，统一为 Tool 正确**

`synthia-skill/src/implicit_tools/load_skill` 已经是 implicit tool。baseline §4.2 G4-1 指出的路径（`component_assembly.rs` 显式注册、`is_hidden=true`）正确。

**决策**：
- ✓ 迁移到统一 `Tool` trait + hidden 标志。
- **优先级**：Phase 2。

### 2.5 `MCP servers → McpTool` — **最该做，且 opencode/codex 有现成方案**

`McpToolAdapter`（baseline §3.3 提到）已部分存在，但 `mcp_proxy` 与 `synthia-server` 的 MCP 集成路径可能重复。**这是 5 份审查中一致支持的方向**。

**决策**：
- ✓ 每个 MCP server 实例化为 `McpTool { server: Arc<McpProxy>, name: String }`，走 `ToolRegistry` 统一调度。
- ✓ 与 N3（server_capabilities 解析）合并做。
- **优先级**：Phase 2（与 N3 / N4 一并）。

### 2.6 `usage tracker → Tool` — **不该 Tool 化**

LLM 看到自己调了多少次 skill，对当前决策**没有任何增益**。这是 internal 统计。opencode / pi-mono 都走 `metrics` 而非 Tool 暴露。

**决策**：
- ✘ 不 Tool 化。
- ✓ 用 `tracing::info!` 落 P9 事件流（baseline §4.2 G4-3 路径正确）。
- **优先级**：不修。

### 2.7 `SessionStateMachine.current_state → SessionInspectTool` — **重复 + P5 错误**

P5（末尾复述）要求的是**自动注入**（已在做，`format_background_task_notification` `main_loop.rs:82-99`），不是让 LLM 主动查询。Tool 化会让 LLM 多一轮调用。

**决策**：
- ✘ 不 Tool 化。
- ✓ 把 state 信息注入末尾 system message（已经做了 `SteeringReceived` 事件）。
- **优先级**：不修。

### 2.8 `CacheInspectTool` — **泄露内部状态**

让 LLM 看到 KV cache hit ratio / prefix stability 比率，会诱导 LLM 写"讨好 cache"的 prompt（污染 P1 前缀一致性目标）。**opencode / codex / pi-mono 都严格隔离 cache 内部状态**。

**决策**：
- ✘ **反对**。P9 可观测性走 OTel / Prometheus，不走 Tool。
- ✓ 走 N5-style（OTel metric export）。
- **优先级**：不修。

### 2.9 `CompactInspectTool` — **与 CacheInspectTool 同病**

不重复论证。

**决策**：
- ✘ 不 Tool 化。P9 走 OTel。
- **优先级**：不修。

### 2.10 `ApprovalService → ApprovalTool` — **安全敏感，**绝对不该** Tool 化**

`ApprovalService` 是**系统侧权限门**——LLM 调"批准自己"是根本性安全反模式。`baseline §1.2` 自己写过 "permission 必须 fail-closed"，把 approval 暴露给 LLM 直接违反这条。

**决策**：
- ✘ **强烈反对 Tool 化**。
- ✓ 真正该做的是把 `approval_service: _`（`main_loop.rs:157`）解构串到 `StepToolExecute::on_before_tool`——baseline G1 / G4 路径正确，但形态是 **hook 拦截**，不是 tool 暴露。
- **优先级**：Phase 1（G13 真实定时器 + G1 approval 串接合并做）。

---

## 3. Synthia 独有的 5 个优秀设计（不要妄自菲薄）

对比 opencode / codex / pi-mono（基于公开信息 + baseline §1–§9）：

1. **PrefixTracker 的三段 hash + rolling stability window**
   `compute_hash_bytes` 三段（system + tools + messages prefix）+ `stability_ratio` 滚动窗口（`tracker.rs:200-213`）。opencode 的 cache 是单 key；codex 没有等价前缀稳定性度量；pi-mono 用 token 比例粗判。**Synthia 的精细度是 top-tier**，在 G5 修复（skill snapshot 纳入 hash）后会更强。

2. **CachePolicyApplier 的 `Arc::ptr_eq` 短路**
   `cache_policy.rs:170-187` 用 `Arc::ptr_eq` 而非 `PartialEq` 比较 tools/messages，**零拷贝短路**——同一引用直接返回 true 不重做。opencode 的 cache mark 重算路径比这慢一档，codex 没有"短路信号"。这是 P1 的工程化最佳实践。

3. **JSONL 事件流 + TURN_* 三态机 + 在途工具中断语义**
   `main_loop.rs:319-348` 的 `fail_interrupted_tools`——cancellation 触发时**先 yield `ToolCallCompleted { is_error: true }` 再 return**，让 P8（不丢信息）+ P5（末尾复述）在中断场景也成立。opencode 的 session 中断有数据丢失窗口；codex 有类似机制但更粗；pi-mono 不持久化 turn 事件。**synthia 在中断正确性上是 top-tier**。

4. **CompactionAnalyticsAttempt 的 trigger 区分（AutoThreshold vs ToolCall vs Recovery）**
   `main_loop.rs:783-790` 区分 LLM-driven、auto-threshold、recovery cascade 三种触发；与 `maybe_auto_trigger_compact_context` + `maybe_auto_trigger_self_reflect` 的 dedup 配合（`llm_compact_called_this_iter` 标志）。**这是 P4 渐进降级 + P9 可观测性的交叉点**，三方都能区分是 LLM 主动还是自动。opencode 只区分触发不区分来源。

5. **DefinitionDrift 检测（ForkPolicy 子系统）**
   `control/fork_policy.rs:90-126` 的 `detect_definition_drift`——子 agent 完成后**反向比对** system prompt hash 和 denied_tools，给出 `minor` / `moderate` / `severe` 三档。这是 baseline §6.1 没强调的、被低估的设计。opencode / codex 的 subagent 没有 drift 监测。**synthia 的 subagent governance 是 top-tier**。

---

## 4. 优先级路线图

### Phase 1（必做，预计 4-6 周）

| ID | 任务 | 证据 |
|----|------|------|
| P1-1 | 修复 `CachePolicyApplier::apply` 加 `user_id: &str` 参数 + 跨用户 cache 隔离 | baseline §7.2 G3；`run_config.rs:43-47` 注释自承 |
| P1-2 | `FailPolicy` 默认改为 `FailClosed`，显式 OptIn 才 `FailOpen` | baseline §2.2 G2；toolsystem-review 高风险 |
| P1-3 | `RequireConfirm` / `RequireExplicit` 真正走 confirm 路径而非 deny | toolsystem-review 高风险 N1；`registration.rs:172-178` |
| P1-4 | `StepToolExecute` 嵌入 `on_before_tool` / `on_after_tool` / `on_iteration_end` / `on_error` | baseline §2.2 G4；main_loop 只 fire 2 个 llm hook |
| P1-5 | `main_loop` 解构保留：把 `_subagent_session_factory` / `_fork_policy` / `_extension_manager` / `_approval_service` 真串到 Step* | baseline §5.2 G1 精细化；`run_config.rs` 字段已注释 |
| P1-6 | `SessionStateMachine::StartApprovalTimeout` 接 `tokio::time::sleep` 真实定时器 | baseline §8.2 G13 |
| P1-7 | `MetricsServer::Drop` + `memory_background_task` handle 保存 + await | production-readiness 高风险 N5/N6 |

### Phase 2（应该做，预计 6-8 周）

| ID | 任务 | 证据 |
|----|------|------|
| P2-1 | Skill snapshot 纳入 `PrefixTracker::compute_hash_bytes`（第四参数） | baseline §4.2 G5；`tracker.rs:77-87` |
| P2-2 | MCP server 统一为 `McpTool` + `server_capabilities` 解析 + SSE 重连 + 进程监控 | baseline §3.3；mcp-protocol-review N3/N4 |
| P2-3 | OAuth token 持久化改 `tokio::fs` / `spawn_blocking` | mcp-protocol-review N2 |
| P2-4 | `previous_summary` 4000 char 抽成 `SummaryConfig` | baseline §10.2 G11 |
| P2-5 | `parent_depth` 硬上限 + `max_subagent_depth` | baseline §6.2 G19 |
| P2-6 | `transition_to` dirty-flag + batch flush | baseline §8.2 G12 |
| P2-7 | LoopDetectionConfig 阈值移到 `AgentConfig` | agent-logic-review N9 |
| P2-8 | `load_skill` implicit tool 迁移统一 `Tool` trait + `is_hidden=true` | baseline §4.2 G4-1 |
| P2-9 | 完成 `builtins/file_tools.rs` 等 stub 实现 | toolsystem-review N7 |
| P2-10 | `max_messages_size` 配置项 + 超出强制 compaction | agent-logic-review N10 |

### Phase 3（可做，长期）

| ID | 任务 |
|----|------|
| P3-1 | 双 hook 系统统一（`AgentHook` ↔ `HookRunner`） |
| P3-2 | `MatchingStrategy::Embedding` 真实 backend |
| P3-3 | `MonitorTool` 迁移 `Tool` trait |
| P3-4 | `PluginManifest.hooks` schemars 验证 |
| P3-5 | `truncate_summary` 走 config |
| P3-6 | `CommandBlacklist` 重命名 `DefensivePatternHint` |
| P3-7 | 并发安全工具矩阵 audit（不盲目改默认） |
| P3-8 | JSON-RPC `id` 支持 string/null |

---

## 5. 一句话总结

> **Synthia 不缺抽象，缺"消费"**。G1 字段丢弃、approval / compaction analytics / DefinitionDrift 这些已就位的高级设计没有被 main_loop 串起来——**结构性问题不是"补抽象"，而是"接好已存在的线"**。Tool 化清单 10 条只有 1.5 条（SubagentTool 工厂注入 + load_skill 统一）值得做，其余 8.5 条要么是回归、要么是错配概念、要么是 P6 安全反模式。**Phase 1 7 条全是"解构串接 + 真实定时器 + 故障隔离"**，没有"全新抽象"——这恰好说明 synthia 抽象层过厚、过载、没有真正启用。