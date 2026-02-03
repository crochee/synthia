---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# Synthia Server API 使用指南

## 1. 快速开始

### 1.1 启动服务器

```bash
# 使用默认配置启动
synthia-server

# 指定工作目录和端口
synthia-server --directory /path/to/project --port 8080

# 完整参数
synthia-server \
  --directory /path/to/project \
  --host 0.0.0.0 \
  --port 8080
```

### 1.2 健康检查

```bash
curl http://localhost:8080/health
# 响应: OK
```

### 1.3 基本聊天

```bash
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "你好，请介绍一下你自己"}'
```

响应：
```json
{
  "message": "你好！我是 Synthia，一个 AI 助手...",
  "session_id": "session-abc123"
}
```

## 2. 认证

### 2.1 启用认证

在 `config.yaml` 中配置：

```yaml
auth:
  enabled: true
  api_keys:
    - "sk-your-api-key-here"
```

### 2.2 使用认证

```bash
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-your-api-key-here" \
  -d '{"message": "你好"}'
```

## 3. 聊天接口

### 3.1 同步聊天

**请求**：
```http
POST /chat
Content-Type: application/json

{
  "message": "请帮我分析这段代码",
  "session_id": "optional-session-id"
}
```

**响应**：
```json
{
  "message": "好的，我来分析这段代码...",
  "session_id": "session-abc123"
}
```

**参数说明**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| message | string | 是 | 用户消息 |
| session_id | string | 否 | 会话ID，不提供则创建新会话 |

### 3.2 流式聊天 (SSE)

**请求**：
```http
POST /chat/stream
Content-Type: application/json

{
  "message": "请帮我写一个排序算法",
  "session_id": "session-abc123"
}
```

**响应** (Server-Sent Events)：
```
data: {"type":"message","content":"好的","session_id":"session-abc123"}

data: {"type":"message","content":"，我来","session_id":"session-abc123"}

data: {"type":"message","content":"写一个快速排序算法...","session_id":"session-abc123"}

data: {"type":"status","status":"Completed","session_id":"session-abc123"}
```

**JavaScript 示例**：
```javascript
const eventSource = new EventSource('/chat/stream', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ message: '你好' })
});

// 注意：EventSource 不支持 POST，需要使用 fetch + ReadableStream
async function streamChat(message, sessionId) {
  const response = await fetch('/chat/stream', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message, session_id: sessionId })
  });

  const reader = response.body.getReader();
  const decoder = new TextDecoder();

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    const text = decoder.decode(value);
    const lines = text.split('\n');

    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const data = JSON.parse(line.slice(6));
        console.log(data);
      }
    }
  }
}
```

### 3.3 WebSocket 聊天

**连接**：
```javascript
const ws = new WebSocket('ws://localhost:8080/ws/session-abc123');

ws.onopen = () => {
  // 发送消息
  ws.send(JSON.stringify({
    action: 'chat',
    content: '你好'
  }));
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log(data);
  // { type: 'message', content: '...' }
  // { type: 'status', status: 'Completed' }
  // { type: 'error', error: '...' }
};

// 取消当前操作
ws.send(JSON.stringify({ action: 'cancel' }));
```

**消息格式**：

客户端发送：
```json
// 发送聊天消息
{"action": "chat", "content": "你好"}

// 取消当前操作
{"action": "cancel"}
```

服务端推送：
```json
// 消息事件
{"type": "message", "content": "..."}

// 状态事件
{"type": "status", "status": "Completed"}

// 错误事件
{"type": "error", "error": "..."}
```

## 4. 会话管理

### 4.1 创建会话

```bash
curl -X POST http://localhost:8080/sessions
```

响应：
```json
{
  "id": "session-abc123",
  "name": null,
  "created_at": 1704067200,
  "updated_at": 1704067200,
  "message_count": 0
}
```

### 4.2 列出会话

```bash
curl http://localhost:8080/sessions
```

