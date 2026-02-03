---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 基本使用

## 1. 概述

本指南介绍 Synthia Server 的基本功能和使用方法。

## 2. 基本概念

### Agent

Agent 是 Synthia 的核心概念，是一个能够理解指令、执行工具和完成任务的 AI 助手。

### 会话 (Session)

会话是 Agent 与用户的交互上下文，包含对话历史和状态信息。

### 工具 (Tool)

工具是 Agent 可以调用的外部功能，如文件操作、网络请求等。

### 技能 (Skill)

技能是 Agent 的专业知识模块，提供特定领域的指导原则和最佳实践。

## 3. 发送消息

### 同步请求

```python
import requests

response = requests.post(
    "http://localhost:8080/chat",
    json={
        "message": "请帮我写一个 Python 函数计算斐波那契数列",
    }
)

result = response.json()
print(result["message"]["content"])
print(f"Session ID: {result['session_id']}")
```

### 流式响应

```python
import requests
import json

def stream_chat(message):
    with requests.post(
        "http://localhost:8080/chat/stream",
        json={"message": message, "stream": True},
        stream=True
    ) as response:
        for line in response.iter_lines():
            if line:
                line = line.decode('utf-8')
                if line.startswith('data: '):
                    data = json.loads(line[6:])
                    if 'content' in data:
                        yield data['content']

for chunk in stream_chat("请写一首诗"):
    print(chunk, end='', flush=True)
```

### 继续对话

```python
# 第一次请求
response1 = requests.post(
    "http://localhost:8080/chat",
    json={"message": "什么是机器学习？"}
)
session_id = response1.json()["session_id"]

# 继续对话
response2 = requests.post(
    "http://localhost:8080/chat",
    json={
        "message": "请举一个具体的例子",
        "session_id": session_id
    }
)
print(response2.json()["message"]["content"])
```

## 4. 使用特定 Agent

### 指定 Agent

```python
response = requests.post(
    "http://localhost:8080/chat",
    json={
        "message": "请审查这段代码",
        "agent": "code-reviewer"
    }
)
```

### 查看可用 Agent

```bash
curl http://localhost:8080/agents
```

响应：

```json
[
  {
    "name": "default",
    "description": "默认 Agent",
    "model": "gpt-4"
  },
  {
    "name": "code-reviewer",
    "description": "代码审查 Agent",
    "model": "gpt-4"
  }
]
```

## 5. 会话管理

### 创建会话

```bash
curl -X POST http://localhost:8080/sessions
```

### 获取会话信息

```bash
curl http://localhost:8080/sessions/{session_id}
```

### 列出所有会话

```bash
curl http://localhost:8080/sessions
```

### 删除会话

```bash
curl -X DELETE http://localhost:8080/sessions/{session_id}
```

## 6. 工具管理

### 查看可用工具

```bash
curl http://localhost:8080/tools
```

响应：

```json
[
  {
    "name": "read",
    "description": "读取文件内容",
    "annotations": {
      "read_only": true,
      "destructive": false
    }
  },
  {
    "name": "write",
    "description": "写入文件",
    "annotations": {
      "read_only": false,
      "destructive": true
    }
  }
]
```

### 配置工具权限

```yaml
agents:
  code-reviewer:
    allowed_tools:
      - read
      - grep
      - glob
    denied_tools:
      - write
      - delete
      - exec
```

## 7. 技能管理

### 查看可用技能

```bash
curl http://localhost:8080/skills
```

### 加载技能

```bash
curl -X POST http://localhost:8080/skills/{skill_name}/load
```

## 8. MCP 服务器管理

### 列出 MCP 服务器

```bash
curl http://localhost:8080/mcp/servers
```

### 注册 MCP 服务器

```bash
curl -X POST http://localhost:8080/mcp/servers \
  -H "Content-Type: application/json" \
  -d '{
    "name": "filesystem",
    "server_type": "stdio",
    "command": "mcp-filesystem",
    "args": ["/home/user/projects"]
  }'
```

### 查看 MCP 工具

```bash
curl http://localhost:8080/mcp/servers/{server_name}/tools
```

## 9. WebSocket 连接

### Python 客户端

```python
import websocket
import json

def on_message(ws, message):
    data = json.loads(message)
    print(data)

def on_error(ws, error):
    print(f"Error: {error}")

def on_close(ws, close_status_code, close_msg):
    print("Connection closed")

def on_open(ws):
    ws.send(json.dumps({
        "type": "chat",
        "payload": {"message": "你好"}
    }))

ws = websocket.WebSocketApp(
    "ws://localhost:8080/ws",
    on_open=on_open,
    on_message=on_message,
    on_error=on_error,
    on_close=on_close
)

ws.run_forever()
```

### JavaScript 客户端

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onopen = () => {
  ws.send(JSON.stringify({
    type: 'chat',
    payload: { message: '你好' }
  }));
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log(data);
};

ws.onerror = (error) => {
  console.error('Error:', error);
};

ws.onclose = () => {
  console.log('Connection closed');
};
```

## 10. 错误处理

### 错误响应格式

```json
{
  "error": {
    "code": "INVALID_INPUT",
    "message": "Invalid input parameters",
    "details": {
      "field": "message",
      "reason": "Message cannot be empty"
    }
  }
}
```

### 常见错误

| 错误码 | 说明 | 解决方法 |
|--------|------|----------|
| INVALID_INPUT | 输入参数无效 | 检查请求参数 |
| CONTEXT_TOO_LONG | 上下文过长 | 开始新会话或启用压缩 |
| MODEL_ERROR | 模型调用失败 | 检查 API 密钥和配额 |
| TOOL_ERROR | 工具执行失败 | 检查工具参数和权限 |

## 11. 最佳实践

### 1. 使用会话管理上下文

```python
# 好的做法：复用会话
session_id = None

def chat(message):
    global session_id
    response = requests.post(
        "http://localhost:8080/chat",
        json={"message": message, "session_id": session_id}
    )
    result = response.json()
    session_id = result["session_id"]
    return result["message"]["content"]
```

### 2. 使用流式响应提升体验

```python
# 好的做法：流式响应
for chunk in stream_chat("请写一个长故事"):
    print(chunk, end='', flush=True)

# 不好的做法：等待完整响应
response = chat("请写一个长故事")  # 用户等待时间长
print(response)
```

### 3. 合理配置工具权限

```yaml
# 好的做法：最小权限原则
agents:
  code-reviewer:
    allowed_tools: [read, grep, glob]
    denied_tools: [write, delete, exec]
```

### 4. 处理错误和重试

```python
import time

def chat_with_retry(message, max_retries=3):
    for attempt in range(max_retries):
        try:
            response = requests.post(
                "http://localhost:8080/chat",
                json={"message": message},
                timeout=30
            )
            response.raise_for_status()
            return response.json()
        except requests.exceptions.RequestException as e:
            if attempt < max_retries - 1:
                time.sleep(2 ** attempt)
            else:
                raise
```

## 12. 下一步

- [API使用指南](../api-reference/API_GUIDE.md) - 完整API文档
- [核心概念](../core-concepts/agent-execution.md) - 深入理解 Agent
- [示例](../examples/basic-chat.md) - 更多代码示例
