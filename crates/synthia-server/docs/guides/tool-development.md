---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 工具开发指南

## 1. 概述

本指南详细说明如何为 Synthia Agent 开发自定义工具，包括 Tool trait 实现、参数验证、错误处理和最佳实践。

## 2. Tool Trait

### 2.1 Trait 定义

所有工具必须实现 `Tool` trait：

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

### 2.2 方法说明

| 方法 | 必需 | 说明 |
|------|------|------|
| `name()` | 是 | 工具名称，用于调用 |
| `description()` | 是 | 工具描述，LLM 用于理解工具用途 |
| `parameters()` | 是 | JSON Schema 格式的参数定义 |
| `execute()` | 是 | 工具执行逻辑 |
| `annotations()` | 否 | 工具注解，说明工具特性 |

## 3. 工具开发步骤

### 3.1 定义输入结构

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct MyToolInput {
    pub file_path: String,
    pub pattern: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    10
}
```

### 3.2 实现工具

```rust
use async_trait::async_trait;
use synthia_agent::{
    tools::{Tool, ToolContext, ToolResult, ToolAnnotations},
    types::{Content, AgentError},
    Result,
};

#[derive(Debug)]
pub struct MyCustomTool {
    workspace: PathBuf,
}

impl MyCustomTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for MyCustomTool {
    fn name(&self) -> &str {
        "my_custom_tool"
    }
    
    fn description(&self) -> &str {
        "Search for patterns in files within the workspace"
    }
    
    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Relative path to the file"
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (regex supported)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return",
                    "default": 10
                }
            },
            "required": ["file_path", "pattern"]
        })
    }
    
    async fn execute(
        &self,
        args: serde_json::Value,
        context: ToolContext,
    ) -> Result<ToolResult> {
        let input: MyToolInput = serde_json::from_value(args)
            .map_err(|e| AgentError::invalid_input(format!("Invalid parameters: {}", e)))?;
        
        let full_path = self.workspace.join(&input.file_path);
        
        if !full_path.exists() {
            return Ok(ToolResult {
                content: vec![Content::text(format!(
                    "File not found: {}",
                    input.file_path
                ))],
                is_error: true,
            });
        }
        
        let content = tokio::fs::read_to_string(&full_path).await
            .map_err(|e| AgentError::io(format!("Failed to read file: {}", e)))?;
        
        let pattern = regex::Regex::new(&input.pattern)
            .map_err(|e| AgentError::invalid_input(format!("Invalid pattern: {}", e)))?;
        
        let matches: Vec<_> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| pattern.is_match(line))
            .take(input.max_results)
            .map(|(i, line)| format!("{}: {}", i + 1, line))
            .collect();
        
        if matches.is_empty() {
            Ok(ToolResult {
                content: vec![Content::text("No matches found")],
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                content: vec![Content::text(matches.join("\n"))],
                is_error: false,
            })
        }
    }
    
    fn annotations(&self) -> Option<ToolAnnotations> {
        Some(ToolAnnotations {
            read_only: Some(true),
            destructive: Some(false),
            concurrency_safe: Some(true),
            tool_kind: Some("FileOperation".to_string()),
        })
    }
}
```

### 3.3 注册工具

```rust
use std::sync::Arc;
use synthia_agent::tools::ToolRegistry;

let registry = ToolRegistry::new();
registry.register(Arc::new(MyCustomTool::new(workspace))).await;
```

## 4. 工具注解

### 4.1 注解类型

```rust
pub struct ToolAnnotations {
    pub read_only: Option<bool>,           // 是否只读
    pub destructive: Option<bool>,         // 是否具有破坏性
    pub concurrency_safe: Option<bool>,    // 是否并发安全
    pub tool_kind: Option<String>,         // 工具类型
}
```

### 4.2 注解说明

| 注解 | 说明 | 影响 |
|------|------|------|
| `read_only` | 工具只读取数据，不修改 | 用于上下文压缩优先级 |
| `destructive` | 工具具有破坏性（如删除） | 需要用户确认 |
| `concurrency_safe` | 工具可以并发执行 | 影响并发调度 |
| `tool_kind` | 工具类型分类 | 用于分组和过滤 |

### 4.3 注解示例

```rust
// 只读工具
fn annotations(&self) -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        read_only: Some(true),
        destructive: Some(false),
        concurrency_safe: Some(true),
        tool_kind: Some("FileOperation".to_string()),
    })
}

