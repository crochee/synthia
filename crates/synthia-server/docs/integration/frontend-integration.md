---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 前端集成

## 1. 概述

本文档说明如何将 Synthia Agent 集成到前端应用中，包括 REST API、WebSocket 和 SDK 使用。

## 2. REST API 集成

### 2.1 基础配置

```typescript
const API_BASE_URL = 'http://localhost:8080';

const apiClient = axios.create({
  baseURL: API_BASE_URL,
  headers: {
    'Content-Type': 'application/json',
    'Authorization': `Bearer ${API_KEY}`,
  },
});
```

### 2.2 聊天接口

```typescript
interface ChatRequest {
  message: string;
  session_id?: string;
  agent?: string;
  stream?: boolean;
}

interface ChatResponse {
  session_id: string;
  message: Message;
  status: AgentStatus;
}

async function chat(request: ChatRequest): Promise<ChatResponse> {
  const response = await apiClient.post('/chat', request);
  return response.data;
}

// 使用示例
const result = await chat({
  message: '请帮我审查这段代码',
  agent: 'code-reviewer',
});
```

### 2.3 流式响应

```typescript
async function* chatStream(
  request: ChatRequest
): AsyncGenerator<Message> {
  const response = await fetch(`${API_BASE_URL}/chat/stream`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${API_KEY}`,
    },
    body: JSON.stringify(request),
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
        const data = JSON.parse(line.slice(6));
        yield data;
      }
    }
  }
}

// 使用示例
for await (const message of chatStream({ message: 'Hello' })) {
  console.log(message.content);
}
```

## 3. WebSocket 集成

### 3.1 连接管理

```typescript
class SynthiaWebSocket {
  private ws: WebSocket | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;

  constructor(
    private url: string,
    private onMessage: (data: any) => void,
    private onStatusChange: (status: ConnectionStatus) => void
  ) {}

  connect() {
    this.ws = new WebSocket(this.url);

    this.ws.onopen = () => {
      console.log('WebSocket connected');
      this.reconnectAttempts = 0;
      this.onStatusChange('connected');
    };

    this.ws.onmessage = (event) => {
      const data = JSON.parse(event.data);
      this.onMessage(data);
    };

    this.ws.onclose = () => {
      console.log('WebSocket closed');
      this.onStatusChange('disconnected');
      this.reconnect();
    };

    this.ws.onerror = (error) => {
      console.error('WebSocket error:', error);
      this.onStatusChange('error');
    };
  }

  private reconnect() {
    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      this.reconnectAttempts++;
      console.log(`Reconnecting... Attempt ${this.reconnectAttempts}`);
      setTimeout(() => this.connect(), this.reconnectDelay * this.reconnectAttempts);
    }
  }

  send(message: any) {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    }
  }

  close() {
    this.ws?.close();
  }
}
```

### 3.2 消息处理

```typescript
interface WebSocketMessage {
  type: MessageType;
  payload: any;
}

type MessageType = 
  | 'chat'
  | 'tool_call'
  | 'tool_result'
  | 'status'
  | 'approval_request'
  | 'steering';

class AgentSession {
  private ws: SynthiaWebSocket;
  private sessionId: string;

  constructor(sessionId: string) {
    this.sessionId = sessionId;
    this.ws = new SynthiaWebSocket(
      `ws://localhost:8080/ws?session_id=${sessionId}`,
      this.handleMessage.bind(this),
      this.handleStatusChange.bind(this)
    );
    this.ws.connect();
  }

  private handleMessage(data: WebSocketMessage) {
    switch (data.type) {
      case 'chat':
        this.onChatMessage(data.payload);
        break;
      case 'tool_call':
        this.onToolCall(data.payload);
        break;
      case 'tool_result':
        this.onToolResult(data.payload);
        break;
      case 'status':
        this.onStatusChange(data.payload);
        break;
      case 'approval_request':
        this.onApprovalRequest(data.payload);
        break;
    }
  }

