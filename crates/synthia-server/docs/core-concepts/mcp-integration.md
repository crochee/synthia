---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# MCP 集成

## 1. 概述

MCP (Model Context Protocol) 是一种标准化的工具协议，允许 Agent 与外部工具和服务进行交互。Synthia Server 支持集成 MCP 服务器，扩展 Agent 的能力。

## 2. MCP 架构

### 2.1 MCP 服务器类型

| 类型 | 说明 | 使用场景 |
|------|------|----------|
| `stdio` | 通过标准输入输出通信 | 本地工具、命令行工具 |
| `sse` | 通过 Server-Sent Events 通信 | 远程服务、Web API |
| `http` | 通过 HTTP 通信 | REST API、Web 服务 |

### 2.2 MCP 组件

```
┌─────────────────────────────────────────────────────────────┐
│                      MCP Architecture                         │
│                                                              │
│  ┌──────────────┐                                            │
│  │   Synthia    │                                            │
│  │   Server     │                                            │
│  └──────┬───────┘                                            │
│         │                                                    │
│         │ MCP Protocol                                       │
│         │                                                    │
│  ┌──────▼───────┐     ┌──────────────┐     ┌──────────────┐ │
│  │  MCP Server  │     │  MCP Server  │     │  MCP Server  │ │
│  │  (filesystem)│     │   (github)   │     │   (custom)   │ │
│  └──────────────┘     └──────────────┘     └──────────────┘ │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## 3. MCP 服务器配置

### 3.1 配置示例

```yaml
mcps:
  filesystem:
    type: stdio
    description: "文件系统 MCP 服务器"
    command: "mcp-filesystem"
    args:
      - "/home/user/projects"
    env:
      LOG_LEVEL: "info"
    timeout: 300
    enabled: true
  
  github:
    type: stdio
    description: "GitHub MCP 服务器"
    command: "mcp-github"
    args: []
    env:
      GITHUB_TOKEN: "ghp_your_token"
    timeout: 300
    enabled: true
  
  remote-api:
    type: sse
    description: "远程 MCP 服务器"
    command: ""
    enabled: true
```

### 3.2 配置参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `type` | string | 否 | 服务器类型（默认：stdio） |
| `description` | string | 否 | 服务器描述 |
| `command` | string | 是 | 启动命令 |
| `args` | array | 否 | 命令参数 |
| `env` | map | 否 | 环境变量 |
| `timeout` | integer | 否 | 超时时间（秒，默认：300） |
| `enabled` | boolean | 否 | 是否启用（默认：true） |

## 4. MCP 服务器管理

### 4.1 列出 MCP 服务器

```bash
curl http://localhost:8080/mcp/servers
```

响应：

```json
[
  {
    "name": "filesystem",
    "status": "running",
    "description": "文件系统 MCP 服务器",
    "tools": ["read_file", "write_file", "list_directory"]
  }
]
```

### 4.2 注册 MCP 服务器

```bash
curl -X POST http://localhost:8080/mcp/servers \
  -H "Content-Type: application/json" \
  -d '{
    "name": "filesystem",
    "server_type": "stdio",
    "command": "mcp-filesystem",
    "args": ["/home/user"],
    "description": "文件系统 MCP 服务器",
    "timeout": 300,
    "enabled": true
  }'
```

### 4.3 列出 MCP 工具

```bash
curl http://localhost:8080/mcp/servers/filesystem/tools
```

响应：

```json
[
  {
    "name": "read_file",
    "description": "读取文件内容",
    "inputSchema": {
      "type": "object",
      "properties": {
        "path": { "type": "string" }
      },
      "required": ["path"]
    }
  }
]
```

### 4.4 注销 MCP 服务器

```bash
curl -X DELETE http://localhost:8080/mcp/servers/filesystem
```

## 5. MCP 服务器生命周期

### 5.1 生命周期状态

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Register   │────▶│    Start    │────▶│   Running   │
└─────────────┘     └─────────────┘     └─────────────┘
                                              │
                                              │
                                              ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Unregister │◀────│    Stop     │◀────│   Stopped   │
└─────────────┘     └─────────────┘     └─────────────┘
```

### 5.2 状态说明

| 状态 | 说明 |
|------|------|
| `running` | 服务器正在运行 |
| `stopped` | 服务器已停止 |
| `error` | 服务器出错 |
| `disabled` | 服务器已禁用 |

## 6. MCP 工具调用

### 6.1 自动发现

MCP 服务器的工具会自动注册到 ToolRegistry：

