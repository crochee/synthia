# Task Tools

持久化任务工具模块，提供长期任务存储和管理能力，任务信息保存在文件系统中。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `task_create` | 创建新任务 |
| `task_list` | 列出所有任务 |
| `task_get` | 获取任务详情 |
| `task_update` | 更新任务状态 |
| `task_delete` | 删除任务 |
| `claim_task` | 认领任务 |
| `task_delegate` | 委托任务 |

## 存储架构

任务数据存储在 `~/.agent/data/tasks/` 目录：

```
~/.agent/data/tasks/
├── index.json          # 任务索引（快速查询）
└── {task_id}.json      # 任务详情
```

### 索引文件格式

```json
{
  "version": 1,
  "updated_at": 1705312200,
  "items": [
    {
      "id": "task-123",
      "subject": "实现用户登录功能",
      "status": "in_progress",
      "updated_at": 1705312200
    }
  ]
}
```

### 详情文件格式

```json
{
  "id": "task-123",
  "subject": "实现用户登录功能",
  "description": "需要实现 JWT 认证",
  "status": "in_progress",
  "blocked_by": [],
  "blocks": [],
  "owner": "agent-1",
  "team_id": null,
  "priority": "high",
  "created_at": 1705312200,
  "updated_at": 1705312200
}
```

## 交互顺序

```
Agent → 创建/查询/更新任务 → 文件存储 → 返回结果
```

## 在 Agent 中的作用

1. **持久化存储**: 任务信息保存在 JSON 文件中
2. **任务追踪**: 长期运行任务的状态追踪
3. **依赖管理**: 支持任务间依赖关系
4. **团队协作**: 多 Agent 间的任务共享

## 使用示例

### 创建任务

```json
{
  "name": "task_create",
  "arguments": {
    "subject": "实现用户登录功能",
    "description": "需要实现 JWT 认证",
    "status": "pending",
    "blockedBy": [],
    "blocks": []
  }
}
```

### 列出任务

```json
{
  "name": "task_list",
  "arguments": {}
}
```

### 更新任务

```json
{
  "name": "task_update",
  "arguments": {
    "id": "task-123",
    "status": "completed"
  }
}
```

## 与 TodoWrite 的区别

| 特性 | Task | TodoWrite |
|------|------|-----------|
| 存储 | JSON 文件 | 内存 |
| 持久化 | 跨会话持久 | 仅当前会话 |
| 用途 | 长期任务追踪 | 会话内规划 |

## 设计理念

> 任务需要持久化追踪

Task 系统解决的问题：
1. **长期存储**: 任务不随会话结束丢失
2. **跨会话**: 支持多个会话间任务追踪
3. **结构化**: 完整任务元数据管理
4. **协作**: 多 Agent 间任务共享

## 内部实现

### TaskFileStore

```rust
pub(crate) struct TaskFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl TaskFileStore {
    pub(crate) async fn create_task(&self, task: Task) -> Result<String>;
    pub(crate) async fn get_task(&self, id: &str) -> Result<Option<Task>>;
    pub(crate) async fn list_tasks(&self) -> Result<Vec<Task>>;
    pub(crate) async fn update_task(&self, task: &Task) -> Result<()>;
    pub(crate) async fn delete_task(&self, id: &str) -> Result<()>;
}
```

### 索引缓存

索引文件会被缓存以提高性能：

- 默认 TTL: 5 分钟
- 自动检测文件修改时间
- 支持手动失效

## 数据迁移

从 SQLite 迁移到文件存储：

```rust
use synthia_agent::tools::migration::migrate_from_sqlite;

let result = migrate_from_sqlite(
    &PathBuf::from(".agents/synthia.db"),
    None
).await?;
```