  sendChat(message: string) {
    this.ws.send({
      type: 'chat',
      payload: { message },
    });
  }

  respondApproval(approvalId: string, approved: boolean) {
    this.ws.send({
      type: 'approval_response',
      payload: { approval_id: approvalId, approved },
    });
  }

  sendSteering(message: string) {
    this.ws.send({
      type: 'steering',
      payload: { message },
    });
  }

  // 事件处理方法（由子类实现）
  protected onChatMessage(message: any) {}
  protected onToolCall(tool: any) {}
  protected onToolResult(result: any) {}
  protected onStatusChange(status: any) {}
  protected onApprovalRequest(request: any) {}
}
```

## 4. React 集成

### 4.1 自定义 Hook

```typescript
import { useState, useEffect, useCallback } from 'react';

interface UseAgentOptions {
  agent?: string;
  autoConnect?: boolean;
}

export function useAgent(options: UseAgentOptions = {}) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [status, setStatus] = useState<AgentStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const [session, setSession] = useState<AgentSession | null>(null);

  useEffect(() => {
    if (options.autoConnect !== false) {
      const newSession = new AgentSession(generateSessionId());
      setSession(newSession);
    }
  }, []);

  const sendMessage = useCallback(async (content: string) => {
    if (!session) return;

    setMessages(prev => [...prev, { role: 'user', content }]);
    setStatus('thinking');

    try {
      session.sendChat(content);
    } catch (err) {
      setError(err.message);
      setStatus('error');
    }
  }, [session]);

  const approveTool = useCallback((approvalId: string, approved: boolean) => {
    session?.respondApproval(approvalId, approved);
  }, [session]);

  const interrupt = useCallback((message: string) => {
    session?.sendSteering(message);
  }, [session]);

  return {
    messages,
    status,
    error,
    sendMessage,
    approveTool,
    interrupt,
  };
}
```

### 4.2 React 组件示例

```tsx
import React, { useState } from 'react';
import { useAgent } from './hooks/useAgent';

export function AgentChat() {
  const [input, setInput] = useState('');
  const {
    messages,
    status,
    error,
    sendMessage,
    approveTool,
    interrupt,
  } = useAgent({ agent: 'code-reviewer' });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (input.trim()) {
      sendMessage(input.trim());
      setInput('');
    }
  };

  return (
    <div className="agent-chat">
      <div className="messages">
        {messages.map((msg, idx) => (
          <div key={idx} className={`message ${msg.role}`}>
            {msg.content}
          </div>
        ))}
        {status === 'thinking' && (
          <div className="status">Agent is thinking...</div>
        )}
        {error && (
          <div className="error">{error}</div>
        )}
      </div>

      <form onSubmit={handleSubmit}>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Type your message..."
          disabled={status === 'thinking'}
        />
        <button type="submit" disabled={status === 'thinking'}>
          Send
        </button>
        {status === 'thinking' && (
          <button type="button" onClick={() => interrupt('Stop')}>
            Interrupt
          </button>
        )}
      </form>
    </div>
  );
}
```

## 5. Vue 集成

### 5.1 Composition API

```typescript
import { ref, onMounted, onUnmounted } from 'vue';

export function useAgent(options: UseAgentOptions = {}) {
  const messages = ref<Message[]>([]);
  const status = ref<AgentStatus>('idle');
  const error = ref<string | null>(null);
  const session = ref<AgentSession | null>(null);

  onMounted(() => {
    if (options.autoConnect !== false) {
      session.value = new AgentSession(generateSessionId());
    }
  });

  onUnmounted(() => {
    session.value?.close();
  });

  const sendMessage = async (content: string) => {
    if (!session.value) return;

    messages.value.push({ role: 'user', content });
    status.value = 'thinking';

    try {
      session.value.sendChat(content);
    } catch (err) {
      error.value = err.message;
      status.value = 'error';
    }
  };

  return {
    messages,
    status,
    error,
    sendMessage,
  };
}
```

### 5.2 Vue 组件示例

```vue
<template>
  <div class="agent-chat">
    <div class="messages">
      <div
        v-for="(msg, idx) in messages"
        :key="idx"
        :class="['message', msg.role]"
      >
        {{ msg.content }}
      </div>
      <div v-if="status === 'thinking'" class="status">
        Agent is thinking...
      </div>
      <div v-if="error" class="error">
        {{ error }}
      </div>
    </div>

    <form @submit.prevent="handleSubmit">
      <input
        v-model="input"
        type="text"
        placeholder="Type your message..."
        :disabled="status === 'thinking'"
      />
      <button type="submit" :disabled="status === 'thinking'">
        Send
      </button>
    </form>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { useAgent } from './composables/useAgent';

