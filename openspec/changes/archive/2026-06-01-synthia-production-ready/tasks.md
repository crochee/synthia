## 1. 前置：修复编译错误

- [x] 1.1 修复 rmcp 库 API 不兼容问题（`client()` 方法不存在、`Annotated<RawContent>` 无 `get` 方法）
- [x] 1.2 修复 `agent_runtime.rs` 类型不匹配（`Ok(Ok(()))` 期望 `Result<(), Error>` 但收到 `()`）
- [x] 1.3 确保 `cargo check --workspace --lib` 通过

## 2. 工具超时与可靠性（P0）

- [x] 2.1 创建 `src/tool_executor/config.rs` — 超时/重试配置结构体（按工具类别）
- [x] 2.2 创建 `src/tool_executor/timeout.rs` — tokio::time::timeout 包装逻辑
- [x] 2.3 创建 `src/tool_executor/truncate.rs` — 结果截断逻辑（head+tail 保留，16KB 阈值）
- [x] 2.4 创建 `src/tool_executor/mod.rs` — ToolExecutor 主结构体，整合超时/截断/重试
- [x] 2.5 修改 `step.rs` — 在 `execute_single_tool` 调用层集成 ToolExecutor
- [x] 2.6 修复 `agent_tools.rs` — Subagent 改为等待结果（复用 SubagentExecutor + 5 分钟超时）
- [x] 2.7 统一 Shell 超时为 60 秒（从现有 120 秒），最大上限 600 秒
- [x] 2.8 编写工具超时和截断的单元测试

## 3. 安全加固（P0/P1）

- [x] 3.1 创建 `src/event_log/mod.rs` — 旁路事件日志写入（异步 + 批量缓冲 + fsync）
- [x] 3.2 创建 `src/event_log/types.rs` — 事件类型定义
- [x] 3.3 事件日志写入集成 credential_guard 脱敏（API Key → [REDACTED]）
- [x] 3.4 大文件 output 限制 10KB，超出部分存 hash 到事件日志，完整内容存 raw_outputs/
- [x] 3.5 文件系统权限：~/.synthia/ 目录 0700，事件日志文件 0600
- [x] 3.6 Shell 安全加固：增加解释器检测（python/perl/node/ruby）、命令替换检测、Base64 解码管道检测
- [x] 3.7 后台任务安全约束：超时控制、Guardian 审批、Agent 退出时清理子进程

## 4. 上下文管理升级（P1/P2）

- [x] 4.1 创建 `src/context/thresholds.rs` — 安全阈值检查（HARD_MIN 16K, WARN_BELOW 32K）
- [x] 4.2 创建 `src/context/pruning.rs` — 三阶段渐进降级逻辑
- [x] 4.3 实现 Soft Trim — 缩减大 tool result，保留头 500 tokens + 尾 500 tokens
- [x] 4.4 实现 Hard Clear — 旧 tool result 替换为 `[cleared]` 占位符
- [x] 4.5 实现分级压缩 — Level 1 保留 Decision/Error, Level 2 FileModified 摘要, Level 3 FileRead/Output 删除
- [x] 4.6 修改 `context/assembler.rs` — 集成 Soft Trim/Hard Clear，不改变现有 compaction 接口
- [x] 4.7 创建 `src/context/prefix_tracker.rs` — KV Cache 前缀追踪（SHA-256 of system_prompt + skill_snapshot）
- [x] 4.8 编写上下文修剪和阈值的单元测试

## 5. Cron 系统桥接（P1）

- [x] 5.1 创建 `src/tools/cron_store.rs` — CronFileStore 实现（JSONL 持久化 + fsync + 容错加载）
- [x] 5.2 创建 `src/tools/cron_wrapper.rs` — CronJobWrapper 实现（三种模式）
- [x] 5.3 创建 `src/tools/cron_tool.rs` — cron_add/list/remove/pause/resume 工具
- [x] 5.4 创建 `cli/src/scheduler/cron_loader.rs` — 启动时加载 cron job
- [x] 5.5 修改 `cli/src/scheduler/mod.rs` — 集成 CronJobWrapper/CronFileStore
- [x] 5.6 修改 `cli/src/agent.rs` — 修复 CronFileStore/CronJobWrapper 引用
- [x] 5.7 Cron 最短间隔 1 分钟限制
- [x] 5.8 Inbox 消息注入机制（session/inbox/ 目录）
- [x] 5.9 编写 Cron 工具的单元测试

## 6. 记忆系统增强（P2）

- [x] 6.1 创建 `src/tools/memory_search.rs` — memory_search 工具（ripgrep 搜索 + 脱敏）
- [x] 6.2 创建 `src/memories/injector.rs` — 记忆注入策略（启动加载、按需搜索、末尾复述）
- [x] 6.3 事件日志按日期分割（events/YYYY-MM-DD.jsonl）
- [x] 6.4 memory_search 结果经过 credential_guard 脱敏
- [x] 6.5 编写 memory_search 的单元测试

## 7. 可观测性（P3）

- [x] 7.1 创建 `src/observability/context_trace.rs` — Context Trace 记录（每步独立文件）
- [x] 7.2 创建 `src/observability/metrics.rs` — 9 个 Prometheus 指标定义
- [x] 7.3 创建 `src/observability/alerts.rs` — 本地告警逻辑
- [x] 7.4 prefix_stability_ratio 统计和暴露
- [x] 7.5 Context Trace 在 call_model 内部集成

## 8. 错误恢复（P3）

- [x] 8.1 创建 `src/error_recovery/mod.rs` — 五层恢复协调器
- [x] 8.2 创建 `src/error_recovery/retry.rs` — 重试逻辑（指数退避，最多 2 次）
- [x] 8.3 创建 `src/error_recovery/fallback.rs` — 降级方案（web_fetch→缓存, subagent→直接回答等）
- [x] 8.4 创建 `src/error_recovery/compact.rs` — 自动压缩（生成摘要）
- [x] 8.5 创建 `src/error_recovery/reset.rs` — 会话重建（transcript 保存 + 新 session 创建 + fail-fast）
- [x] 8.6 防止 L4 ↔ L5 死锁循环（全局错误计数器 + 30 秒冷却期）
- [x] 8.7 React 错误路径增加 cleanup 逻辑（reset circuit breaker + drain steering + 清理后台进程）
- [x] 8.8 编写错误恢复的单元测试

## 9. 验证与集成

- [x] 9.1 `cargo fmt --all` 格式化
- [x] 9.2 `cargo clippy --all-targets --all-features --tests --all` 无警告
- [x] 9.3 `cargo test --lib` 全部通过
- [x] 9.4 本地运行 CLI 完成编码任务（读写文件、执行命令、子 agent）
- [x] 9.5 本地运行 CLI 完成定时任务（cron_add 设置、到点触发、结果存储）
- [x] 9.6 验证 Prometheus 指标正确输出
- [x] 9.7 验证 Context Trace 文件正确生成
