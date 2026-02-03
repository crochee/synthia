---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# Synthia Server 架构文档

## 1. 概述

Synthia Server 是 Synthia Agent 的 HTTP API 服务器实现，为前端应用、编辑器插件和 TUI 客户端提供 RESTful API 接口。本文档描述了服务器的整体架构设计、组件交互和关键实现细节。

## 2. 系统架构

### 2.1 分层架构

系统采用清晰的分层架构，从上到下分为五层：

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         HTTP/WebSocket Layer                             │
│                                                                          │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐       │
│  │  handlers   │ │chat_handlers│ │session_hdlrs│ │  ws_handlers│       │
│  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘       │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐                        │
│  │skill_hdlrs  │ │ mcp_handlers│ │model_handlers│                       │
│  └─────────────┘ └─────────────┘ └─────────────┘                        │
├─────────────────────────────────────────────────────────────────────────┤
│                           Service Layer                                  │
│                                                                          │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐       │
│  │ ChatService │ │SessionServie│ │ ToolService │ │ SkillService│       │
│  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘       │
│  ┌─────────────┐ ┌─────────────┐                                        │
│  │  McpService │ │ ModelService│                                        │
│  └─────────────┘ └─────────────┘                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                           Domain Layer                                   │
│                                                                          │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐       │
│  │     mcp     │ │   session   │ │    skill    │ │    tool     │       │
│  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘       │
│  ┌─────────────┐                                                         │
│  │   models    │                                                         │
│  └─────────────┘                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                       Infrastructure Layer                               │
│                                                                          │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐       │
│  │    state    │ │   config    │ │    auth     │ │ validation  │       │
│  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘       │
├─────────────────────────────────────────────────────────────────────────┤
│                           Agent Core                                     │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                        synthia-agent crate                        │   │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐   │   │
│  │  │  Agent  │ │ Storage │ │ Context │ │  Tools  │ │  Hooks  │   │   │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘   │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 各层职责

| 层级 | 职责 | 关键文件 |
|------|------|----------|
| HTTP/WebSocket Layer | 处理HTTP请求/响应、WebSocket连接、路由分发 | `*_handlers.rs`, `lib.rs` |
| Service Layer | 业务逻辑处理、数据转换、错误处理 | `service/*.rs` |
| Domain Layer | 领域模型、业务规则、状态管理 | `mcp.rs`, `session.rs`, `skill.rs`, `tool.rs`, `models.rs` |
| Infrastructure Layer | 配置管理、认证授权、状态共享、验证 | `state.rs`, `config.rs`, `auth.rs`, `validation.rs` |
| Agent Core | 核心Agent逻辑、LLM交互、工具执行 | `synthia-agent` crate |

## 3. 核心组件

### 3.1 AppState

AppState 是整个应用的核心状态容器，通过 `Arc<AppState>` 在所有请求处理器之间共享。

**代码位置**: `crates/synthia-server/src/state.rs`

```rust
pub struct AppState {
    pub agent: Agent,                      // 核心Agent实例
    pub storage: Arc<UnifiedStorage>,      // 统一存储（SQLite）
    pub tool_registry: Arc<ToolRegistry>,  // 工具注册表
    pub current_dir: PathBuf,              // 工作目录
    pub mcp_module: McpModule,             // MCP服务器管理模块
    pub config: Arc<RwLock<ServerConfig>>, // 运行时配置（支持热更新）
    pub config_path: PathBuf,              // 配置文件路径
    pub config_host: String,               // 监听地址
    pub config_port: u16,                  // 监听端口
}
```

**设计要点**：
- 使用 `Arc` 实现线程安全的共享
- `config` 使用 `RwLock` 支持运行时配置更新
- `Clone` trait 实现使得每个请求可以独立持有状态引用

### 3.2 ServerConfig

服务器配置结构，支持 YAML 和 JSON 格式。

**代码位置**: `crates/synthia-server/src/config.rs`

