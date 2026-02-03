---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# Synthia Server 配置说明

## 1. 概述

Synthia Server 使用 YAML 格式的配置文件，默认从工作目录下的 `config.yaml` 加载。配置文件支持以下主要部分：

- 服务器基础配置
- 模型提供商配置
- MCP 服务器配置
- 子 Agent 配置
- 技能配置
- 认证配置
- 限流配置

## 2. 配置文件位置

配置文件按以下顺序查找：

1. 命令行参数 `--directory` 指定的工作目录下的 `config.yaml`
2. 当前工作目录下的 `config.yaml`
3. 如果未找到，使用默认配置

## 3. 完整配置示例

```yaml
# 服务器基础配置
version: "1.0"
host: "127.0.0.1"
port: 8080
model_override: null
max_agents: 5

# 模型提供商配置
providers:
  openai:
    api_key: "sk-your-openai-api-key"
    base_url: "https://api.openai.com/v1"
    models:
      - name: "gpt-4"
        description: "GPT-4 模型"
        context_window: 8192
        temperature: 0.7
        max_tokens: 4096
      - name: "gpt-4-turbo"
        description: "GPT-4 Turbo 模型"
        context_window: 128000
        temperature: 0.7
        max_tokens: 4096
  
  anthropic:
    api_key: "sk-ant-your-anthropic-api-key"
    base_url: "https://api.anthropic.com/v1"
    models:
      - name: "claude-3-opus"
        description: "Claude 3 Opus"
        context_window: 200000
        temperature: 0.7

# MCP 服务器配置
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

# 子 Agent 配置
agents:
  code-reviewer:
    description: "代码审查 Agent"
    model: "gpt-4"
    max_steps: 50
    allowed_tools:
      - read
      - grep
      - glob
    denied_tools:
      - exec
      - write
    hidden: false
    color: "blue"
  
  test-generator:
    description: "测试生成 Agent"
    model: "gpt-4"
    max_steps: 30
    allowed_tools:
      - read
      - write
      - grep
    denied_tools: []
    hidden: false

# 技能配置
skills:
  - name: "code-review"
    path: ".trae/skills/code-review.md"
  - name: "test-generator"
    path: ".trae/skills/test-generator.md"

# 认证配置
auth:
  enabled: true
  api_keys:
    - "sk-server-key-1"
    - "sk-server-key-2"

# 限流配置
rate_limit:
  enabled: true
  requests_per_minute: 60
  burst: 10
```

## 4. 配置项详解

### 4.1 服务器基础配置

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `version` | string | "1.0" | 配置文件版本 |
| `host` | string | "127.0.0.1" | 监听地址 |
| `port` | integer | 8080 | 监听端口 |
| `model_override` | string | null | 强制使用的模型（覆盖 Agent 配置） |
| `max_agents` | integer | 5 | 最大并发子 Agent 数量 |

**示例**：
```yaml
version: "1.0"
host: "0.0.0.0"      # 监听所有网卡
port: 8080
model_override: "gpt-4"  # 强制使用 gpt-4
max_agents: 10       # 允许最多 10 个并发子 Agent
```

### 4.2 模型提供商配置

`providers` 是一个映射，键为提供商名称，值为提供商配置。

#### ProviderConfig

| 配置项 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `api_key` | string | 否 | API 密钥 |
| `base_url` | string | 否 | API 基础 URL |
| `models` | array | 否 | 模型列表 |

#### ModelConfig

| 配置项 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `name` | string | 是 | 模型名称 |
| `description` | string | 否 | 模型描述 |
| `context_window` | integer | 否 | 上下文窗口大小 |
| `temperature` | float | 否 | 温度参数 (0.0-2.0) |
| `max_tokens` | integer | 否 | 最大输出 token 数 |

**示例**：
```yaml
providers:
  openai:
    api_key: "${OPENAI_API_KEY}"  # 支持环境变量
    base_url: "https://api.openai.com/v1"
    models:
      - name: "gpt-4"
        description: "GPT-4"
        context_window: 8192
        temperature: 0.7
        max_tokens: 4096
  
  # 本地模型示例
  local:
    api_key: "not-needed"
    base_url: "http://localhost:11434/v1"
    models:
      - name: "llama2"
        description: "Llama 2 本地模型"
        context_window: 4096
```

