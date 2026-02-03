---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 工具系统

## 1. 概述

工具系统是 Synthia Agent 的核心组件，提供 Agent 与外部世界交互的能力。本文档说明工具架构、Tool trait、工具注册、工具执行和最佳实践。

## 2. 工具架构

### 2.1 Tool Trait

所有工具必须实现 `Tool` trait。详细的 trait 定义和方法说明请参考 [工具开发指南](../guides/tool-development.md)。

```rust
#[async_trait]
pub trait Tool: Send + Sync + Debug {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    
    async fn execute(
        &self,
        args: serde_json::Value,
        context: ToolContext,
    ) -> Result<ToolResult>;
    
    fn annotations(&self) -> Option<ToolAnnotations> {
        None
    }
}
```

### 2.2 工具注解

```rust
pub struct ToolAnnotations {
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub concurrency_safe: Option<bool>,
    pub tool_kind: Option<String>,
}
```

| 注解 | 说明 | 影响 |
|------|------|------|
| `read_only` | 工具只读取数据 | 用于上下文压缩优先级 |
| `destructive` | 工具具有破坏性 | 需要用户确认 |
| `concurrency_safe` | 工具可并发执行 | 影响并发调度 |
| `tool_kind` | 工具类型分类 | 用于分组和过滤 |

### 2.3 工具分类

| 分类 | 说明 | 示例工具 |
|------|------|----------|
| **只读工具** | 不修改文件系统 | read, grep, glob, list_directory |
| **写入工具** | 修改文件系统 | write, edit, delete, move |
| **执行工具** | 执行命令 | exec |
| **其他工具** | 辅助功能 | TodoWrite, SequentialThinking |

## 3. ToolRegistry

### 3.1 注册工具

```rust
use synthia_agent::tools::{ToolRegistry, Tool};

let registry = ToolRegistry::new();

// 注册单个工具
registry.register(Arc::new(MyTool::new())).await;

// 注册多个工具
registry.registers(tools.into_iter()).await;
```

### 3.2 工具过滤

```rust
// 配置允许/禁止的工具
let config = ToolConfig {
    allowed_tools: vec!["read".to_string(), "write".to_string()],
    denied_tools: vec!["exec".to_string()],
    max_concurrent_tools: 5,
};

registry.set_config(config).await;

// 获取过滤后的工具
let filtered_tools = registry.filtered_tools().await;
```

## 4. 内置工具

### 4.1 文件系统工具

| 工具 | 说明 | 注解 |
|------|------|------|
| `read` | 读取文件内容 | readOnly=true, destructive=false |
| `write` | 写入文件 | readOnly=false, destructive=true |
| `edit` | 编辑文件 | readOnly=false, destructive=false |
| `delete` | 删除文件或目录 | readOnly=false, destructive=true |
| `move` | 移动文件 | readOnly=false, destructive=true |
| `grep` | 搜索文件内容 | readOnly=true |
| `glob` | 查找文件 | readOnly=true |
| `list_directory` | 列出目录 | readOnly=true |
| `directory_tree` | 目录树 | readOnly=true |

### 4.2 Web 工具

| 工具 | 说明 | 注解 |
|------|------|------|
| `web_search` | Web 搜索 | readOnly=true |
| `web_fetch` | 获取网页内容 | readOnly=true |

### 4.3 辅助工具

| 工具 | 说明 | 注解 |
|------|------|------|
| `TodoWrite` | 任务列表管理 | readOnly=false |
| `SequentialThinking` | 顺序思考 | readOnly=true |
| `ContextInject` | 上下文注入 | readOnly=false |

## 5. 工具执行

### 5.1 执行流程