```rust
pub struct ServerConfig {
    pub version: String,
    pub host: String,
    pub port: u16,
    pub model_override: Option<String>,
    pub max_agents: usize,
    pub providers: HashMap<String, ProviderConfig>,
    pub mcps: HashMap<String, McpConfig>,
    pub agents: HashMap<String, AgentConfig>,
    pub skills: Vec<SkillConfig>,
    pub auth: AuthConfig,
    pub rate_limit: RateLimitConfig,
}
```

**配置加载流程**：
1. 启动时从 `config.yaml` 加载
2. CLI 参数可覆盖配置文件中的 host/port
3. 运行时通过 API 修改配置（存储在内存中）

### 3.3 ServerError

统一的错误类型，实现了 `IntoResponse` trait，自动转换为 HTTP 响应。

**代码位置**: `crates/synthia-server/src/error.rs`

```rust
pub enum ServerError {
    Internal(String),
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    Conflict(String),
    TooManyRequests(String),
    ServiceUnavailable(String),
    AgentError(String),
    McpError(String),
    ToolError(String),
    SessionError(String),
    ConfigError(String),
}
```

**错误响应格式**：
```json
{
    "error": {
        "type": "not_found",
        "message": "Session 'abc123' not found"
    }
}
```

## 4. 服务层设计

### 4.1 服务模式

所有服务遵循统一的设计模式：

```rust
pub struct XxxService {
    state: Arc<AppState>,  // 或特定依赖
}

impl XxxService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn operation(&self, ...) -> Result<T, ServerError> {
        // 业务逻辑
    }
}
```

### 4.2 服务列表

| 服务 | 职责 | 依赖 |
|------|------|------|
| ChatService | 聊天逻辑、会话创建 | AppState |
| SessionService | 会话CRUD、上下文压缩 | AppState |
| ToolService | 工具列表、执行 | ToolRegistry |
| SkillService | 技能列表、加载、执行 | SkillTool |
| McpService | MCP服务器管理 | McpModule |
| ModelService | 模型提供商管理 | AppState |

### 4.3 服务交互示例

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ HTTP Handler │────▶│   Service    │────▶│  Agent Core  │
└──────────────┘     └──────────────┘     └──────────────┘
       │                    │                    │
       │                    │                    │
       ▼                    ▼                    ▼
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Extract     │     │  Transform   │     │   Execute    │
│  Parameters  │     │  Data        │     │   Business   │
│              │     │              │     │   Logic      │
└──────────────┘     └──────────────┘     └──────────────┘
       │                    │                    │
       └────────────────────┴────────────────────┘
                            │
                            ▼
                     ┌──────────────┐
                     │   Response   │
                     └──────────────┘
```

## 5. 路由设计

### 5.1 路由结构

```rust
let app = Router::new()
    // 健康检查
    .route("/health", get(handlers::health))
    
    // 工具管理
    .route("/tools", get(handlers::list_tools))
    .route("/tools/{name}", get(handlers::get_tool))
    .route("/tools/{name}/execute", post(handlers::execute_tool))
    
    // 聊天接口
    .route("/chat", post(chat_handlers::chat))
    .route("/chat/stream", post(chat_handlers::chat_stream))
    
    // 会话管理
    .route("/sessions", get(session_handlers::list_sessions))
    .route("/sessions", post(session_handlers::create_session))
    .route("/sessions/{id}", get(session_handlers::get_session))
    .route("/sessions/{id}", delete(session_handlers::delete_session))
    .route("/sessions/{id}/compact", post(session_handlers::compact_session))
    .route("/sessions/{id}/messages", get(session_handlers::get_session_messages))
    
    // 技能管理
    .route("/skills", get(skill_handlers::list_skills))
    .route("/skills", post(skill_handlers::add_skill))
    .route("/skills/{name}", get(skill_handlers::get_skill))
    .route("/skills/{name}", delete(skill_handlers::delete_skill))
    .route("/skills/{name}/load", post(skill_handlers::load_skill))
    
    // MCP服务器管理
    .route("/mcp/servers", get(mcp_handlers::list_mcp_servers))
    .route("/mcp/servers", post(mcp_handlers::register_mcp_server))
    .route("/mcp/servers/{name}", delete(mcp_handlers::unregister_mcp_server))
    .route("/mcp/servers/{name}/tools", get(mcp_handlers::list_mcp_tools))
    
    // 模型提供商管理
    .route("/models", get(model_handlers::list_models))
    .route("/models", post(model_handlers::add_model_provider))
    .route("/models/{provider}", delete(model_handlers::delete_model))
    .route("/models/{provider}/{name}", get(model_handlers::get_model))
    .route("/models/{provider}/{name}", put(model_handlers::update_model))
    
    // WebSocket
    .route("/ws/{session_id}", get(ws_handlers::websocket))
    
    .with_state(state.clone())
    .layer(cors);