### 4.3 MCP 服务器配置

`mcps` 是一个映射，键为服务器名称，值为服务器配置。

#### McpConfig

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `type` | string | "stdio" | 服务器类型: stdio, sse, http |
| `description` | string | null | 服务器描述 |
| `command` | string | 必填 | 启动命令 |
| `args` | array | [] | 命令参数 |
| `env` | map | {} | 环境变量 |
| `timeout` | integer | 300 | 超时时间（秒） |
| `enabled` | boolean | true | 是否启用 |

**示例**：
```yaml
mcps:
  filesystem:
    type: stdio
    command: "mcp-filesystem"
    args: ["/home/user"]
    env:
      LOG_LEVEL: "debug"
    timeout: 300
    enabled: true
  
  # SSE 类型示例
  remote-server:
    type: sse
    command: ""
    description: "远程 MCP 服务器"
    enabled: true
```

### 4.4 子 Agent 配置

`agents` 是一个映射，键为 Agent 名称，值为 Agent 配置。

#### AgentConfig

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `description` | string | null | Agent 描述 |
| `model` | string | null | 使用的模型 |
| `max_steps` | integer | null | 最大执行步数 |
| `allowed_tools` | array | [] | 允许使用的工具列表 |
| `denied_tools` | array | [] | 禁止使用的工具列表 |
| `hidden` | boolean | false | 是否隐藏（不在列表中显示） |
| `color` | string | null | 终端显示颜色 |

**示例**：
```yaml
agents:
  code-reviewer:
    description: "专门用于代码审查的 Agent"
    model: "gpt-4"
    max_steps: 50
    allowed_tools:
      - read
      - grep
      - glob
    denied_tools:
      - exec
      - write
      - delete
    hidden: false
    color: "blue"
  
  test-writer:
    description: "测试编写 Agent"
    model: "gpt-4"
    allowed_tools:
      - read
      - write
      - grep
```

### 4.5 技能配置

`skills` 是一个数组，每个元素是一个技能配置。

#### SkillConfig

| 配置项 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `name` | string | 是 | 技能名称 |
| `path` | string | 是 | 技能文件路径（相对于工作目录） |

**示例**：
```yaml
skills:
  - name: "code-review"
    path: ".trae/skills/code-review.md"
  - name: "test-generator"
    path: ".trae/skills/test-generator.md"
  - name: "documentation"
    path: ".trae/skills/documentation.md"
```

### 4.6 认证配置

#### AuthConfig

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `enabled` | boolean | false | 是否启用认证 |
| `api_keys` | array | [] | 有效的 API Key 列表 |

**示例**：
```yaml
auth:
  enabled: true
  api_keys:
    - "sk-server-key-1"
    - "sk-server-key-2"
    - "${API_KEY_FROM_ENV}"  # 支持环境变量
```

### 4.7 限流配置

#### RateLimitConfig

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `enabled` | boolean | false | 是否启用限流 |
| `requests_per_minute` | integer | 60 | 每分钟请求数限制 |
| `burst` | integer | 10 | 突发请求数 |

**示例**：
```yaml
rate_limit:
  enabled: true
  requests_per_minute: 60
  burst: 10
```

## 5. 环境变量

配置文件支持环境变量替换，格式为 `${ENV_VAR_NAME}`。

**示例**：
```yaml
providers:
  openai:
    api_key: "${OPENAI_API_KEY}"
    base_url: "${OPENAI_BASE_URL:-https://api.openai.com/v1}"  # 支持默认值

auth:
  api_keys:
    - "${SERVER_API_KEY}"
```

## 6. 配置优先级

配置值按以下优先级确定（从高到低）：

1. 命令行参数
2. 配置文件
3. 默认值

**示例**：
```bash
# 配置文件中 port: 8080
# 命令行 --port 3000
# 实际使用端口: 3000
synthia-server --port 3000
```

## 7. 配置热更新

部分配置支持运行时更新：