```
┌─────────────────────────────────────────────────────────────┐
│                      Tool Execution                          │
│                                                              │
│  1. 解析工具调用                                             │
│     ├── tool_name: 工具名称                                  │
│     └── tool_input: 工具参数                                 │
│     │                                                        │
│     ▼                                                        │
│  2. 查找工具                                                 │
│     └── 在 ToolRegistry 中查找                               │
│     │                                                        │
│     ▼                                                        │
│  3. 验证参数                                                 │
│     └── 检查参数是否符合 JSON Schema                         │
│     │                                                        │
│     ▼                                                        │
│  4. 执行工具                                                 │
│     ├── 并发执行（最多 max_concurrent_tools 个）             │
│     └── 记录执行结果                                         │
│     │                                                        │
│     ▼                                                        │
│  5. 返回结果                                                 │
│     ├── 成功：ToolResult                                     │
│     └── 失败：Error                                          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 并发执行

Agent 支持并发执行多个工具：

```rust
let tool_futures = tool_uses.into_iter().map(|tool_use| {
    async move { agent.execute_single_tool(tool_use).await }
});

let mut concurrent_stream = futures::stream::iter(tool_futures)
    .buffer_unordered(max_concurrent);
```

## 6. 自定义工具开发

详细的工具开发指南请参考 [工具开发指南](../guides/tool-development.md)，包含：

- **Tool trait 实现**：详细的 trait 方法和参数说明
- **参数验证**：JSON Schema 验证和自定义验证
- **错误处理**：错误类型和最佳实践
- **性能优化**：异步操作、并发处理、缓存策略
- **测试**：单元测试和集成测试

### 6.1 快速开始

```rust
use async_trait::async_trait;
use synthia_agent::tools::{Tool, ToolContext, ToolResult};

#[derive(Debug)]
pub struct MyCustomTool;

#[async_trait]
impl Tool for MyCustomTool {
    fn name(&self) -> &str { "my_custom_tool" }
    fn description(&self) -> &str { "A custom tool" }
    fn parameters(&self) -> serde_json::Value { json!({}) }
    
    async fn execute(
        &self,
        args: serde_json::Value,
        _context: ToolContext,
    ) -> Result<ToolResult> {
        Ok(ToolResult {
            content: vec![Content::text("Done")],
            is_error: false,
        })
    }
}
```

### 6.2 注册自定义工具

```rust
let registry = ToolRegistry::new();
registry.register(Arc::new(MyCustomTool)).await;
```

更多详细信息请查看 [工具开发指南](../guides/tool-development.md)。

## 7. 最佳实践

### 7.1 返回高信号信息

```rust
// 不好的做法：返回大量低价值信息
Ok(ToolResult {
    content: vec![Content::text(format!("{:?}", huge_data_structure))],
    is_error: false,
})

// 好的做法：返回关键信息
Ok(ToolResult {
    content: vec![Content::text(format!(
        "Found {} matches in {} files",
        match_count,
        file_count
    ))],
    is_error: false,
})
```

### 7.2 优先上下文相关性

```rust
// 不好的做法：返回技术标识符
Ok(ToolResult {
    content: vec![Content::text(format!("uuid: {}", uuid))],
    is_error: false,
})

// 好的做法：返回可理解的描述
Ok(ToolResult {
    content: vec![Content::text(format!(
        "Created file: {}",
        file_path
    ))],
    is_error: false,
})
```

### 7.3 错误处理

```rust
async fn execute(&self, args: Value, _context: ToolContext) -> Result<ToolResult> {
    let input: MyToolInput = serde_json::from_value(args)
        .map_err(|e| AgentError::invalid_input(format!("Invalid parameters: {}", e)))?;
    
    // 执行操作
    match perform_operation(&input).await {
        Ok(result) => Ok(ToolResult {
            content: vec![Content::text(result)],
            is_error: false,
        }),
        Err(e) => Ok(ToolResult {
            content: vec![Content::text(format!("Error: {}", e))],
            is_error: true,
        }),
    }
}
```

## 8. 相关文档

- [Agent执行流程](agent-execution.md)
- [工具开发指南](../guides/tool-development.md)

## 9. 参考资料

- [Anthropic Tool Design](https://www.anthropic.com/engineering/writing-tools-for-agents)
- [MCP Tool Specification](https://modelcontextprotocol.io/specification)