```

### 5.2 路由分组

| 前缀 | 功能模块 | 处理器文件 |
|------|----------|------------|
| `/health` | 健康检查 | `handlers.rs` |
| `/tools` | 工具管理 | `handlers.rs` |
| `/chat` | 聊天接口 | `chat_handlers.rs` |
| `/sessions` | 会话管理 | `session_handlers.rs` |
| `/skills` | 技能管理 | `skill_handlers.rs` |
| `/mcp` | MCP服务器 | `mcp_handlers.rs` |
| `/models` | 模型提供商 | `model_handlers.rs` |
| `/ws` | WebSocket | `ws_handlers.rs` |

## 6. 数据流

### 6.1 同步聊天流程

```
┌─────────┐     ┌──────────────┐     ┌─────────────┐     ┌─────────┐
│ Client  │────▶│ chat_handler │────▶│ ChatService │────▶│  Agent  │
└─────────┘     └──────────────┘     └─────────────┘     └─────────┘
     │                 │                    │                  │
     │  POST /chat     │                    │                  │
     │  {message, ...} │                    │                  │
     │                 │                    │                  │
     │                 │ get_or_create_     │                  │
     │                 │ session()          │                  │
     │                 │───────────────────▶│                  │
     │                 │                    │                  │
     │                 │                    │    reply()       │
     │                 │                    │─────────────────▶│
     │                 │                    │                  │
     │                 │                    │    AgentEvent    │
     │                 │                    │◀─────────────────│
     │                 │                    │    Stream        │
     │                 │                    │                  │
     │                 │    collect events  │                  │
     │                 │◀───────────────────│                  │
     │                 │                    │                  │
     │  ChatResponse   │                    │                  │
     │◀────────────────│                    │                  │
     │                 │                    │                  │
```

### 6.2 流式聊天流程

```
┌─────────┐     ┌──────────────┐     ┌─────────────┐     ┌─────────┐
│ Client  │────▶│ chat_stream  │────▶│ ChatService │────▶│  Agent  │
└─────────┘     └──────────────┘     └─────────────┘     └─────────┘
     │                 │                    │                  │
     │  POST /chat/    │                    │                  │
     │  stream         │                    │                  │
     │                 │                    │                  │
     │                 │                    │    reply()       │
     │                 │                    │─────────────────▶│
     │                 │                    │                  │
     │  SSE Event:     │                    │    AgentEvent    │
     │  message        │◀───────────────────│◀─────────────────│
     │                 │                    │                  │
     │  SSE Event:     │                    │    AgentEvent    │
     │  message        │◀───────────────────│◀─────────────────│
     │                 │                    │                  │
     │  SSE Event:     │                    │    Status:       │
     │  status         │◀───────────────────│◀─────────────────│
     │  (Completed)    │                    │    Completed     │
     │                 │                    │                  │
