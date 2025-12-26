# DM-Code-Agent Rust 重构计划

## 1. 项目概述
将现有的 Python 实现的 DM-Code-Agent 重构为 Rust 语言，只保留 OpenAI 客户端，移除其他 LLM 客户端支持，并**仅支持流式输出**。

## 2. 重构目标
- 保持原有核心功能不变
- 使用 Rust 现代库和最佳实践
- 只保留 OpenAI 客户端
- **仅支持流式输出**，移除所有非流式实现
- **使用 rmcp 库实现 MCP 和工具**
- 提高性能和安全性
- 保持 API 设计一致性

## 3. 项目结构设计
```
synthia-agent/
├── src/
│   ├── core/
│   │   ├── agent.rs       # ReActAgent 实现（仅流式）
│   │   ├── step.rs        # Step 数据结构
│   │   └── mod.rs
│   ├── clients/
│   │   ├── mod.rs         # LLMClient trait 定义和实现
│   │   └── openai.rs      # OpenAI 客户端实现（仅流式）
│   ├── tools/             # 使用 rmcp 库实现工具
│   │   ├── base.rs        # Tool trait（基于 rmcp）
│   │   ├── code_analysis.rs
│   │   ├── execution.rs
│   │   ├── file.rs
│   │   └── mod.rs
│   ├── prompts/
│   │   ├── code_agent.rs  # 代码智能体提示词构建
│   │   ├── system.rs      # 系统提示词
│   │   └── mod.rs
│   ├── memory/
│   │   ├── context_compressor.rs
│   │   └── mod.rs
│   ├── mcp/               # 使用 rmcp 库实现
│   │   ├── client.rs      # rmcp 客户端封装
│   │   ├── config.rs      # MCP 配置
│   │   ├── manager.rs     # MCP 工具管理器（基于 rmcp）
│   │   └── mod.rs
│   ├── lib.rs             # 库导出
│   └── main.rs            # CLI 入口（仅流式输出）
├── Cargo.toml
└── README.md
```

## 4. 模块和实现定义（mod.rs）

### 4.1 clients/mod.rs - LLMClient Trait 定义
```rust
/// LLM 客户端 trait，定义流式交互接口
#[async_trait::async_trait]
pub trait LLMClient: Send + Sync {
    /// 流式完成请求，返回 LLM 响应的字节流
    async fn stream_complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<impl Stream<Item = Result<StreamChunk, LLMError>> + Send, LLMError>;
    
    /// 获取模型信息
    fn model_info(&self) -> ModelInfo;
}

/// 消息结构
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// 流式响应块
#[derive(Debug, Clone, PartialEq)]
pub struct StreamChunk {
    pub content: String,
    pub chunk_type: ChunkType,
    pub delta: bool,
}

/// OpenAI 客户端实现（仅流式）
pub struct OpenAIClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

#[async_trait::async_trait]
impl LLMClient for OpenAIClient {
    async fn stream_complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<impl Stream<Item = Result<StreamChunk, LLMError>> + Send, LLMError> {
        // 实现 SSE 流式解析
    }
    
    fn model_info(&self) -> ModelInfo {
        ModelInfo { name: self.model.clone() }
    }
}
```

### 4.2 tools/mod.rs - Tool Trait 定义（基于 rmcp）
```rust
/// 工具 trait，继承自 rmcp 的 Tool 特性
pub trait Tool: rmcp::Tool + Send + Sync {
    /// 工具元信息
    fn info(&self) -> ToolInfo;
    
    /// 执行工具逻辑
    async fn execute(&self, arguments: Value) -> Result<Value, ToolError>;
}

/// 工具定义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Schema,
}

/// 工具调用结果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_name: String,
    pub arguments: Value,
    pub result: Result<Value, ToolError>,
    pub is_error: bool,
}

/// 使用 rmcp 实现的具体工具
pub struct FileReadTool {
    base_path: PathBuf,
}

impl rmcp::Tool for FileReadTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read file contents" }
    fn input_schema(&self) -> Schema { /* ... */ }
}

#[async_trait::async_trait]
impl Tool for FileReadTool {
    fn info(&self) -> ToolInfo { /* ... */ }
    async fn execute(&self, arguments: Value) -> Result<Value, ToolError> { /* ... */ }
}
```