```rust
// MCP 工具自动添加到工具列表
let tools = agent.get_filtered_tools().await;
// tools 包含 MCP 服务器提供的工具
```

### 6.2 工具调用流程

```
┌─────────────────────────────────────────────────────────────┐
│                      MCP Tool Call                           │
│                                                              │
│  1. Agent 决定调用 MCP 工具                                  │
│     │                                                        │
│     ▼                                                        │
│  2. 查找 MCP 服务器                                          │
│     │                                                        │
│     ▼                                                        │
│  3. 通过 MCP 协议调用工具                                    │
│     ├── stdio: 写入标准输入                                  │
│     ├── sse: 发送 HTTP 请求                                  │
│     └── http: 发送 HTTP 请求                                 │
│     │                                                        │
│     ▼                                                        │
│  4. 接收工具结果                                             │
│     │                                                        │
│     ▼                                                        │
│  5. 返回给 Agent                                             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## 7. 常见 MCP 服务器

### 7.1 文件系统服务器

```yaml
mcps:
  filesystem:
    type: stdio
    command: "mcp-filesystem"
    args: ["/home/user/projects"]
```

**提供的工具**：
- `read_file`: 读取文件
- `write_file`: 写入文件
- `list_directory`: 列出目录
- `search_files`: 搜索文件

### 7.2 GitHub 服务器

```yaml
mcps:
  github:
    type: stdio
    command: "mcp-github"
    env:
      GITHUB_TOKEN: "${GITHUB_TOKEN}"
```

**提供的工具**：
- `create_issue`: 创建 Issue
- `create_pull_request`: 创建 PR
- `search_repositories`: 搜索仓库
- `get_file_contents`: 获取文件内容

### 7.3 数据库服务器

```yaml
mcps:
  postgres:
    type: stdio
    command: "mcp-postgres"
    args: ["postgresql://user:pass@localhost/db"]
```

**提供的工具**：
- `query`: 执行 SQL 查询
- `list_tables`: 列出表
- `describe_table`: 描述表结构

## 8. 开发自定义 MCP 服务器

### 8.1 MCP 服务器接口

```typescript
interface MCPServer {
  name: string;
  tools: MCPTool[];
  
  async callTool(name: string, args: any): Promise<MCPToolResult>;
  async listTools(): Promise<MCPTool[]>;
}
```

### 8.2 实现示例

```typescript
import { Server } from '@modelcontextprotocol/sdk';

const server = new Server({
  name: 'my-mcp-server',
  version: '1.0.0',
});

server.addTool({
  name: 'my_tool',
  description: 'My custom tool',
  inputSchema: {
    type: 'object',
    properties: {
      param: { type: 'string' },
    },
    required: ['param'],
  },
  async execute(args) {
    return {
      content: [{ type: 'text', text: `Result: ${args.param}` }],
    };
  },
});

server.start();
```

## 9. 最佳实践

### 9.1 安全配置

```yaml
mcps:
  filesystem:
    type: stdio
    command: "mcp-filesystem"
    args: ["/home/user/safe-directory"]  # 限制访问范围
    env:
      LOG_LEVEL: "warn"  # 避免泄露敏感信息
```

### 9.2 错误处理

```rust
// 处理 MCP 服务器错误
match mcp_server.call_tool(&tool_name, &args).await {
    Ok(result) => result,
    Err(e) => {
        tracing::error!("MCP tool call failed: {}", e);
        ToolResult::error(format!("MCP server error: {}", e))
    }
}
```

### 9.3 超时配置

```yaml
mcps:
  slow-service:
    type: http
    timeout: 600  # 增加超时时间
```

## 10. 故障排查

### 10.1 服务器启动失败

**症状**：MCP 服务器状态为 error

**排查步骤**：
1. 检查 `command` 是否正确
2. 检查命令是否在 PATH 中
3. 检查 `args` 参数是否正确
4. 检查 `env` 环境变量是否设置

### 10.2 工具调用超时

**症状**：工具调用长时间无响应

**排查步骤**：
1. 检查 `timeout` 配置
2. 检查 MCP 服务器是否正常运行
3. 检查网络连接（对于 sse/http 类型）

## 11. 相关文档

- [工具系统](tool-system.md)
- [配置说明](../configuration/CONFIGURATION.md#43-mcp-服务器配置)

## 12. 参考资料

- [MCP Specification](https://modelcontextprotocol.io/specification)
- [MCP SDK](https://github.com/modelcontextprotocol/sdk)
