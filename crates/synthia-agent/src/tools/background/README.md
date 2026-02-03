# Background Tools

后台任务工具模块，提供非阻塞命令执行能力，Agent 可以启动长时间运行的任务并继续处理其他工作。

## 架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         Agent                                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │background_  │  │background_ │  │background_  │              │
│  │   start     │  │   list     │  │   status    │              │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘              │
│         │                │                │                     │
└─────────┼────────────────┼────────────────┼─────────────────────┘
          │                │                │
          ▼                ▼                ▼
┌─────────────────────────────────────────────────────────────────┐
│                    BackgroundFileStore                           │
│              (file_store.rs - 文件存储实现)                      │
└─────────────────────────────────────────────────────────────────┘
          │                │                │
          ▼                ▼                ▼
┌─────────────────────────────────────────────────────────────────┐
│                      JSON 文件存储                               │
│              (~/.agent/data/background/tasks.json)               │
└─────────────────────────────────────────────────────────────────┘
```

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `background_start` | 启动后台任务 |
| `background_list` | 列出所有后台任务 |
| `background_status` | 查看指定任务状态 |
| `background_stop` | 停止运行中的任务 |

## 存储架构

后台任务数据存储在 `~/.agent/data/background/` 目录：

```
~/.agent/data/background/
└── tasks.json          # 后台任务列表
```

### 任务文件格式

```json
[
  {
    "id": "bg-550e8400-e29b-41d4-a716-446655440000",
    "command": "npm run build",
    "cwd": "/path/to/project",
    "status": "completed",
    "pid": null,
    "started_at": 1700000000,
    "ended_at": 1700000060,
    "exit_code": 0,
    "output": "Build successful...",
    "notification_delivered": true
  }
]
```

## 内部实现

### BackgroundFileStore

```rust
pub(crate) struct BackgroundFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl BackgroundFileStore {
    pub(crate) async fn create_task(&self, task: BackgroundTask) -> Result<()>;
    pub(crate) async fn get_task(&self, id: &str) -> Result<Option<BackgroundTask>>;
    pub(crate) async fn update_task(&self, task: &BackgroundTask) -> Result<()>;
    pub(crate) async fn list_tasks(&self) -> Result<Vec<BackgroundTask>>;
}
```

## 任务状态

| 状态 | 描述 |
|------|------|
| `running` | 任务正在执行 |
| `completed` | 任务成功完成（退出码为 0） |
| `failed` | 任务执行失败（非零退出码） |
| `stopped` | 任务被手动停止 |

## 工作流程

```
Agent                              后台任务                          存储
  │                                   │                              │
  │──background_start───────────────▶│                              │
  │                                   │                               │
  │   返回 task_id                    │                               │
  │◀──────────────────────────────────┼                               │
  │                                   │                               │
  │   (Agent 继续处理其他任务)          │                               │
  │                                   │                               │
  │                                   │──── create_task ─────────────▶│
  │                                   │                               │
  │                                   │◀─── task_id ─────────────────│
  │                                   │                               │
  │                                   │──── update_task ─────────────▶│
  │                                   │   (设置 PID)                  │
  │                                   │                               │
  │                                   │◀─ process running ────────────│
  │                                   │                               │
  │                                   │──── update_task ─────────────▶│
  │                                   │   (设置 output/error/exit_code)│
  │                                   │                               │
  │                                   │                    任务完成     │
  │                                   │                               │
  │◀──────────────────────────────────┼   发送通知                    │
  │                                   │                               │
  │──background_status───────────────▶│                               │
  │◀─── task status ─────────────────│                               │
```

## 使用示例

### 启动后台任务

```json
{
  "name": "background_start",
  "arguments": {
    "command": "npm run build",
    "cwd": "/path/to/project"
  }
}
```

**参数说明：**
- `command` (必填): 要执行的 shell 命令
- `cwd` (可选): 工作目录，默认为当前目录

**返回值：**
```json
{
  "task_id": "bg-550e8400-e29b-41d4-a716-446655440000",
  "command": "npm run build",
  "cwd": "/path/to/project",
  "pid": 12345,
  "status": "started"
}
```

### 查看任务状态

```json
{
  "name": "background_status",
  "arguments": {
    "task_id": "bg-550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**返回值：**
```json
{
  "task_id": "bg-550e8400-e29b-41d4-a716-446655440000",
  "command": "npm run build",
  "status": "completed",
  "pid": 12345,
  "started_at": 1700000000,
  "ended_at": 1700000060,
  "exit_code": 0,
  "output_lines": 50,
  "error_lines": 2
}
```

### 列出所有任务

```json
{
  "name": "background_list",
  "arguments": {}
}
```

**返回值：**
```json
{
  "tasks": [
    {
      "task_id": "bg-550e8400-e29b-41d4-a716-446655440000",
      "command": "npm run build",
      "status": "completed",
      "pid": null,
      "started_at": 1700000000,
      "ended_at": 1700000060,
      "exit_code": 0
    }
  ],
  "count": 1
}
```

### 停止任务

```json
{
  "name": "background_stop",
  "arguments": {
    "task_id": "bg-550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**返回值：**
```json
{
  "success": true,
  "task_id": "bg-550e8400-e29b-41d4-a716-446655440000",
  "message": "Task 'bg-550e8400-e29b-41d4-a716-446655440000' has been stopped"
}
```

## 与 Cron 的区别

| 特性 | Background | Cron |
|------|------------|------|
| 执行时机 | 立即 | 定时 |
| 用途 | 长时间任务 | 定时任务 |
| 生命周期 | 完成后结束 | 周期性 |
| 触发方式 | 主动调用 | 时间驱动 |
| 通知机制 | 任务完成通知 | 无主动通知 |

## 设计理念

> "Run slow operations in the background; the agent keeps thinking"

Background 系统解决的问题：
1. **不被阻塞**: 长时间命令不阻塞 Agent
2. **并发执行**: 同时运行多个任务
3. **进度追踪**: 可查看任务状态和输出
4. **灵活控制**: 可随时停止任务
5. **独立存储**: 使用文件存储，无需外部数据库

## 数据迁移

从 SQLite 迁移到文件存储：

```rust
use synthia_agent::tools::migration::migrate_from_sqlite;

let result = migrate_from_sqlite(
    &PathBuf::from(".agents/synthia.db"),
    None
).await?;
```
