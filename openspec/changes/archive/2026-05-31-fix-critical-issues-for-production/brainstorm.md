# Brainstorming: Synthia 生产级 AI Agent 全面修复

## 背景

5人专家团队对 Synthia 进行了全面架构审查，发现多个影响生产部署的严重问题。

## 问题汇总

### P0 严重问题（必须修复）

| # | 模块 | 问题 | 位置 |
|---|------|------|------|
| 1 | MCP | 缺少 ping/pong 心跳机制 | connection.rs |
| 2 | MCP | SSE 传输使用 duplex 模拟，非真实实现 | sse_transport.rs |
| 3 | Memory | `load_all_entries()` 全表扫描性能问题 | sqlite.rs:503-506 |
| 4 | Memory | LLM 摘要输出占位符 `[LLM Summary Placeholder]` | compactor.rs:55-68 |
| 5 | Plugin | `hook_runner.rs:336` 使用 panic! 处理错误 | hook_runner.rs |
| 6 | Execution | 沙箱命令检测可被绕过（如 `curlhttps://evil.com`） | command 模块 |
| 7 | Execution | `get_child()` 不等待子进程，可能产生僵尸进程 | session.rs |

### P1 中等问题

| # | 模块 | 问题 |
|---|------|------|
| 1 | Agent | 断路器参数硬编码不可配置 |
| 2 | Agent | Hook 失败默认 fail-open，应支持 fail-closed |
| 3 | Memory | LRU eviction O(n) 复杂度 |
| 4 | Memory | `created_at` 时间戳在加载时被覆盖 |
| 5 | MCP | WebSocket 消息传递实现不完整 |
| 6 | MCP | OAuth 使用自定义错误码而非标准 MCP 错误码 |
| 7 | Plugin | `McpProxy` Drop 不支持异步清理 |

### 低优先级

1. Agent self-reflection 使用硬编码中文 prompt
2. 多个模块日志不完整
3. 工具执行无显式超时保护

## 修复策略

### 1. MCP 协议修复
- 添加 heartbeat 模块实现 ping/pong
- 使用 `axum` 的 SSE 支持或 `sse` crate 重写 sse_transport
- 完善握手流程，解析 capabilities

### 2. Memory 性能优化
- 将过滤逻辑 push down 到 SQL 查询
- 实现真正的 LLM 摘要调用
- 优化 LRU 使用 `VecDeque` 替代 `HashMap` 遍历

### 3. Plugin 安全修复
- 将 `panic!` 改为返回 `Result.Err`
- 实现 AsyncDrop 模式或显式 `shutdown()` 方法

### 4. Execution 安全修复
- 增强命令检测正则，支持更多混淆模式
- 确保子进程正确等待（使用 `wait()` 而非 `detach()`）

### 5. 清理
- Examples 错误修复
- Clippy 0 lint

## 设计决策

**Q1: SSE 实现方案**
- 选项 A: 使用 `sse` crate（轻量、专注 SSE）
- 选项 B: 使用 `axum::extract::SSE`（已有 axum 依赖）
- **推荐: 选项 B**，复用现有依赖

**Q2: LLM 摘要调用**
- 需确定是使用外部 LLM API 还是本地模型
- **当前占位符**，后续集成真实 LLM 调用

**Q3: Plugin 异步清理**
- 选项 A: 实现 `AsyncDrop` trait（需要 unsafe）
- 选项 B: 添加显式 `shutdown()` 方法
- **推荐: 选项 B**，更安全可控