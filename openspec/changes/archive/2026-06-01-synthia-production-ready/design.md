## Context

synthia-agent 是一个基于 Rust 的 AI agent 框架，实现 ReAct (Reasoning + Acting) 模式。当前代码库中核心引擎（ReAct 循环、循环检测器、Steering、Guardian Bridge）已成熟，但工具执行可靠性、上下文管理精细化、定时任务集成和记忆系统完善度存在六大缺口。当前就绪度约 55%。

约束条件：
- 部署方式：本地 CLI
- LLM 配置：模型路由
- 记忆策略：文件系统 (P10)
- 可靠性标准：稳定可靠（不丢数据、不无限阻塞、错误可恢复）
- 设计原则：遵循 agent_rule.md（P1 前缀一致性、P2 Append-Only、P4 渐进降级、P6 不信任 LLM、P7 可中断性、P8 信息不丢失、P10 文件即记忆）

已知前置问题：代码库当前有 11 个编译错误（rmcp 库 API 不兼容、类型不匹配），必须在实施本设计前修复。

## Goals / Non-Goals

**Goals:**
- 工具调用超时可控、可取消、可截断
- 上下文渐进降级，不突然丢失大量信息
- KV Cache 前缀稳定性可追踪（prefix_hash 指标）
- Cron 定时任务可持久化、可混合执行（独立/注入/新会话）
- 记忆可检索（memory_search）、事件日志完整保留（脱敏后）
- 五层错误恢复，防止死锁循环
- 关键指标可观测（9 个 Prometheus 指标 + Context Trace）

**Non-Goals:**
- 向量数据库/语义搜索（Phase 1 才考虑）
- 云端部署/多租户支持
- 企业级 SLA/混沌工程测试
- 修改 ReAct 循环核心逻辑
- 修改循环检测器实现

## Decisions

### D1: 工具超时在 step.rs 调用层包装，不在 registry 层
- **选择**: 超时/截断/重试逻辑包装在 `execute_single_tool`（step.rs）的调用层
- **理由**: 现有 `agent/tool_executor.rs` 已有 ToolExecutionResult 和 ToolErrorSummary，扩展而非重建更简单；registry 层不应关心超时策略
- **已考虑 alternative**: 在工具 registry 层面加超时 → 拒绝，这会让每个工具实现都受同一超时策略限制，无法差异化

### D2: Subagent 复用已有 SubagentExecutor，不修复 AgentTool
- **选择**: 废弃 AgentTool 的 fire-and-forget 逻辑，改用 SubagentExecutor + 5 分钟超时
- **理由**: SubagentExecutor 在 cli/src/agent.rs 已有基础，修复成本更低
- **已考虑 alternative**: 修补 AgentTool.call 增加等待逻辑 → 拒绝，AgentTool 是 legacy 实现，不值得投入

### D3: Pruning 作为 compaction 的前置步骤，非替换
- **选择**: 流程为 Soft Trim → Hard Clear → 分级压缩 → (如果还不够) → 调用现有 compaction
- **理由**: compaction.rs 作为 L4 (Auto-Compact) 的触发器保留，新增 pruning 是更细粒度的前置处理
- **已考虑 alternative**: 替换整个 compaction 系统 → 拒绝，compaction 调用 LLM 做摘要是独立能力，pruning 是确定性规则

### D4: prefix_hash = system_prompt + skill_snapshot 的 SHA-256
- **选择**: 前缀哈希仅包含 system_prompt 和 skill_snapshot，不包含 conversation history
- **理由**: 符合 P1 (前缀一致性) — 这两部分才是 KV Cache 前缀的内容，conversation history 追加到末尾不影响前缀
- **已考虑 alternative**: 整个 prompt 的 hash → 拒绝，每轮追加消息后 hash 必然变化，无法追踪前缀稳定性

### D5: 事件日志写入前必须脱敏
- **选择**: 所有事件经过 credential_guard 扫描脱敏后写入，大文件 output 限制 10KB，高敏感事件仅存 hash
- **理由**: SEC-1 安全风险 — 事件日志存储所有工具输出，包含明文 API Key 等敏感数据
- **已考虑 alternative**: 不脱敏，靠文件权限保护 → 拒绝，文件权限无法防止同一用户的其他进程读取

### D6: Context Trace 每步独立文件
- **选择**: trace 文件名为 `context_<session_id>_<step>.jsonl`
- **理由**: REL-2 避免并发写入 race condition（subagent 和主 agent 可能并发写同一 session 的 trace）
- **已考虑 alternative**: 单个文件 + 文件锁 → 拒绝，锁增加复杂度，独立文件更简单

### D7: Context Trace 每步独立文件
- **选择**: trace 文件名为 `context_<session_id>_<step>.jsonl`
- **理由**: REL-2 避免并发写入 race condition
- **已考虑 alternative**: 单个文件 + 文件锁 → 拒绝，锁增加复杂度

## Risks / Trade-offs

- [Risk] Context pruning 实现复杂度高，可能影响 cache 命中率 → Mitigation: 分阶段实现，先 Stage 1 (Soft Trim)
- [Risk] Subagent 等待引入死锁风险 → Mitigation: 严格 5 分钟超时 + CancellationToken
- [Risk] 事件日志无界增长（高频工具调用 + 大文件输出） → Mitigation: output 限制 10KB + 按日期分割 + 保留 30 天
- [Risk] L4 (Auto-Compact) ↔ L5 (Reset) 死锁循环 → Mitigation: 全局错误计数器 + 30 秒冷却期 + fail-fast
- [Trade-off] 文件系统记忆 vs 向量搜索 → 接受文件系统（Phase 0）：零依赖、人可审计、搜索能力弱但够用
- [Trade-off] ripgrep 关键词搜索 vs 语义搜索 → 接受关键词搜索：简单、可靠、Phase 1 可升级

## Migration Plan

部署顺序：
1. **前置**: 修复 11 个编译错误
2. **P0**: 工具超时包装 + Subagent 修复 + 事件日志脱敏
3. **P1**: 上下文安全阈值 + CronJobWrapper + CronFileStore + Shell 安全加固
4. **P2**: Soft Trim/Hard Clear + 事件日志 + memory_search
5. **P3**: 可观测性指标 + 错误恢复完整链路

Rollback 策略：所有新增模块通过 feature flag 或配置开关控制，可逐个禁用。工具超时行为是 breaking change，可通过配置恢复旧行为（无限超时）。

验收条件：
- `cargo clippy --workspace --all-targets` 无警告
- `cargo test --lib` 全通过
- 本地运行 CLI 可完成编码任务（读写文件、执行命令、子 agent）
- 本地运行 CLI 可完成定时任务（设置 cron、到点触发、结果存储）

## Open Questions

- Shell 超时统一为 60 秒还是保持现有 120 秒？设计文档建议统一为 60 秒
- 事件日志保留策略（30 天自动清理）是否需要在 Phase 0 实现，还是可以推迟？
- Cron 最短间隔 1 分钟是否足够？是否需要支持秒级间隔的受信任任务？