| 配置项 | 支持热更新 | 说明 |
|--------|------------|------|
| `providers` | 是 | 通过 API 添加/删除提供商 |
| `skills` | 是 | 通过 API 添加/删除技能 |
| `mcps` | 部分 | 可注册新服务器，已启动的服务器需重启 |
| `agents` | 否 | 需要重启服务 |
| `auth` | 否 | 需要重启服务 |
| `rate_limit` | 否 | 需要重启服务 |

## 8. 配置验证

### 8.1 必填字段

以下字段为必填：

- `providers.*.models[].name` - 模型名称
- `mcps.*.command` - MCP 服务器启动命令
- `skills[].name` - 技能名称
- `skills[].path` - 技能文件路径

### 8.2 值约束

| 配置项 | 约束 |
|--------|------|
| `port` | 1-65535 |
| `max_agents` | > 0 |
| `temperature` | 0.0-2.0 |
| `context_window` | > 0 |
| `max_tokens` | > 0 |
| `timeout` | > 0 |

### 8.3 路径验证

- `skills[].path` - 相对于工作目录的有效路径
- `mcps.*.command` - 可执行命令或路径

## 9. 配置示例

### 9.1 开发环境配置

```yaml
version: "1.0"
host: "127.0.0.1"
port: 8080
max_agents: 3

providers:
  openai:
    api_key: "${OPENAI_API_KEY}"
    models:
      - name: "gpt-4"
        context_window: 8192

mcps:
  filesystem:
    type: stdio
    command: "mcp-filesystem"
    args: ["."]

auth:
  enabled: false

rate_limit:
  enabled: false
```

### 9.2 生产环境配置

```yaml
version: "1.0"
host: "0.0.0.0"
port: 8080
max_agents: 10

providers:
  openai:
    api_key: "${OPENAI_API_KEY}"
    base_url: "https://api.openai.com/v1"
    models:
      - name: "gpt-4"
        context_window: 8192
        temperature: 0.7
        max_tokens: 4096

mcps:
  filesystem:
    type: stdio
    command: "/usr/local/bin/mcp-filesystem"
    args: ["/app/workspace"]
    timeout: 300

agents:
  code-reviewer:
    description: "代码审查 Agent"
    model: "gpt-4"
    max_steps: 50

auth:
  enabled: true
  api_keys:
    - "${SERVER_API_KEY}"

rate_limit:
  enabled: true
  requests_per_minute: 60
  burst: 10
```

### 9.3 多提供商配置

```yaml
providers:
  openai:
    api_key: "${OPENAI_API_KEY}"
    base_url: "https://api.openai.com/v1"
    models:
      - name: "gpt-4"
        description: "GPT-4"
        context_window: 8192
      - name: "gpt-4-turbo"
        description: "GPT-4 Turbo"
        context_window: 128000
  
  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"
    base_url: "https://api.anthropic.com/v1"
    models:
      - name: "claude-3-opus"
        description: "Claude 3 Opus"
        context_window: 200000
  
  azure:
    api_key: "${AZURE_OPENAI_KEY}"
    base_url: "https://your-resource.openai.azure.com/openai/deployments/gpt-4"
    models:
      - name: "gpt-4"
        description: "Azure GPT-4"
        context_window: 8192
```

## 10. 故障排查

### 10.1 配置文件未加载

**症状**: 使用默认配置而非配置文件中的值

**排查**:
1. 确认配置文件路径正确
2. 确认配置文件名为 `config.yaml`
3. 检查配置文件语法是否正确

### 10.2 环境变量未替换

**症状**: 配置中 `${VAR}` 未被替换

**排查**:
1. 确认环境变量已设置
2. 检查 `.env` 文件是否加载
3. 确认环境变量名称正确

### 10.3 MCP 服务器启动失败

**症状**: MCP 服务器状态为 error

**排查**:
1. 检查 `command` 是否正确
2. 检查命令是否在 PATH 中
3. 检查 `args` 参数是否正确
4. 检查 `env` 环境变量是否设置

### 10.4 认证失败

**症状**: 返回 401 错误

**排查**:
1. 确认 `auth.enabled` 为 true
2. 确认 API Key 在 `api_keys` 列表中
3. 确认请求头格式正确: `Authorization: Bearer <key>`
