# Tools 模块

工具系统模块，提供 Agent 与外部世界交互的能力。

## 核心组件

| 组件 | 功能描述 |
|------|----------|
| `Tool` | 工具 trait，定义工具的基本接口 |
| `ToolRegistry` | 工具注册表，管理工具的注册和获取 |
| `value_to_object` | JSON Value 转换为 JsonObject 的工具函数 |
| `storage` | 文件存储基础库，提供 JSON/JSONL 读写功能 |

## 工具 Trait

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn call(&self, args: Value) -> CallToolResult;
}
```

## 存储架构

### 文件存储

工具使用文件存储进行数据持久化，存储在 `~/.agent/data/` 目录：

```
~/.agent/data/
├── tasks/              # 任务数据
│   ├── index.json      # 任务索引
│   └── {id}.json       # 任务详情
├── teammates/          # 队友数据
│   ├── index.json      # 队友索引
│   └── {name}.json     # 队友详情
├── teams/              # 团队数据
│   ├── index.json      # 团队索引
│   └── {id}.json       # 团队详情
├── messages/           # 消息队列
│   └── {recipient}.jsonl  # 消息（JSONL 格式）
├── protocol/           # 协议数据
│   ├── shutdown_requests.json
│   └── plan_requests.json
└── background/         # 后台任务
    └── tasks.json
```

### 存储优势

- **工具独立**：每个工具管理自己的存储实例
- **简化部署**：无需初始化数据库
- **人类可读**：JSON 格式，便于查看和调试
- **易于迁移**：直接复制文件即可

### 数据迁移

从 SQLite 迁移到文件存储：

```rust
use synthia_agent::tools::migration::migrate_from_sqlite;

let result = migrate_from_sqlite(
    &PathBuf::from(".agents/synthia.db"),
    None
).await?;

println!("Migrated {} items", result.total_migrated());
```

## 工具注册

### 内置工具注册

`register_builtin_tools` 函数注册所有内置工具：

```rust
pub async fn register_builtin_tools(registry: &ToolRegistry) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(fs::ReadTool::new()),
        Arc::new(fs::WriteTool::new()),
        Arc::new(fs::CreateDirectoryTool::new()),
        Arc::new(fs::DeleteTool::new()),
        Arc::new(fs::DirectoryTreeTool::new()),
        Arc::new(fs::EditTool::new()),
        Arc::new(fs::GrepTool::new()),
        Arc::new(fs::ListDirectoryTool::new()),
        Arc::new(fs::MoveFileTool::new()),
        Arc::new(todo::TodoWriteTool::new()),
        Arc::new(tom::ContextInjectTool::new()),
        Arc::new(web::WebFetchTool::new()),
        Arc::new(web::WebSearchTool::new()),
        Arc::new(thinking::SequentialThinkingTool::new_with_stdout()),
    ];
    registry.registers(tools.into_iter()).await;
}
```

### 工具特定注册

Task 工具注册（无需存储参数）：

```rust
pub async fn register_task_tools(registry: &ToolRegistry) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(TaskCreateTool::new()),
        Arc::new(TaskGetTool::new()),
        Arc::new(TaskListTool::new()),
        Arc::new(TaskUpdateTool::new()),
        Arc::new(TaskDeleteTool::new()),
        Arc::new(ClaimTaskTool::new()),
        Arc::new(TaskDelegateTool::new()),
    ];
    registry.registers(tools.into_iter()).await;
}
```

### ToolRegistry API

```rust
impl ToolRegistry {
    pub async fn register(&self, tool: Arc<dyn Tool>);
    pub async fn registers(&self, tools: impl Iterator<Item = Arc<dyn Tool>>);
    pub async fn unregister(&self, name: &str) -> bool;
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
    pub async fn tool_count(&self) -> usize;
    pub async fn tool_names(&self) -> Vec<String>;
    pub async fn filtered_tools(&self, allowed_tools: &[String], denied_tools: &[String]) -> Vec<Arc<dyn Tool>>;
}
```

## 交互顺序

```
LLM 决策 → 工具选择 → 参数验证 → 执行 → 返回结果
```

## 安全机制

- 工具权限控制（allowed/denied）
- Guardian 审查
- 路径验证

## 测试

```bash
# 运行所有 tools 测试
cargo test -p synthia-agent tools:: --lib

# 运行特定工具测试
cargo test -p synthia-agent tools::fs:: --lib
cargo test -p synthia-agent tools::skill:: --lib

# 运行存储测试
cargo test -p synthia-agent tools::storage:: --lib
```

## 相关文档

- [Agent 模块](../agent/README.md)
- [Guardian 模块](../guardian/README.md)
- [Storage 模块](../storage/README.md)

## 子模块文档

- [fs](./fs/README.md) - 文件系统工具
- [exec](./exec/README.md) - 命令执行工具
- [todo](./todo/README.md) - 任务规划工具
- [task](./task/README.md) - 持久化任务工具
- [team](./team/README.md) - 团队协作工具
- [worktree](./worktree/README.md) - 工作树隔离工具
- [background](./background/README.md) - 后台任务工具
- [cron](./cron/README.md) - 定时任务工具
- [skill](./skill/README.md) - 技能加载工具
- [subagent](./subagent/README.md) - 子代理工具
- [ask_user](./ask_user/README.md) - 用户交互工具
- [mcp](./mcp/README.md) - MCP 协议工具
- [tom](./tom/README.md) - ContextInject 上下文注入工具
- [web](./web/README.md) - 网络工具
- [thinking](./thinking/README.md) - 结构化思考工具
- [migration](./migration/README.md) - 数据迁移工具
