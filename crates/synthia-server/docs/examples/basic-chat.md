---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 基础聊天示例

## 1. 概述

本示例演示如何使用 Synthia Server 进行基础的聊天交互。

## 2. Python 客户端

### 1.1 同步请求

```python
import requests

API_URL = "http://localhost:8080"
API_KEY = "your-api-key"

def chat(message: str, session_id: str = None) -> dict:
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {API_KEY}",
    }
    
    payload = {
        "message": message,
        "session_id": session_id,
    }
    
    response = requests.post(
        f"{API_URL}/chat",
        headers=headers,
        json=payload
    )
    response.raise_for_status()
    
    return response.json()

# 使用示例
result = chat("你好，请介绍一下你自己")
print(result["message"]["content"])

# 继续对话
result = chat("你能做什么？", session_id=result["session_id"])
print(result["message"]["content"])
```

### 1.2 流式响应

```python
import requests
import json

def chat_stream(message: str, session_id: str = None):
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {API_KEY}",
    }
    
    payload = {
        "message": message,
        "session_id": session_id,
        "stream": True,
    }
    
    with requests.post(
        f"{API_URL}/chat/stream",
        headers=headers,
        json=payload,
        stream=True
    ) as response:
        response.raise_for_status()
        
        for line in response.iter_lines():
            if line:
                line = line.decode('utf-8')
                if line.startswith('data: '):
                    data = json.loads(line[6:])
                    yield data

# 使用示例
for chunk in chat_stream("请写一首诗"):
    if "content" in chunk:
        print(chunk["content"], end="", flush=True)
```

### 1.3 WebSocket 客户端

```python
import websocket
import json
import threading

class AgentClient:
    def __init__(self, url: str, api_key: str):
        self.url = url
        self.api_key = api_key
        self.ws = None
        self.session_id = None
        
    def connect(self):
        self.ws = websocket.WebSocketApp(
            f"{self.url}/ws",
            header={"Authorization": f"Bearer {self.api_key}"},
            on_message=self._on_message,
            on_error=self._on_error,
            on_close=self._on_close,
        )
        
        thread = threading.Thread(target=self.ws.run_forever)
        thread.daemon = True
        thread.start()
    
    def _on_message(self, ws, message):
        data = json.loads(message)
        
        if data["type"] == "connected":
            self.session_id = data["session_id"]
            print(f"Connected: {self.session_id}")
        elif data["type"] == "message":
            print(data["payload"]["content"], end="", flush=True)
        elif data["type"] == "status":
            print(f"\nStatus: {data['payload']['status']}")
    
    def _on_error(self, ws, error):
        print(f"Error: {error}")
    
    def _on_close(self, ws, close_status_code, close_msg):
        print("Connection closed")
    
    def send(self, message: str):
        if self.ws:
            self.ws.send(json.dumps({
                "type": "chat",
                "payload": {"message": message}
            }))

# 使用示例
client = AgentClient("ws://localhost:8080", API_KEY)
client.connect()

import time
time.sleep(1)  # 等待连接

client.send("你好")
time.sleep(5)  # 等待响应
```

## 2. TypeScript 客户端

### 2.1 使用 Axios

```typescript
import axios from 'axios';

const API_URL = 'http://localhost:8080';
const API_KEY = 'your-api-key';

const client = axios.create({
  baseURL: API_URL,
  headers: {
    'Content-Type': 'application/json',
    'Authorization': `Bearer ${API_KEY}`,
  },
});

interface ChatResponse {
  session_id: string;
  message: {
    role: string;
    content: string;
  };
  status: string;
}

async function chat(
  message: string,
  sessionId?: string
): Promise<ChatResponse> {
  const response = await client.post('/chat', {
    message,
    session_id: sessionId,
  });
  return response.data;
}

// 使用示例
async function main() {
  let result = await chat('你好，请介绍一下你自己');
  console.log(result.message.content);

  result = await chat('你能做什么？', result.session_id);
  console.log(result.message.content);
}

main();
```

### 2.2 流式响应

```typescript
async function* chatStream(
  message: string,
  sessionId?: string
): AsyncGenerator<any> {
  const response = await fetch(`${API_URL}/chat/stream`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${API_KEY}`,
    },
    body: JSON.stringify({
      message,
      session_id: sessionId,
    }),
  });

  const reader = response.body?.getReader();
  const decoder = new TextDecoder();

  if (!reader) return;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    const chunk = decoder.decode(value);
    const lines = chunk.split('\n').filter(line => line.trim());

    for (const line of lines) {
      if (line.startsWith('data: ')) {
        yield JSON.parse(line.slice(6));
      }
    }
  }
}

// 使用示例
async function main() {
  for await (const chunk of chatStream('请写一首诗')) {
    if (chunk.content) {
      process.stdout.write(chunk.content);
    }
  }
}

main();
```

### 2.3 WebSocket 客户端

```typescript
class AgentClient {
  private ws: WebSocket | null = null;
  private sessionId: string | null = null;

  constructor(
    private url: string,
    private apiKey: string
  ) {}

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(`${this.url}/ws`, {
        headers: {
          Authorization: `Bearer ${this.apiKey}`,
        },
      });

      this.ws.onopen = () => {
        console.log('Connected');
        resolve();
      };

      this.ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        this.handleMessage(data);
      };

      this.ws.onerror = (error) => {
        console.error('Error:', error);
        reject(error);
      };

      this.ws.onclose = () => {
        console.log('Disconnected');
      };
    });
  }

  private handleMessage(data: any) {
    switch (data.type) {
      case 'connected':
        this.sessionId = data.session_id;
        console.log(`Session: ${this.sessionId}`);
        break;
      case 'message':
        process.stdout.write(data.payload.content);
        break;
      case 'status':
        console.log(`\nStatus: ${data.payload.status}`);
        break;
    }
  }

  send(message: string) {
    if (this.ws) {
      this.ws.send(JSON.stringify({
        type: 'chat',
        payload: { message },
      }));
    }
  }

  close() {
    this.ws?.close();
  }
}

// 使用示例
async function main() {
  const client = new AgentClient('ws://localhost:8080', API_KEY);
  await client.connect();
  
  client.send('你好');
  
  await new Promise(resolve => setTimeout(resolve, 5000));
  client.close();
}

main();
```

## 3. cURL 示例

### 3.1 基础请求

```bash
# 发送消息
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "message": "你好，请介绍一下你自己"
  }'

# 继续对话
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "message": "你能做什么？",
    "session_id": "session-id-from-previous-response"
  }'
```

### 3.2 流式响应

```bash
curl -X POST http://localhost:8080/chat/stream \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "message": "请写一首诗",
    "stream": true
  }'
```

### 3.3 指定 Agent

```bash
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "message": "请审查这段代码",
    "agent": "code-reviewer"
  }'
```

## 4. 相关文档

- [API使用指南](../api-reference/API_GUIDE.md)
- [前端集成](../integration/frontend-integration.md)