### 4.3 mcp/mod.rs - MCP 模块（基于 rmcp）
```rust
/// MCP 工具管理器（使用 rmcp 库）
pub struct MCPManager {
    clients: HashMap<String, rmcp::Client>,
    tools: HashMap<String, Box<dyn Tool>>,
    config: MCPConfig,
}

/// MCP 客户端（基于 rmcp）
pub struct MCPClient {
    name: String,
    client: rmcp::Client,
    server_params: ServerParameters,
}

/// MCP 配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MCPConfig {
    pub servers: HashMap<String, MCPServerConfig>,
}

impl MCPManager {
    /// 连接 MCP 服务器并注册工具
    pub async fn connect_server(
        &mut self,
        name: String,
        config: MCPServerConfig,
    ) -> Result<(), MCPError> {
        let client = rmcp::Client::new(config.command, config.args, config.env);
        client.connect().await?;
        
        // 注册服务器工具
        let tools = client.list_tools().await?;
        for tool in tools {
            self.register_tool(tool.name, Box::new(tool));
        }
        
        Ok(())
    }
    
    /// 调用 MCP 工具
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, MCPError> {
        let client = self.clients.get(server_name)
            .ok_or(MCPError::ServerNotFound)?;
        client.call_tool(tool_name, arguments).await
    }
}
```

## 5. 核心实现步骤

### 5.1 初始化 Rust 项目
- 设置 Cargo.toml，添加必要依赖
- 配置 Rust 工具链和 linter
- 添加 rmcp 相关依赖
- 添加流式相关依赖（futures, tokio-stream）

### 5.2 实现 LLMClient Trait
- 定义 `LLMClient` trait，**仅包含流式方法**
- 实现 `Message`、`StreamChunk` 等数据结构
- 设计流式响应数据结构
- 实现 OpenAI 流式客户端（SSE 解析）

### 5.3 实现 OpenAI 客户端（仅流式）
- 使用 `reqwest` 库调用 OpenAI Chat Completions Streaming API
- **仅实现流式请求模式**，支持 SSE（Server-Sent Events）
- 移除所有非流式实现
- 实现消息转换和响应处理
- 处理错误和超时

### 5.4 实现智能体核心逻辑
- 重构 `ReactAgent` 类
- 实现推理-行动循环
- **仅支持流式输出**，实时返回智能体思考和行动
- 移除所有非流式方法
- 集成 rmcp 工具调用机制

### 5.5 实现工具系统（基于 rmcp）
- 使用 rmcp 库实现所有工具
- 实现文件操作工具（read_file, write_file, list_dir）
- 实现代码分析工具（grep, parse_code）
- 实现执行工具（run_command）
- 工具调用通过 rmcp 协议进行

### 5.6 实现 MCP 模块（基于 rmcp）
- 使用 rmcp 库连接 MCP 服务器
- 实现 MCP 客户端封装
- 实现 MCP 工具自动注册和调用
- 管理多个 MCP 服务器连接

### 5.7 实现提示词系统
- 重构提示词构建逻辑
- 支持动态工具注入
- 适配流式输出格式

### 5.8 实现上下文压缩
- 实现上下文压缩算法
- 优化长对话处理

### 5.9 实现命令行接口
- 重构 CLI 入口
- **仅支持流式输出**，实时显示智能体响应
- 支持配置文件和环境变量

## 6. 技术栈选择
- **HTTP 客户端**: `reqwest`
- **JSON 处理**: `serde` + `serde_json`
- **命令行解析**: `clap`
- **异步运行时**: `tokio`
- **流式处理**: `futures` + `tokio-stream`
- **MCP 协议**: `rmcp` 库
- **环境变量**: `dotenvy`
- **日志**: `tracing`

## 7. LLMClient Trait 设计

