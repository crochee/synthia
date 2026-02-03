---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# Synthia Server 错误码表

## 1. 概述

Synthia Server 使用统一的错误响应格式，所有错误都包含 `error` 对象，其中包含 `type` 和 `message` 字段。

### 1.1 错误响应格式

```json
{
  "error": {
    "type": "error_type",
    "message": "详细错误信息"
  }
}
```

### 1.2 HTTP 状态码映射

| HTTP 状态码 | 说明 | 常见错误类型 |
|-------------|------|--------------|
| 400 | 请求参数错误 | bad_request |
| 401 | 未授权 | unauthorized |
| 403 | 禁止访问 | forbidden |
| 404 | 资源不存在 | not_found |
| 409 | 资源冲突 | conflict |
| 429 | 请求过于频繁 | too_many_requests |
| 500 | 服务器内部错误 | internal_error, agent_error, tool_error, etc. |
| 503 | 服务不可用 | service_unavailable |

## 2. 错误类型详解

### 2.1 bad_request (400)

请求参数无效或格式错误。

**常见原因**：
- 缺少必填字段
- 字段类型错误
- 字段值无效

**示例**：
```json
{
  "error": {
    "type": "bad_request",
    "message": "Missing required field: message"
  }
}
```

**常见场景**：

| 场景 | 错误消息 |
|------|----------|
| 聊天请求缺少消息 | Missing required field: message |
| 工具参数无效 | Invalid arguments: ... |
| MCP服务器类型无效 | Invalid server_type: must be one of: stdio, sse, http |
| URL格式无效 | Invalid base_url: must be a valid HTTP or HTTPS URL |

### 2.2 unauthorized (401)

未提供认证信息或认证信息无效。

**常见原因**：
- 未提供 Authorization 头
- Bearer Token 格式错误
- API Key 无效

**示例**：
```json
{
  "error": {
    "type": "unauthorized",
    "message": "Missing or invalid Authorization header"
  }
}
```

```json
{
  "error": {
    "type": "unauthorized",
    "message": "Invalid API key"
  }
}
```

### 2.3 forbidden (403)

已认证但无权访问资源。

**常见原因**：
- 权限不足
- 资源被锁定

**示例**：
```json
{
  "error": {
    "type": "forbidden",
    "message": "You do not have permission to access this resource"
  }
}
```

### 2.4 not_found (404)

请求的资源不存在。

**常见原因**：
- 会话ID不存在
- 工具名称不存在
- 技能名称不存在
- MCP服务器不存在
- 模型不存在

**示例**：
```json
{
  "error": {
    "type": "not_found",
    "message": "Session 'abc123' not found"
  }
}
```

```json
{
  "error": {
    "type": "not_found",
    "message": "Tool 'unknown_tool' not found"
  }
}
```

**常见场景**：

| 场景 | 错误消息模板 |
|------|-------------|
| 会话不存在 | Session '{id}' not found |
| 工具不存在 | Tool '{name}' not found |
| 技能不存在 | Skill '{name}' not found |
| MCP服务器不存在 | MCP server '{name}' not found |
| 提供商不存在 | Provider '{name}' not found |
| 模型不存在 | Model in provider '{provider}' '{name}' not found |

### 2.5 conflict (409)

资源冲突，通常是因为资源已存在。

**常见原因**：
- 尝试创建已存在的资源
- 并发修改冲突

**示例**：
```json
{
  "error": {
    "type": "conflict",
    "message": "Skill 'code-review' already exists"
  }
}
```

```json
{
  "error": {
    "type": "conflict",
    "message": "Provider 'openai' already exists"
  }
}
```

**常见场景**：

| 场景 | 错误消息模板 |
|------|-------------|
| 技能已存在 | Skill '{name}' already exists |
| 提供商已存在 | Provider '{name}' already exists |
| MCP服务器已存在 | MCP server '{name}' already registered |

### 2.6 too_many_requests (429)

请求频率超过限制。

**常见原因**：
- 超过速率限制
- 并发请求过多

