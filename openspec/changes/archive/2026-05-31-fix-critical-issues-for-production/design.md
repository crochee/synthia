## Context

Synthia 是一个生产级 AI Agent 框架，当前代码存在多个影响生产部署的严重问题。专家团队审查发现：

- **MCP 协议实现不完整**：缺少心跳机制，SSE 传输是模拟实现
- **Memory 性能问题**：全表扫描导致高并发下性能严重下降
- **Plugin 安全问题**：使用 panic! 而非错误传播
- **Execution 沙箱绕过**：命令检测可被简单混淆绕过
- **Clippy 存在 lint**：examples 有构建错误

目标：在 0 lint 状态下实现生产级 AI Agent，支持实际 CLI 和 Server 工作。

## Goals / Non-Goals

**Goals:**
- 修复所有 P0 严重问题（MCP 心跳、SSE、Memory 性能、Plugin panic、Execution 沙箱）
- 实现 0 clippy warnings
- 确保 CLI 和 Server 可实际运行
- 修复 examples 构建错误

**Non-Goals:**
- 不重构现有架构（保持现有设计）
- 不添加新功能（仅修复问题）
- 不实现 LLM 摘要调用（保留占位符，待后续集成）

## Decisions

### D1：SSE 实现方案

- **選擇**：使用 `axum::extract::SSE` 配合 `tokio::sync::mpsc`
- **理由**：已有 axum 依赖，复用现有异步运行时
- **已考慮 alternative**：使用独立的 `sse` crate → 增加外部依赖

### D2：Memory 查询优化

- **選擇**：将过滤逻辑 push down 到 SQL，使用 WHERE/ORDER BY/LIMIT
- **理由**：避免加载全部数据到内存，减少内存占用和 CPU 开销
- **已考慮 alternative**：使用 `VecDeque` 优化 LRU → 仅优化 eviction，查询仍全表扫描

### D3：Plugin 错误处理

- **選擇**：将 `panic!` 改为返回 `Result.Err(HookRunnerError::ExecutionFailed)`
- **理由**：符合 Rust 错误处理规范，便于调用者处理
- **已考慮 alternative**：使用 ` anyhow::Result` → 引入新依赖

### D4：Execution 沙箱检测

- **選擇**：增强正则表达式，检测 `curlhttps`、`hxxps` 等混淆模式
- **理由**：简单有效，无需引入额外沙箱库
- **已考慮 alternative**：使用 seccomp/bpf → 增加复杂性，不跨平台

### D5：子进程管理

- **選擇**：使用 `wait()` 等待子进程退出，避免僵尸进程
- **理由**：标准 POSIX 做法，跨平台兼容
- **已考慮 alternative**：使用 `SIGCHLD` 处理 → 增加复杂性

## Risks / Trade-offs

[Risk] SSE 重写可能破坏现有连接 → Mitigation: 先在单独模块实现，测试通过后替换

[Risk] SQL 查询改写可能引入新的性能问题 → Mitigation: 添加查询性能测试

[Risk] 命令检测增强可能产生误杀 → Mitigation: 添加白名单机制

[Trade-off] 保留 LLM 摘要占位符 → 接受理由：真实 LLM 集成需要 API 设计决策，后续迭代

## Migration Plan

1. 按模块依次修复：MCP → Memory → Plugin → Execution
2. 每个模块修复后运行 `cargo clippy` 验证
3. 修复 examples 构建错误
4. 完整测试后合并

验收条件：
- `cargo clippy --all-targets -- -D warnings` 通过
- `cargo test` 通过
- CLI 和 Server 可正常启动

## Open Questions

1. MCP heartbeat 间隔时间（建议 30s）
2. LLM 摘要是否需要真实实现（当前为占位符）
3. 是否需要实现 AsyncDrop for McpProxy