响应：
```json
[
  {
    "id": "session-abc123",
    "name": "代码审查",
    "created_at": 1704067200,
    "updated_at": 1704067300,
    "message_count": 15
  },
  {
    "id": "session-def456",
    "name": null,
    "created_at": 1704067100,
    "updated_at": 1704067200,
    "message_count": 5
  }
]
```

### 4.3 获取会话详情

```bash
curl http://localhost:8080/sessions/session-abc123
```

### 4.4 获取会话消息

```bash
curl http://localhost:8080/sessions/session-abc123/messages
```

响应：
```json
[
  {
    "role": "user",
    "content": "请帮我分析这段代码"
  },
  {
    "role": "assistant",
    "content": "好的，我来分析..."
  }
]
```

### 4.5 压缩会话上下文

当会话消息过多时，可以压缩上下文以减少 token 使用：

```bash
curl -X POST http://localhost:8080/sessions/session-abc123/compact
```

响应：
```json
{
  "before_count": 50,
  "after_count": 10,
  "strategy": "Summary",
  "token_ratio_before": 0.85,
  "token_ratio_after": 0.25
}
```

### 4.6 删除会话

```bash
curl -X DELETE http://localhost:8080/sessions/session-abc123
```

响应：HTTP 204 No Content

## 5. 工具管理

### 5.1 列出工具

```bash
curl http://localhost:8080/tools
```

响应：
```json
[
  {
    "name": "read",
    "description": "读取文件内容",
    "parameters": {
      "type": "object",
      "properties": {
        "path": {
          "type": "string",
          "description": "文件路径"
        }
      },
      "required": ["path"]
    },
    "annotations": {
      "readOnly": true,
      "destructive": false,
      "concurrencySafe": true,
      "toolKind": "FileOperation"
    }
  }
]
```

### 5.2 获取工具详情

```bash
curl http://localhost:8080/tools/read
```

### 5.3 执行工具

```bash
curl -X POST http://localhost:8080/tools/read/execute \
  -H "Content-Type: application/json" \
  -d '{"arguments": {"path": "/home/user/file.txt"}}'
```

响应：
```json
{
  "success": true,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "文件内容..."
      }
    ]
  }
}
```

## 6. 技能管理

### 6.1 列出技能

```bash
curl http://localhost:8080/skills
```

响应：
```json
[
  {
    "name": "code-review",
    "description": "代码审查技能"
  },
  {
    "name": "test-generator",
    "description": "测试生成技能"
  }
]
```

### 6.2 添加技能

```bash
curl -X POST http://localhost:8080/skills \
  -H "Content-Type: application/json" \
  -d '{
    "name": "code-review",
    "path": ".trae/skills/code-review.md",
    "description": "代码审查技能"
  }'
```

### 6.3 获取技能详情

```bash
curl http://localhost:8080/skills/code-review
```

### 6.4 加载技能

```bash
curl -X POST http://localhost:8080/skills/code-review/load
```

响应：
```json
{
  "name": "code-review",
  "status": "loaded",
  "content": "# Code Review Skill\n\n## 指南\n..."
}
```

### 6.5 删除技能

```bash
curl -X DELETE http://localhost:8080/skills/code-review
```

## 7. MCP 服务器管理

### 7.1 列出 MCP 服务器

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

### 7.2 注册 MCP 服务器

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

### 7.3 列出 MCP 工具

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

### 7.4 注销 MCP 服务器

```bash
curl -X DELETE http://localhost:8080/mcp/servers/filesystem
```

## 8. 模型提供商管理

### 8.1 列出模型提供商

```bash
curl http://localhost:8080/models
```

响应：
```json
[
  {
    "name": "openai",
    "api_key": "sk-...",
    "base_url": "https://api.openai.com/v1",
    "models": [
      {
        "name": "gpt-4",
        "description": "GPT-4 模型",
        "context_window": 8192,
        "temperature": 0.7,
        "max_tokens": 4096
      }
    ]
  }
]
```

### 8.2 添加模型提供商

```bash
curl -X POST http://localhost:8080/models \
  -H "Content-Type: application/json" \
  -d '{
    "name": "openai",
    "api_key": "sk-your-api-key",
    "base_url": "https://api.openai.com/v1",
    "models": [
      {
        "name": "gpt-4",
        "description": "GPT-4 模型",
        "context_window": 8192
      }
    ]
  }'
```

