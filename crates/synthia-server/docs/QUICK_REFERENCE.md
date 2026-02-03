---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 快速参考

本文档提供 Synthia Server 的快速参考信息。

## 1. 常用 API 端点

### 健康检查

```bash
curl http://localhost:8080/health
```

### 聊天

```bash
# 同步聊天
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{"message": "你好"}'

# 流式聊天
curl -X POST http://localhost:8080/chat/stream \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{"message": "你好"}'
```

### 会话管理

```bash
# 列出会话
curl http://localhost:8080/sessions \
  -H "Authorization: Bearer your-api-key"

# 获取会话
curl http://localhost:8080/sessions/{session_id} \
  -H "Authorization: Bearer your-api-key"

# 删除会话
curl -X DELETE http://localhost:8080/sessions/{session_id} \
  -H "Authorization: Bearer your-api-key"

# 压缩会话
curl -X POST http://localhost:8080/sessions/{session_id}/compact \
  -H "Authorization: Bearer your-api-key"
```

### 工具管理

```bash
# 列出工具
curl http://localhost:8080/tools \
  -H "Authorization: Bearer your-api-key"

# 执行工具
curl -X POST http://localhost:8080/tools/{name}/execute \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{"arguments": {"path": "/tmp/test.txt"}}'
```

### 技能管理

```bash
# 列出技能
curl http://localhost:8080/skills \
  -H "Authorization: Bearer your-api-key"

# 加载技能
curl -X POST http://localhost:8080/skills/{name}/load \
  -H "Authorization: Bearer your-api-key"
```

## 2. API 端点速查表

| 端点 | 方法 | 说明 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/chat` | POST | 同步聊天 |
| `/chat/stream` | POST | 流式聊天 (SSE) |
| `/sessions` | GET | 列出会话 |
| `/sessions` | POST | 创建会话 |
| `/sessions/{id}` | GET | 获取会话 |
| `/sessions/{id}` | DELETE | 删除会话 |
| `/sessions/{id}/compact` | POST | 压缩会话 |
| `/sessions/{id}/messages` | GET | 获取会话消息 |
| `/tools` | GET | 列出工具 |
| `/tools/{name}` | GET | 获取工具信息 |
| `/tools/{name}/execute` | POST | 执行工具 |
| `/skills` | GET | 列出技能 |
| `/skills` | POST | 添加技能 |
| `/skills/{name}` | GET | 获取技能 |
| `/skills/{name}` | DELETE | 删除技能 |
| `/skills/{name}/load` | POST | 加载技能 |
| `/mcp/servers` | GET | 列出 MCP 服务器 |
| `/mcp/servers` | POST | 注册 MCP 服务器 |
| `/mcp/servers/{name}` | DELETE | 注销 MCP 服务器 |
| `/models` | GET | 列出模型提供商 |
| `/models` | POST | 添加模型提供商 |
| `/ws/{session_id}` | GET | WebSocket 连接 |

## 3. 常用配置

### 最小配置

```yaml
providers:
  openai:
    api_key: "${OPENAI_API_KEY}"
    models:
      - name: "gpt-4"
```

### 完整配置示例

```yaml
version: "1.0"
host: "0.0.0.0"
port: 8080

providers:
  openai:
    api_key: "${OPENAI_API_KEY}"
    models:
      - name: "gpt-4"
        max_tokens: 4096
      - name: "gpt-3.5-turbo"
  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"
    models:
      - name: "claude-3-opus"

mcps:
  filesystem:
    type: stdio
    command: "mcp-filesystem"
    args: ["--root", "/workspace"]

agents:
  code-reviewer:
    model: "claude-3-opus"
    max_steps: 50
    allowed_tools: ["read", "grep", "glob"]

auth:
  enabled: true
  api_keys:
    - "sk-server-xxx"

rate_limit:
  enabled: true
  requests_per_minute: 60
```

### 启用认证

```yaml
auth:
  enabled: true
  api_keys:
    - "your-api-key-1"
    - "your-api-key-2"
```

### 配置子 Agent

```yaml
agents:
  code-reviewer:
    model: "claude-3-opus"
    max_steps: 50
    system_prompt: "You are a code reviewer..."
    allowed_tools: ["read", "grep", "glob"]
    denied_tools: ["exec", "delete"]
```

## 4. 常见错误码

| 错误码 | HTTP 状态 | 说明 |
|--------|-----------|------|
| `bad_request` | 400 | 请求参数错误 |
| `unauthorized` | 401 | 未授权 |
| `forbidden` | 403 | 禁止访问 |
| `not_found` | 404 | 资源不存在 |
| `conflict` | 409 | 资源冲突 |
| `too_many_requests` | 429 | 请求过于频繁 |
| `internal_error` | 500 | 内部错误 |
| `service_unavailable` | 503 | 服务不可用 |

### 错误响应格式

```json
{
  "error": {
    "type": "not_found",
    "message": "Session 'abc123' not found"
  }
}
```

## 5. WebSocket 消息格式

### 客户端发送

```json
{"action": "chat", "content": "你好"}
{"action": "cancel"}
```

### 服务器响应

```json
{"type": "connected", "session_id": "xxx"}
{"type": "message", "content": "..."}
{"type": "status", "status": "Completed"}
{"type": "error", "message": "..."}
```

## 6. SSE 事件格式

```
data: {"type":"message","content":"Hello"}

data: {"type":"status","status":"Completed"}
```

## 7. 环境变量

| 变量 | 说明 |
|------|------|
| `OPENAI_API_KEY` | OpenAI API 密钥 |
| `ANTHROPIC_API_KEY` | Anthropic API 密钥 |
| `SYNTHIA_CONFIG` | 配置文件路径 |
| `SYNTHIA_HOST` | 监听地址 |
| `SYNTHIA_PORT` | 监听端口 |
| `RUST_LOG` | 日志级别 |

## 8. 命令行参数

```bash
synthia-server [OPTIONS]

Options:
  -c, --config <FILE>  配置文件路径 [default: config.yaml]
  -H, --host <HOST>    监听地址 [default: 127.0.0.1]
  -p, --port <PORT>    监听端口 [default: 8080]
  -h, --help           显示帮助信息
  -V, --version        显示版本信息
```

## 9. 内置工具列表

| 工具 | 说明 | 类型 |
|------|------|------|
| `read` | 读取文件 | 只读 |
| `write` | 写入文件 | 写入 |
| `edit` | 编辑文件 | 写入 |
| `delete` | 删除文件 | 破坏性 |
| `move` | 移动文件 | 破坏性 |
| `grep` | 搜索内容 | 只读 |
| `glob` | 查找文件 | 只读 |
| `list_directory` | 列出目录 | 只读 |
| `exec` | 执行命令 | 执行 |
| `web_search` | Web 搜索 | 只读 |
| `web_fetch` | 获取网页 | 只读 |
| `TodoWrite` | 任务管理 | 写入 |

## 10. 相关文档

- [API 使用指南](api-reference/API_GUIDE.md)
- [配置说明](configuration/CONFIGURATION.md)
- [架构文档](architecture/ARCHITECTURE.md)
- [错误码表](api-reference/ERROR_CODES.md)