**示例**：
```json
{
  "error": {
    "type": "too_many_requests",
    "message": "Rate limit exceeded. Please try again later."
  }
}
```

### 2.7 internal_error (500)

服务器内部错误。

**常见原因**：
- 未预期的异常
- 系统资源不足
- 依赖服务故障

**示例**：
```json
{
  "error": {
    "type": "internal_error",
    "message": "An unexpected error occurred: ..."
  }
}
```

### 2.8 agent_error (500)

Agent 执行过程中发生错误。

**常见原因**：
- LLM API 调用失败
- 上下文处理错误
- Agent 内部错误

**示例**：
```json
{
  "error": {
    "type": "agent_error",
    "message": "Failed to get response from LLM: API timeout"
  }
}
```

**常见场景**：

| 场景 | 可能的错误消息 |
|------|---------------|
| LLM API 错误 | Failed to get response from LLM: ... |
| 上下文超限 | Context length exceeded |
| 会话错误 | Session operation failed: ... |

### 2.9 mcp_error (500)

MCP 服务器相关错误。

**常见原因**：
- MCP 服务器启动失败
- MCP 服务器通信错误
- MCP 工具执行错误

**示例**：
```json
{
  "error": {
    "type": "mcp_error",
    "message": "Failed to start MCP server 'filesystem': command not found"
  }
}
```

```json
{
  "error": {
    "type": "mcp_error",
    "message": "Failed to list tools from MCP server 'github': connection refused"
  }
}
```

**常见场景**：

| 场景 | 可能的错误消息 |
|------|---------------|
| 服务器启动失败 | Failed to start MCP server '{name}': ... |
| 服务器停止失败 | Failed to stop MCP server '{name}': ... |
| 工具列表获取失败 | Failed to list tools from MCP server '{name}': ... |
| 服务器注册失败 | Failed to register MCP server: ... |

### 2.10 tool_error (500)

工具执行错误。

**常见原因**：
- 工具参数验证失败
- 工具执行异常
- 工具返回错误

**示例**：
```json
{
  "error": {
    "type": "tool_error",
    "message": "Tool 'read' failed: File not found: /path/to/file"
  }
}
```

```json
{
  "error": {
    "type": "tool_error",
    "message": "Tool 'exec' failed: Command returned non-zero exit code"
  }
}
```

**常见场景**：

| 场景 | 可能的错误消息 |
|------|---------------|
| 文件操作失败 | Tool '{name}' failed: File not found: ... |
| 权限不足 | Tool '{name}' failed: Permission denied |
| 命令执行失败 | Tool '{name}' failed: Command failed: ... |
| 技能加载失败 | Failed to load skill '{name}': ... |

### 2.11 session_error (500)

会话操作错误。

**常见原因**：
- 会话创建失败
- 会话存储错误
- 会话加载错误

**示例**：
```json
{
  "error": {
    "type": "session_error",
    "message": "Failed to create session: storage error"
  }
}
```

```json
{
  "error": {
    "type": "session_error",
    "message": "Failed to save session: disk full"
  }
}
```

### 2.12 config_error (500)

配置相关错误。

**常见原因**：
- 配置文件格式错误
- 配置加载失败
- 配置保存失败

**示例**：
```json
{
  "error": {
    "type": "config_error",
    "message": "Failed to load config: invalid YAML format"
  }
}
```

```json
{
  "error": {
    "type": "config_error",
    "message": "Failed to save config: permission denied"
  }
}
```

### 2.13 service_unavailable (503)

服务暂时不可用。

**常见原因**：
- 服务正在启动
- 服务正在维护
- 依赖服务不可用

**示例**：
```json
{
  "error": {
    "type": "service_unavailable",
    "message": "Service is temporarily unavailable. Please try again later."
  }
}
```

## 3. 错误处理最佳实践

### 3.1 客户端错误处理