### 8.3 获取模型详情

```bash
curl http://localhost:8080/models/openai/gpt-4
```

### 8.4 更新模型配置

```bash
curl -X PUT http://localhost:8080/models/openai/gpt-4 \
  -H "Content-Type: application/json" \
  -d '{
    "temperature": 0.5,
    "max_tokens": 2048
  }'
```

### 8.5 删除模型提供商

```bash
curl -X DELETE http://localhost:8080/models/openai
```

## 9. 错误处理

### 9.1 错误响应格式

```json
{
  "error": {
    "type": "not_found",
    "message": "Session 'abc123' not found"
  }
}
```

### 9.2 常见错误类型

| 错误类型 | HTTP 状态码 | 说明 |
|----------|-------------|------|
| bad_request | 400 | 请求参数错误 |
| unauthorized | 401 | 未授权 |
| forbidden | 403 | 禁止访问 |
| not_found | 404 | 资源不存在 |
| conflict | 409 | 资源冲突 |
| too_many_requests | 429 | 请求过于频繁 |
| internal_error | 500 | 内部错误 |
| service_unavailable | 503 | 服务不可用 |
| agent_error | 500 | Agent 执行错误 |
| mcp_error | 500 | MCP 服务器错误 |
| tool_error | 500 | 工具执行错误 |
| session_error | 500 | 会话错误 |
| config_error | 500 | 配置错误 |

### 9.3 错误处理示例

```javascript
async function chat(message) {
  try {
    const response = await fetch('/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message })
    });

    if (!response.ok) {
      const error = await response.json();
      console.error(`Error (${error.error.type}): ${error.error.message}`);
      return;
    }

    const data = await response.json();
    return data;
  } catch (e) {
    console.error('Network error:', e);
  }
}
```

## 10. 完整示例

详细的客户端代码示例请参考 [基础聊天示例](../examples/basic-chat.md)，包含：

- **Python 客户端**：同步请求、流式响应、WebSocket 连接
- **TypeScript 客户端**：Axios 封装、流式处理、WebSocket 客户端
- **cURL 示例**：基础请求、流式响应、指定 Agent

### 10.1 快速开始

最简单的调用方式：

```python
import requests

response = requests.post(
    "http://localhost:8080/chat",
    headers={"Content-Type": "application/json"},
    json={"message": "你好"}
)
print(response.json())
```

```typescript
const response = await fetch('http://localhost:8080/chat', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ message: '你好' }),
});
const data = await response.json();
console.log(data);
```

更多完整示例请查看 [基础聊天示例](../examples/basic-chat.md)。

## 11. 最佳实践

### 11.1 会话管理

- 对于长时间对话，使用固定的 `session_id` 保持上下文
- 定期调用 `/sessions/{id}/compact` 压缩上下文
- 不再需要的会话及时删除

### 11.2 流式响应

- 对于长响应，优先使用流式接口
- 正确处理 SSE 连接断开和重连
- 设置合理的超时时间

### 11.3 错误处理

- 始终检查 HTTP 状态码
- 解析错误响应中的 `error.type` 和 `error.message`
- 实现重试机制处理临时错误

### 11.4 安全

- 生产环境启用认证
- API Key 不要硬编码，使用环境变量
- 使用 HTTPS 加密传输

## 12. 常见问题

### Q: 如何保持对话上下文？

A: 在后续请求中使用第一次聊天返回的 `session_id`。

### Q: 流式响应如何处理超时？

A: 设置合理的读取超时，并实现心跳机制。

### Q: 如何限制 Agent 的行为？

A: 通过配置文件设置 `allowed_tools` 和 `denied_tools`。

### Q: 支持哪些模型提供商？

A: 支持所有兼容 OpenAI API 的提供商，包括 OpenAI、Azure OpenAI、本地模型等。

### Q: 如何调试工具执行？

A: 使用 `/tools/{name}/execute` 直接执行工具查看结果。