// 破坏性工具
fn annotations(&self) -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        read_only: Some(false),
        destructive: Some(true),
        concurrency_safe: Some(false),
        tool_kind: Some("FileOperation".to_string()),
    })
}
```

## 5. 参数验证

### 5.1 JSON Schema 验证

```rust
fn parameters(&self) -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "File path (must be absolute)"
            },
            "mode": {
                "type": "string",
                "enum": ["read", "write", "append"],
                "default": "read"
            },
            "encoding": {
                "type": "string",
                "enum": ["utf-8", "utf-16", "ascii"],
                "default": "utf-8"
            }
        },
        "required": ["path"]
    })
}
```

### 5.2 自定义验证

```rust
async fn execute(&self, args: Value, context: ToolContext) -> Result<ToolResult> {
    let input: MyToolInput = serde_json::from_value(args)?;
    
    // 路径验证
    if !input.path.starts_with('/') {
        return Err(AgentError::invalid_input(
            "Path must be absolute"
        ));
    }
    
    // 路径遍历检查
    if input.path.contains("..") {
        return Err(AgentError::invalid_input(
            "Path traversal not allowed"
        ));
    }
    
    // 范围验证
    if input.max_results > 1000 {
        return Err(AgentError::invalid_input(
            "max_results cannot exceed 1000"
        ));
    }
    
    // 执行工具逻辑
    // ...
}
```

## 6. 错误处理

### 6.1 错误类型

```rust
pub enum AgentError {
    InvalidInput(String),
    Io(String),
    Timeout(String),
    NotFound(String),
    PermissionDenied(String),
    Internal(String),
}
```

### 6.2 错误处理最佳实践

```rust
async fn execute(&self, args: Value, context: ToolContext) -> Result<ToolResult> {
    let input: MyToolInput = serde_json::from_value(args)
        .map_err(|e| AgentError::invalid_input(format!("Invalid parameters: {}", e)))?;
    
    // 1. 前置条件检查
    let full_path = self.workspace.join(&input.path);
    if !full_path.exists() {
        return Ok(ToolResult {
            content: vec![Content::text(format!(
                "File not found: {}",
                input.path
            ))],
            is_error: true,
        });
    }
    
    // 2. 权限检查
    if !has_permission(&full_path) {
        return Ok(ToolResult {
            content: vec![Content::text(format!(
                "Permission denied: {}",
                input.path
            ))],
            is_error: true,
        });
    }
    
    // 3. 执行操作（带超时）
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        perform_operation(&full_path, &input)
    )
    .await
    .map_err(|_| AgentError::timeout("Operation timed out"))?;
    
    // 4. 处理结果
    match result {
        Ok(data) => Ok(ToolResult {
            content: vec![Content::text(data)],
            is_error: false,
        }),
        Err(e) => Ok(ToolResult {
            content: vec![Content::text(format!("Error: {}", e))],
            is_error: true,
        }),
    }
}
```

## 7. 返回高信号信息

### 7.1 原则

根据 Anthropic 的工具设计原则，工具应该返回高信号信息：

1. **优先上下文相关性**：只返回与当前任务相关的信息
2. **避免技术标识符**：使用可理解的描述而非 UUID、内部 ID
3. **结构化输出**：使用清晰的格式便于 LLM 理解

### 7.2 示例对比

```rust
// 不好的做法：返回大量低价值信息
Ok(ToolResult {
    content: vec![Content::text(format!(
        "Debug info: {:?}",
        huge_data_structure
    ))],
    is_error: false,
})

// 好的做法：返回关键信息
Ok(ToolResult {
    content: vec![Content::text(format!(
        "Found {} matches in {} files:\n{}",
        match_count,
        file_count,
        top_matches.join("\n")
    ))],
    is_error: false,
})
```

### 7.3 格式化建议

```rust
// 使用结构化格式
Ok(ToolResult {
    content: vec![Content::text(format!(
        r#"Search Results:
- Total matches: {}
- Files searched: {}
- Top results:
  1. {}:{} - {}
  2. {}:{} - {}
  3. {}:{} - {}
"#,
        total_matches,
        files_searched,
        file1, line1, match1,
        file2, line2, match2,
        file3, line3, match3,
    ))],
    is_error: false,
})
```

## 8. 性能优化

### 8.1 异步操作

```rust
// 使用异步 I/O
async fn execute(&self, args: Value, context: ToolContext) -> Result<ToolResult> {
    // 异步读取文件
    let content = tokio::fs::read_to_string(&path).await?;
    
    // 异步处理
    let results = process_content(&content).await?;
    
    Ok(ToolResult {
        content: vec![Content::text(results)],
        is_error: false,
    })
}
```

### 8.2 并发处理

```rust
async fn execute(&self, args: Value, context: ToolContext) -> Result<ToolResult> {
    let paths: Vec<PathBuf> = get_paths(&args)?;
    
    // 并发处理多个文件
    let results: Vec<_> = futures::future::join_all(
        paths.into_iter().map(|path| async move {
            process_file(&path).await
        })
    )
    .await;
    
    let successful: Vec<_> = results.into_iter()
        .filter_map(|r| r.ok())
        .collect();
    
    Ok(ToolResult {
        content: vec![Content::text(format!(
            "Processed {} files successfully",
            successful.len()
        ))],
        is_error: false,
    })
}
```

### 8.3 缓存

```rust
use moka::future::Cache;

