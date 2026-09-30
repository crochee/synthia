## 1. MCP 协议修复

- [x] 1.1 添加 heartbeat 模块到 synthia-mcp，包含 ping/pong 状态机
- [x] 1.2 在 McpConnection 中实现 idle 状态检测和心跳发送
- [x] 1.3 添加 pong 超时检测，超时后转换到 Error 状态
- [x] 1.4 重写 sse_transport.rs，使用真实 HTTP SSE 流替代 DuplexStream 模拟
- [x] 1.5 修复 examples/plugin_example.rs 缺少的依赖声明

## 2. Memory 性能优化

- [x] 2.1 重构 `search_with_mode` 方法，将过滤逻辑 push down 到 SQL 查询
- [x] 2.2 修复 `load_all_entries` 中的 `created_at` 时间戳丢失问题
- [x] 2.3 验证修改后 `cargo clippy -p synthia-memory -- -D warnings` 通过
- [x] 2.4 添加 LRU 性能优化（使用 VecDeque 替代 HashMap 遍历）

## 3. Plugin 安全修复

- [x] 3.1 将 `hook_runner.rs:336` 的 `panic!` 改为返回 `Result.Err(HookRunnerError::ExecutionFailed)`
- [x] 3.2 验证修改后 `cargo clippy -p synthia-plugin -- -D warnings` 通过

## 4. Execution 沙箱修复

- [x] 4.1 增强命令检测正则，支持 `curlhttps://`、`hxxps://`、`wget` 等混淆模式
- [x] 4.2 添加 case-insensitive URL scheme 检测
- [x] 4.3 实现白名单机制配置
- [x] 4.4 修复 `get_child()` 僵尸进程问题，使用 `wait()` 等待子进程退出

## 5. 验证和清理

- [x] 5.1 运行 `cargo clippy --all-targets -- -D warnings` 确保 0 lint
- [x] 5.2 运行 `cargo test` 确保所有测试通过
- [x] 5.3 验证 CLI 和 Server 可正常启动
- [x] 5.4 清理任何遗留的临时文件或调试代码