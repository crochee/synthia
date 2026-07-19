## Why

synthia-agent 当前就绪度约 55%，缺少工具超时控制、上下文渐进降级、Cron 桥接层和记忆检索能力，无法作为稳定的日常生产力工具使用。核心引擎（ReAct 循环、循环检测、Steering）已成熟，但工具执行可靠性、上下文管理精细化、定时任务集成和记忆系统完善度是六大缺口。本变更补齐这些能力，使 agent 达到"不丢数据、不无限阻塞、错误可恢复、超时可控、有可观测性"的稳定可靠标准。

## What Changes

### 工具执行可靠性
- From: 工具调用无超时、无取消、无截断，慢命令可无限阻塞
- To: 差异化超时包装（按工具类型）、CancellationToken 全链路传递、结果截断（头尾保留）、重试层（仅幂等操作）
- Reason: C5 物理约束 — 工具执行不可预测
- Impact: 所有工具调用路径，breaking change（超时行为新增）

### Subagent 结果等待
- From: AgentTool.call fire-and-forget，只返回 "Waiting for result..."
- To: 复用 SubagentExecutor，等待完整结果（5 分钟超时）
- Reason: 功能缺口 — 多 agent 协作无法同步
- Impact: AgentTool 行为变更

### 上下文管理升级
- From: 整段 section 丢弃，信息突然大量丢失
- To: 三阶段渐进降级（Soft Trim → Hard Clear → 分级压缩）+ 安全阈值（16K/32K）+ KV Cache 前缀追踪
- Reason: P4 渐进降级 + P1 前缀一致性
- Impact: Context 组装和修剪路径

### Cron 系统桥接
- From: 时间轮调度器 + Cron 解析器已有，但桥接到 agent 的部分全部缺失
- To: CronJobWrapper（三种执行模式）+ CronFileStore（持久化）+ cron_add/list 工具 + 混合执行模式
- Reason: 核心能力缺失 — 无法定时触发 agent 任务
- Impact: CLI scheduler 和 agent 工具层

### 记忆系统增强
- From: Phase 1/2 Pipeline 已有，但无记忆检索和注入能力
- To: 旁路事件日志（JSONL，脱敏存储）+ memory_search 工具 + 记忆注入策略
- Reason: P8 信息不丢失 + P3 按需加载
- Impact: 事件记录、记忆检索、上下文注入

### 可观测性
- From: 有 Prometheus 端口但缺少关键指标
- To: Context Trace（每步独立文件）+ 8 个关键指标 + 本地告警
- Reason: P9 可观测性优先
- Impact: 新增监控层

### 错误恢复
- From: 循环检测器好，但缺少系统性错误恢复
- To: 五层恢复（Truncate → Retry → Fallback → Auto-Compact → Reset）+ 死锁防护
- Reason: 稳定可靠标准要求
- Impact: 错误处理全链路

## Capabilities

### New Capabilities
- `tool-execution`: 工具超时控制、重试、截断、可取消执行
- `context-management`: 渐进降级、安全阈值、KV Cache 前缀追踪
- `cron-system`: 定时任务桥接、持久化、混合执行模式
- `memory-system`: 旁路事件日志、记忆搜索、记忆注入
- `observability`: Context Trace、Prometheus 指标、本地告警
- `error-recovery`: 五层错误恢复、死锁防护

### Modified Capabilities
<!-- No existing capabilities whose REQUIREMENTS are changing -->

## Impact

- **Affected crates**: synthia-agent (核心变更), synthia-cli (scheduler 集成)
- **New modules**: tool_executor/, context/pruning.rs, tools/cron_*.rs, event_log/, observability/, error_recovery/
- **Dependencies**: tokio-util (CancellationToken), fs2 (文件锁，可选)
- **Breaking changes**: 工具调用新增超时行为；SubagentTool 从 fire-and-forget 改为同步等待
- **Config changes**: 超时配置表、安全阈值配置