pub struct CachedTool {
    cache: Cache<String, String>,
}

impl CachedTool {
    pub fn new() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(100)
                .time_to_live(Duration::from_secs(300))
                .build(),
        }
    }
}

#[async_trait]
impl Tool for CachedTool {
    async fn execute(&self, args: Value, context: ToolContext) -> Result<ToolResult> {
        let key = compute_cache_key(&args);
        
        if let Some(cached) = self.cache.get(&key).await {
            return Ok(ToolResult {
                content: vec![Content::text(cached)],
                is_error: false,
            });
        }
        
        let result = compute_result(&args).await?;
        self.cache.insert(key, result.clone()).await;
        
        Ok(ToolResult {
            content: vec![Content::text(result)],
            is_error: false,
        })
    }
}
```

## 9. 测试

### 9.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_my_tool() {
        let temp_dir = TempDir::new().unwrap();
        let tool = MyCustomTool::new(temp_dir.path().to_path_buf());
        
        // 创建测试文件
        let test_file = temp_dir.path().join("test.txt");
        tokio::fs::write(&test_file, "Hello World\nHello Rust").await.unwrap();
        
        // 测试工具
        let args = json!({
            "file_path": "test.txt",
            "pattern": "Hello"
        });
        
        let result = tool.execute(args, ToolContext::default()).await.unwrap();
        
        assert!(!result.is_error);
        assert!(result.content[0].as_text().unwrap().contains("Hello"));
    }
    
    #[tokio::test]
    async fn test_invalid_input() {
        let temp_dir = TempDir::new().unwrap();
        let tool = MyCustomTool::new(temp_dir.path().to_path_buf());
        
        let args = json!({
            "file_path": "nonexistent.txt",
            "pattern": "test"
        });
        
        let result = tool.execute(args, ToolContext::default()).await.unwrap();
        
        assert!(result.is_error);
    }
}
```

### 9.2 集成测试

```rust
#[tokio::test]
async fn test_tool_with_agent() {
    let registry = ToolRegistry::new();
    registry.register(Arc::new(MyCustomTool::new(workspace))).await;
    
    let agent = create_test_agent(registry).await;
    
    let stream = agent.react(session_config, cancel_token).await;
    let events: Vec<_> = stream.collect().await;
    
    // 验证工具被正确调用
    assert!(events.iter().any(|e| {
        matches!(e, Ok(AgentEvent::Message(msg)) if msg.contains("my_custom_tool"))
    }));
}
```

## 10. 最佳实践总结

### 10.1 设计原则

1. **单一职责**：每个工具只做一件事
2. **明确命名**：工具名称应该清晰表达其功能
3. **详细描述**：提供足够的描述让 LLM 理解用途
4. **合理默认值**：为可选参数提供合理的默认值

### 10.2 实现原则

1. **参数验证**：严格验证所有输入参数
2. **错误处理**：提供清晰的错误信息
3. **资源管理**：正确管理文件句柄、网络连接等资源
4. **超时控制**：为长时间操作设置超时

### 10.3 性能原则

1. **异步优先**：使用异步 I/O 操作
2. **并发处理**：支持并发执行多个独立操作
3. **结果缓存**：缓存计算结果避免重复计算
4. **增量返回**：对于大量数据，考虑分批返回

## 11. 相关文档

- [工具系统](../core-concepts/tool-system.md)
- [Agent执行流程](../core-concepts/agent-execution.md)
- [错误恢复](error-recovery.md)

## 12. 参考资料

- [Anthropic: Writing Tools for Agents](https://www.anthropic.com/engineering/writing-tools-for-agents)
- [MCP Tool Specification](https://modelcontextprotocol.io/specification)
- [LangChain Tool Interface](https://python.langchain.com/docs/modules/tools/)
