---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 贡献指南

感谢您有兴趣为 Synthia 做出贡献！本文档将帮助您了解如何参与项目开发。

## 1. 目录

- [行为准则](#行为准则)
- [如何贡献](#如何贡献)
- [开发环境设置](#开发环境设置)
- [代码规范](#代码规范)
- [提交规范](#提交规范)
- [Pull Request 流程](#pull-request-流程)

## 2. 行为准则

请阅读并遵守我们的行为准则。我们致力于提供友好、安全和欢迎的环境。

## 3. 如何贡献

### 报告问题

如果您发现了 bug 或有功能建议，请：

1. 搜索现有的 [issues](https://github.com/your-org/synthia/issues)，确认问题未被报告
2. 创建新的 issue，包含：
   - **问题描述**：清晰简洁地描述问题
   - **复现步骤**：详细说明如何复现
   - **期望行为**：描述您期望发生什么
   - **实际行为**：描述实际发生了什么
   - **环境信息**：操作系统、Rust 版本等
   - **日志/截图**：如果适用，附上相关日志或截图

### 提交代码

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 进行更改
4. 提交更改 (`git commit -m 'feat: add amazing feature'`)
5. 推送到分支 (`git push origin feature/amazing-feature`)
6. 创建 Pull Request

## 4. 开发环境设置

### 前置要求

- Rust 1.75+ (推荐使用 rustup)
- SQLite 3.x
- Git

### 克隆仓库

```bash
git clone https://github.com/your-org/synthia.git
cd synthia
```

### 构建项目

```bash
cargo build
```

### 运行测试

```bash
cargo test --all
```

### 运行服务器

```bash
cargo run --package synthia-server
```

## 5. 代码规范

### Rust 代码规范

- 遵循 [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- 运行 `cargo fmt` 格式化代码
- 运行 `cargo clippy` 检查代码
- 确保所有测试通过

```bash
# 格式化代码
cargo +nightly fmt --all

# 检查代码
cargo clippy --all-targets --all-features --tests --all
```

### 文档规范

- 为公共 API 添加文档注释
- 使用完整的句子和正确的标点
- 包含使用示例

```rust
/// 执行工具并返回结果。
///
/// # Arguments
///
/// * `name` - 工具名称
/// * `args` - 工具参数 (JSON 格式)
///
/// # Returns
///
/// 工具执行结果
///
/// # Example
///
/// ```rust
/// let result = execute_tool("read", json!({"path": "/tmp/test.txt"})).await?;
/// ```
pub async fn execute_tool(name: &str, args: Value) -> Result<ToolResult> {
    // ...
}
```

### 测试规范

- 为新功能添加单元测试
- 为 API 端点添加集成测试
- 测试覆盖率应保持合理水平

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chat_endpoint() {
        // Arrange
        let app = create_test_app().await;

        // Act
        let response = app
            .post("/chat")
            .json(&json!({"message": "Hello"}))
            .await;

        // Assert
        assert_eq!(response.status_code(), StatusCode::OK);
    }
}
```

## 6. 提交规范

我们使用 [Conventional Commits](https://www.conventionalcommits.org/) 规范：

### 提交格式

```
<type>(<scope>): <subject>

<body>

<footer>
```

### 提交类型

| 类型 | 说明 |
|------|------|
| `feat` | 新功能 |
| `fix` | Bug 修复 |
| `docs` | 文档更新 |
| `style` | 代码格式（不影响功能） |
| `refactor` | 重构（不新增功能或修复 bug） |
| `perf` | 性能优化 |
| `test` | 添加或修改测试 |
| `chore` | 构建过程或辅助工具的变动 |

### 示例

```
feat(server): add WebSocket support

- Implement WebSocket endpoint at /ws
- Add session isolation
- Support real-time bidirectional communication

Closes #123
```

## 7. Pull Request 流程

### 提交前检查

- [ ] 代码已格式化 (`cargo fmt`)
- [ ] 代码已通过 clippy 检查 (`cargo clippy`)
- [ ] 所有测试已通过 (`cargo test`)
- [ ] 文档已更新（如适用）
- [ ] CHANGELOG.md 已更新（如适用）

### PR 标题

PR 标题应遵循提交规范：

```
feat(server): add new API endpoint
fix(agent): resolve loop detection issue
docs(readme): update installation instructions
```

### PR 描述

PR 描述应包含：

1. **变更说明**：描述此 PR 解决的问题或添加的功能
2. **变更类型**：Bug 修复 / 新功能 / 重构 / 文档更新
3. **测试说明**：如何测试这些变更
4. **相关 Issue**：链接到相关 issue（如有）

### 代码审查

- 所有 PR 需要至少一位维护者审查
- 审查者会检查代码质量、测试覆盖率和文档完整性
- 请及时响应审查意见

### 合并条件

- 所有 CI 检查通过
- 至少一位维护者批准
- 没有未解决的审查意见
- 分支与主分支同步

## 8. 开发指南

### 添加新的 API 端点

1. 在 `src/service/` 下创建或修改服务
2. 在 `src/*_handlers.rs` 下创建处理器
3. 在 `src/lib.rs` 中注册路由
4. 添加测试
5. 更新 API 文档

### 添加新的工具

1. 实现 `synthia_agent::tools::Tool` trait
2. 在 `src/setup.rs` 的 `register_tools()` 中注册
3. 添加单元测试
4. 更新工具文档

### 添加新的 MCP 服务器类型

1. 在 `src/mcp.rs` 中扩展 `McpServer` 实现
2. 更新 `McpServerConfig` 的验证逻辑
3. 添加测试
4. 更新 MCP 文档

## 9. 获取帮助

- 在 [Discussions](https://github.com/your-org/synthia/discussions) 中提问
- 加入我们的 [Discord](https://discord.gg/synthia) 社区
- 发送邮件至 maintainers@synthia.dev

## 10. 许可证

通过贡献代码，您同意您的贡献将根据项目的 MIT 许可证进行许可。
