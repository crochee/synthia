## Why

Synthia 的核心架构设计良好，但存在多个阻止生产部署的严重问题：MCP 协议心跳缺失导致长连接被中间件切断、Memory 全表扫描导致性能崩溃、Plugin 使用 panic! 而非错误传播、Execution 沙箱可被简单绕过。这些问题必须修复才能支持实际的 CLI 和 Server 工作。

## What Changes

**MCP 协议心跳**
- From: 连接建立后无任何心跳，Idle 状态无检测
- To: 添加 ping/pong 心跳机制，空闲超时断开
- Reason: 防止中间件切断空闲连接
- Impact: 非破坏性，MCP 客户端兼容

**MCP SSE 传输**
- From: 使用 tokio io DuplexStream 模拟
- To: 使用真实的 SSE 事件流处理
- Reason: 支持真正的服务端推送
- Impact: 非破坏性，内部实现变更

**Memory 冷查询优化**
- From: `load_all_entries()` 加载全部到内存再过滤
- To: 使用 SQL WHERE/ORDER BY/LIMIT 精确查询
- Reason: 避免高并发下内存爆炸
- Impact: 非破坏性，API 不变

**Plugin 错误处理**
- From: `hook_runner.rs:336` 使用 `panic!` 处理命令失败
- To: 返回 `Result.Err(HookRunnerError::ExecutionFailed)`
- Reason: 符合 Rust 错误处理规范
- Impact: 非破坏性，调用方需处理 Result

**Execution 沙箱检测**
- From: 正则检测 `curl http` 但 `curlhttps` 可绕过
- To: 增强正则检测多种混淆模式
- Reason: 防止命令注入
- Impact: 可能误杀，待添加白名单

**僵尸进程防护**
- From: `get_child()` 不等待子进程
- To: 使用 `wait()` 确保子进程退出
- Reason: 防止系统资源泄漏
- Impact: 非破坏性

## Capabilities

### New Capabilities
- `mcp-heartbeat`: MCP 连接心跳机制，空闲检测和自动重连
- `sse-transport`: 真实 SSE 传输实现，支持服务端推送
- `cold-query-optimization`: SQL 推送过滤，避免全表扫描
- `command-sandbox-v2`: 增强命令检测，防止绕过

### Modified Capabilities
- `plugin-hook-runner`: 错误处理从 panic 改为 Result
- `session-process`: 子进程等待机制

## Impact

**代码变更：**
- `crates/synthia-mcp/src/connection.rs` - 添加心跳状态和 ping/pong
- `crates/synthia-mcp/src/sse_transport.rs` - 重写为真实 SSE
- `crates/synthia-memory/src/cold/sqlite.rs` - SQL 查询优化
- `crates/synthia-plugin/src/hook_runner.rs` - panic 改 Result
- `crates/synthia-command/` - 增强沙箱检测
- `crates/synthia-session/` - 子进程等待
- `examples/plugin_example.rs` - 修复构建错误