```

### 6.3 WebSocket 通信流程

```
┌─────────┐                              ┌──────────────┐
│ Client  │◀──────────WebSocket─────────▶│ ws_handler   │
└─────────┘                              └──────────────┘
     │                                          │
     │  {"action":"chat","content":"..."}       │
     │─────────────────────────────────────────▶│
     │                                          │
     │                                          │ Agent::reply()
     │                                          │
     │  {"type":"message","content":"..."}      │
     │◀─────────────────────────────────────────│
     │                                          │
     │  {"type":"status","status":"Completed"}  │
     │◀─────────────────────────────────────────│
     │                                          │
     │  {"action":"cancel"}                     │
     │─────────────────────────────────────────▶│
     │                                          │
     │                                          │ CancellationToken.cancel()
     │                                          │
```

## 7. MCP 模块

### 7.1 MCP 架构

```
┌─────────────────────────────────────────────────────────────┐
│                        McpModule                             │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              DashMap<String, McpServer>              │    │
│  │                                                      │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │    │
│  │  │ McpServer 1 │  │ McpServer 2 │  │ McpServer N │  │    │
│  │  │ (filesystem)│  │  (github)   │  │  (custom)   │  │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 7.2 MCP Server 生命周期

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

### 7.3 MCP Server 配置

```rust
pub struct McpServerConfig {
    pub name: String,
    pub server_type: String,      // stdio, sse, http
    pub description: Option<String>,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub timeout: u64,
    pub enabled: bool,
}
```

## 8. 会话管理

### 8.1 会话生命周期

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Create    │────▶│    Active   │────▶│   Compact   │
└─────────────┘     └─────────────┘     └─────────────┘
                          │                    │
                          │                    │
                          ▼                    ▼
                    ┌─────────────┐     ┌─────────────┐
                    │   Delete    │◀────│   Active    │
                    └─────────────┘     └─────────────┘
```

### 8.2 会话上下文压缩

当会话上下文接近 token 限制时，可以触发压缩操作：

```rust
pub struct CompactionResult {
    pub before_count: usize,        // 压缩前消息数
    pub after_count: usize,         // 压缩后消息数
    pub strategy: String,           // 使用的策略
    pub token_ratio_before: f64,    // 压缩前token比例
    pub token_ratio_after: f64,     // 压缩后token比例
}
```

## 9. 工具系统

### 9.1 工具注册流程

```
┌─────────────────────────────────────────────────────────────┐
│                       setup.rs                               │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │                   register_tools()                    │    │
│  │                                                      │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │    │
│  │  │ builtin     │  │ cron        │  │ task        │  │    │
│  │  │ tools       │  │ tools       │  │ tools       │  │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │    │
│  │                                                      │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │    │
│  │  │ team        │  │ worktree    │  │ background  │  │    │
│  │  │ tools       │  │ tools       │  │ tools       │  │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │    │
│  │                                                      │    │
│  │  ┌─────────────┐  ┌─────────────┐                   │    │
│  │  │ ExecTool    │  │ Subagent    │                   │    │
│  │  │             │  │ Tool        │                   │    │
│  │  └─────────────┘  └─────────────┘                   │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 9.2 工具信息结构

```rust
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: Value,           // JSON Schema
    pub annotations: Option<ToolAnnotations>,
}

pub struct ToolAnnotations {
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub concurrency_safe: Option<bool>,
    pub tool_kind: Option<String>,
}
```

## 10. 认证与授权

### 10.1 认证中间件

```rust
pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ServerError> {
    let config = state.config.read().await;

    // 认证未启用，直接放行
    if !config.auth.enabled || config.auth.api_keys.is_empty() {
        return Ok(next.run(request).await);
    }

    // 验证 Bearer Token
    let auth_header = request.headers().get(AUTHORIZATION);
    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if config.auth.api_keys.iter().any(|k| k == token) {
                Ok(next.run(request).await)
            } else {
                Err(ServerError::Unauthorized("Invalid API key".into()))
            }
        }
        _ => Err(ServerError::Unauthorized(
            "Missing or invalid Authorization header".into()
        ))
    }
}
```

### 10.2 配置示例

```yaml
auth:
  enabled: true
  api_keys:
    - "sk-server-xxx"
    - "sk-server-yyy"
```