### 7.1 Trait 定义
```rust
#[async_trait::async_trait]
pub trait LLMClient: Send + Sync {
    /// 流式完成请求
    async fn stream_complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk, LLMError>> + Send + Unpin>, LLMError>;
    
    /// 获取模型信息
    fn model_info(&self) -> ModelInfo;
}

/// 消息角色
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// 流式响应块类型
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkType {
    Content,      // 文本内容
    ToolCall,     // 工具调用开始
    ToolArgs,     // 工具参数
    Done,         // 完成信号
    Error,        // 错误信息
}
```

### 7.2 OpenAI 客户端实现
```rust
pub struct OpenAIClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(600),
        }
    }
}

#[async_trait::async_trait]
impl LLMClient for OpenAIClient {
    async fn stream_complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk, LLMError>> + Send + Unpin>, LLMError> {
        let request = self.build_request(messages, tools)?;
        
        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| LLMError::RequestFailed(e.to_string()))?;
            
        Ok(Box::new(self.parse_stream(response).await))
    }
    
    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            name: self.model.clone(),
            max_tokens: Some(16384),
            supports_streaming: true,
        }
    }
}
```

## 8. rmcp 集成设计

### 8.1 MCP 工具注册
```rust
impl MCPManager {
    /// 从 rmcp 客户端注册工具
    fn register_rmcp_tools(&mut self, client: rmcp::Client) {
        let tools = client.list_tools();
        for tool in tools {
            let name = tool.name.clone();
            let tool: Box<dyn Tool> = Box::new(tool);
            self.tools.insert(name, tool);
        }
        self.clients.insert(client.id(), client);
    }
}
```

### 8.2 工具调用流程
1. 智能体输出工具调用
2. 解析工具名称和参数
3. 通过 rmcp 调用对应工具
4. 实时返回工具执行结果
5. 将结果注入对话上下文

## 9. 流式输出实现细节

### 9.1 客户端层
- `LLMClient` trait 仅包含 `stream_complete` 方法
- 返回 `impl Stream<Item = Result<StreamChunk, LLMError>>`
- 仅实现 OpenAI SSE 解析
- 移除所有非流式方法

### 9.2 智能体层
- `ReactAgent` 仅提供 `stream_run` 方法
- 实时处理流式响应，分块解析思考、行动和观察
- 支持中途中断和取消
- 移除 `run` 等非流式方法

### 9.3 CLI 层
- 使用 `tokio-stream` 处理流式输出
- 实时显示智能体思考和行动
- 支持彩色输出和格式化
- 仅支持流式交互模式

## 10. 迁移策略
- 提供详细的迁移文档，说明从非流式到仅流式的变化
- 支持原有配置文件格式
- 提供完整的流式 API 文档和示例
- 保持工具调用接口与 rmcp 库兼容

## 11. 测试计划
- 单元测试：测试核心功能和流式输出
- 集成测试：测试端到端流式流程
- MCP 集成测试：测试 rmcp 工具调用
- 性能测试：比较 Rust 实现与 Python 实现的性能差异
- 流式测试：验证流式输出的实时性和完整性
- 压力测试：测试高并发流式请求处理

## 12. 重构后优势
- 提高性能和响应速度
- 增强内存安全性
- 减少依赖和部署复杂性
- 更好的并发处理能力
- 更严格的类型检查
- **统一的流式输出体验**，简化 API 设计
- **使用 rmcp 库**，实现标准的 MCP 协议支持
- 降低维护成本，减少代码复杂度

## 13. 风险评估
- Rust 学习曲线：需要熟悉 Rust 语法和生态
- API 变更：移除非流式方法可能影响现有代码
- rmcp 库依赖：需要熟悉 rmcp 库的 API
- 流式实现复杂度：需要处理异步流和 SSE 解析
- 调试难度：流式应用的调试比非流式更复杂

## 14. 交付物
- 完整的 Rust 实现（仅流式输出，基于 rmcp）
- 全面的测试套件
- 详细的迁移文档
- 完整的示例代码
- API 文档

## 15. 后续优化方向
- 优化流式响应解析性能
- 实现更高效的上下文管理
- 探索更多 rmcp 生态工具
- 实现插件系统
- 增强安全性和监控