```python
import requests

def handle_error(response):
    if response.ok:
        return response.json()
    
    try:
        error = response.json().get('error', {})
        error_type = error.get('type', 'unknown')
        error_message = error.get('message', 'Unknown error')
    except:
        error_type = 'unknown'
        error_message = response.text
    
    if response.status_code == 400:
        print(f"请求错误 ({error_type}): {error_message}")
        # 检查请求参数
    elif response.status_code == 401:
        print(f"认证失败 ({error_type}): {error_message}")
        # 检查 API Key
    elif response.status_code == 404:
        print(f"资源不存在 ({error_type}): {error_message}")
        # 检查资源 ID
    elif response.status_code == 429:
        print(f"请求过于频繁 ({error_type}): {error_message}")
        # 等待后重试
    elif response.status_code >= 500:
        print(f"服务器错误 ({error_type}): {error_message}")
        # 重试或联系管理员
    
    raise Exception(f"{error_type}: {error_message}")
```

### 3.2 重试策略

```python
import time
import random

def retry_request(func, max_retries=3, base_delay=1):
    for attempt in range(max_retries):
        try:
            return func()
        except Exception as e:
            if attempt == max_retries - 1:
                raise
            
            # 指数退避 + 随机抖动
            delay = base_delay * (2 ** attempt) + random.uniform(0, 1)
            time.sleep(delay)
```

### 3.3 错误日志记录

```python
import logging

def log_error(response, context=None):
    error = response.json().get('error', {})
    logging.error(
        "API Error - Type: %s, Message: %s, Status: %d, Context: %s",
        error.get('type'),
        error.get('message'),
        response.status_code,
        context
    )
```

## 4. 错误码速查表

| 错误类型 | HTTP 状态码 | 说明 | 常见原因 |
|----------|-------------|------|----------|
| bad_request | 400 | 请求参数错误 | 缺少必填字段、参数格式错误 |
| unauthorized | 401 | 未授权 | 缺少认证头、API Key 无效 |
| forbidden | 403 | 禁止访问 | 权限不足 |
| not_found | 404 | 资源不存在 | ID 或名称错误 |
| conflict | 409 | 资源冲突 | 资源已存在 |
| too_many_requests | 429 | 请求过于频繁 | 超过速率限制 |
| internal_error | 500 | 内部错误 | 未预期异常 |
| agent_error | 500 | Agent 错误 | LLM 调用失败 |
| mcp_error | 500 | MCP 错误 | MCP 服务器故障 |
| tool_error | 500 | 工具错误 | 工具执行失败 |
| session_error | 500 | 会话错误 | 会话操作失败 |
| config_error | 500 | 配置错误 | 配置文件问题 |
| service_unavailable | 503 | 服务不可用 | 服务维护中 |

## 5. 常见错误排查

### 5.1 认证错误排查

**错误**: `unauthorized: Missing or invalid Authorization header`

**排查步骤**:
1. 检查是否启用了认证 (`auth.enabled: true`)
2. 检查 Authorization 头格式: `Authorization: Bearer <api_key>`
3. 检查 API Key 是否正确

### 5.2 资源不存在错误排查

**错误**: `not_found: Session 'xxx' not found`

**排查步骤**:
1. 检查会话 ID 是否正确
2. 检查会话是否已被删除
3. 列出所有会话确认 ID

### 5.3 工具执行错误排查

**错误**: `tool_error: Tool 'read' failed: File not found`

**排查步骤**:
1. 检查文件路径是否正确
2. 检查工作目录设置
3. 检查文件权限

### 5.4 MCP 服务器错误排查

**错误**: `mcp_error: Failed to start MCP server 'xxx': command not found`

**排查步骤**:
1. 检查 MCP 服务器命令是否正确
2. 检查命令是否在 PATH 中
3. 检查 MCP 服务器是否已安装

### 5.5 Agent 错误排查

**错误**: `agent_error: Failed to get response from LLM: API timeout`

**排查步骤**:
1. 检查网络连接
2. 检查 API Key 是否有效
3. 检查 API 配额是否用尽
4. 检查 base_url 是否正确
