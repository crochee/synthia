# MCP (Model Context Protocol) Tools

MCP 工具模块，提供对 Model Context Protocol 的支持，扩展 Agent 的工具能力。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `get_mcp_tools` | 获取 MCP 服务器工具 |

## 交互顺序

```
Agent启动 → 连接MCP服务器 → 获取工具列表 → 注册到工具箱 → Agent使用
```

## 在 Agent 中的作用

1. **协议扩展**: 通过 MCP 协议扩展工具
2. **动态工具**: 运行时发现可用工具
3. **标准化接口**: 统一的工具调用方式
4. **服务集成**: 集成外部服务能力

## Agent 运行机制

### MCP 连接流程

```
1. Agent 初始化
      ↓
2. 读取 MCP 配置
      ↓
3. 连接 MCP 服务器
      ↓
4. 获取可用工具列表
      ↓
5. 转换为内部工具格式
      ↓
6. 注册到工具注册表
```

### 工具调用

```
Agent → 调用MCP工具 → MCP客户端 → HTTP/stdio → MCP服务器 → 返回结果
```

## MCP 配置

MCP 服务器通常在配置文件中定义：

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/workspace"]
    }
  }
}
```

## 支持的 MCP 功能

- **Tools**: 外部工具调用
- **Resources**: 文件资源访问
- **Prompts**: 预定义提示模板

## 使用示例

### 动态获取工具

```rust
let mcp_tools = get_mcp_tools(config).await?;
for tool in mcp_tools {
    registry.register(tool).await;
}
```

## 设计理念

> 工具不应该被局限

MCP 系统解决的问题：
1. **扩展性**: 动态添加新工具
2. **标准化**: 统一的工具协议
3. **可移植性**: 工具可跨 Agent 使用
4. **服务化**: 工具作为服务提供

## 与内置工具的区别

| 特性 | MCP Tools | 内置 Tools |
|------|-----------|------------|
| 加载时机 | 运行时 | 编译时 |
| 扩展方式 | 动态连接 | 代码添加 |
| 协议 | MCP | 内部定义 |
| 配置方式 | 配置文件 | 代码配置 |