## 11. 启动流程

```
┌─────────────────────────────────────────────────────────────┐
│                        main.rs                               │
│                                                              │
│  1. 解析命令行参数                                           │
│     │                                                        │
│     ▼                                                        │
│  2. 加载配置文件 (config.yaml)                               │
│     │                                                        │
│     ▼                                                        │
│  3. 构建Agent (build_agent)                                  │
│     │                                                        │
│     ├── 初始化存储 (UnifiedStorage)                          │
│     ├── 初始化工具注册表 (ToolRegistry)                      │
│     ├── 初始化上下文管理器 (ContextManager)                  │
│     ├── 注册内置工具                                         │
│     ├── 创建子Agent工具                                      │
│     └── 注册MCP服务器                                        │
│     │                                                        │
│     ▼                                                        │
│  4. 创建取消令牌 (CancellationToken)                         │
│     │                                                        │
│     ▼                                                        │
│  5. 启动信号处理任务 (Ctrl+C)                                │
│     │                                                        │
│     ▼                                                        │
│  6. 启动HTTP服务器 (run_server)                              │
│     │                                                        │
│     ▼                                                        │
│  7. 等待优雅关闭                                             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## 12. 部署架构

### 12.1 单机部署

```
┌─────────────────────────────────────────────────────────────┐
│                        Host Machine                          │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │                  Synthia Server                       │    │
│  │                                                       │    │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐             │    │
│  │  │ HTTP    │  │ WebSocket│  │  SSE    │             │    │
│  │  │ :8080   │  │ :8080    │  │ :8080   │             │    │
│  │  └─────────┘  └─────────┘  └─────────┘             │    │
│  │                                                       │    │
│  │  ┌─────────────────────────────────────────────┐    │    │
│  │  │              Agent Core                      │    │    │
│  │  └─────────────────────────────────────────────┘    │    │
│  │                                                       │    │
│  │  ┌─────────────────────────────────────────────┐    │    │
│  │  │           SQLite (.agents/synthia.db)        │    │    │
│  │  └─────────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │                  MCP Servers                         │    │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐             │    │
│  │  │filesystem│  │ github  │  │ custom  │             │    │
│  │  └─────────┘  └─────────┘  └─────────┘             │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 12.2 配置文件位置

```
project/
├── config.yaml           # 服务器配置
├── .agents/
│   └── synthia.db        # SQLite数据库
└── .trae/
    └── skills/           # 技能文件
        └── *.md
```

## 13. 扩展点

### 13.1 添加新的API端点

1. 在 `service/` 下创建或修改服务
2. 在 `*_handlers.rs` 下创建处理器
3. 在 `lib.rs` 中注册路由

### 13.2 添加新的工具

1. 实现 `synthia_agent::tools::Tool` trait
2. 在 `setup.rs` 的 `register_tools()` 中注册

### 13.3 添加新的MCP服务器类型

1. 在 `mcp.rs` 中扩展 `McpServer` 实现
2. 更新 `McpServerConfig` 的验证逻辑

## 14. 性能考虑

### 14.1 并发处理

- 使用 Tokio 异步运行时
- 每个请求独立处理，无阻塞
- `DashMap` 用于 MCP 服务器的并发安全访问

### 14.2 内存管理

- `Arc` 共享状态，避免复制
- 流式响应避免完整缓冲
- 会话压缩减少内存占用

### 14.3 连接管理

- WebSocket 连接独立处理
- MCP 服务器进程池管理
- 数据库连接池（通过 `UnifiedStorage`）

## 15. 安全考虑

### 15.1 输入验证

- `validation.rs` 提供验证工具
- 路径验证防止目录遍历
- 参数类型验证

### 15.2 认证授权

- 可选的 API Key 认证
- Bearer Token 格式
- 中间件实现，易于扩展

### 15.3 错误处理

- 不暴露内部实现细节
- 统一的错误响应格式
- 日志记录敏感操作