const input = ref('');
const { messages, status, error, sendMessage } = useAgent();

const handleSubmit = () => {
  if (input.value.trim()) {
    sendMessage(input.value.trim());
    input.value = '';
  }
};
</script>
```

## 6. 错误处理

### 6.1 错误类型

```typescript
enum AgentErrorCode {
  INVALID_INPUT = 'INVALID_INPUT',
  CONTEXT_TOO_LONG = 'CONTEXT_TOO_LONG',
  MODEL_ERROR = 'MODEL_ERROR',
  TOOL_ERROR = 'TOOL_ERROR',
  TIMEOUT = 'TIMEOUT',
  UNAUTHORIZED = 'UNAUTHORIZED',
  RATE_LIMIT = 'RATE_LIMIT',
}

interface AgentError {
  code: AgentErrorCode;
  message: string;
  details?: any;
}
```

### 6.2 错误处理策略

```typescript
function handleAgentError(error: AgentError): string {
  switch (error.code) {
    case AgentErrorCode.INVALID_INPUT:
      return '输入无效，请检查您的输入';
    
    case AgentErrorCode.CONTEXT_TOO_LONG:
      return '对话太长，请开始新会话';
    
    case AgentErrorCode.MODEL_ERROR:
      return '模型暂时不可用，请稍后重试';
    
    case AgentErrorCode.TOOL_ERROR:
      return `工具执行失败: ${error.message}`;
    
    case AgentErrorCode.TIMEOUT:
      return '请求超时，请重试';
    
    case AgentErrorCode.UNAUTHORIZED:
      return '未授权，请检查API密钥';
    
    case AgentErrorCode.RATE_LIMIT:
      return '请求过于频繁，请稍后重试';
    
    default:
      return '未知错误，请联系支持';
  }
}
```

## 7. 性能优化

### 7.1 消息分页

```typescript
function usePaginatedMessages(sessionId: string, pageSize = 50) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [page, setPage] = useState(0);
  const [hasMore, setHasMore] = useState(true);

  const loadMore = async () => {
    const response = await apiClient.get(`/sessions/${sessionId}/messages`, {
      params: { page, limit: pageSize },
    });

    setMessages(prev => [...response.data.messages, ...prev]);
    setHasMore(response.data.hasMore);
    setPage(prev => prev + 1);
  };

  return { messages, loadMore, hasMore };
}
```

### 7.2 消息缓存

```typescript
const messageCache = new Map<string, Message[]>();

async function getMessages(sessionId: string): Promise<Message[]> {
  if (messageCache.has(sessionId)) {
    return messageCache.get(sessionId)!;
  }

  const response = await apiClient.get(`/sessions/${sessionId}/messages`);
  messageCache.set(sessionId, response.data.messages);
  return response.data.messages;
}
```

## 8. 相关文档

- [API使用指南](../api-reference/API_GUIDE.md)
- [错误码表](../api-reference/ERROR_CODES.md)
- [配置说明](../configuration/CONFIGURATION.md)

## 9. 参考资料

- [OpenAI API Reference](https://platform.openai.com/docs/api-reference)
- [Anthropic API Reference](https://docs.anthropic.com/claude/reference)
- [WebSocket API](https://developer.mozilla.org/en-US/docs/Web/API/WebSocket)
