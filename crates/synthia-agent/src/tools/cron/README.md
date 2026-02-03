# Cron Tools

定时任务工具模块，提供周期性任务调度能力，Agent 可以安排未来自动执行的任务。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `cron_add` | 添加定时任务 |
| `cron_list` | 列出定时任务 |
| `cron_get` | 获取任务详情 |
| `cron_remove` | 删除定时任务 |
| `cron_update` | 更新定时任务 |
| `cron_run` | 立即执行任务 |
| `cron_runs` | 查看执行历史 |

## 存储架构

定时任务数据存储在 `~/.agent/data/cron/` 目录：

```
~/.agent/data/cron/
├── jobs.json           # 定时任务列表
└── runs/
    └── {job_id}.jsonl  # 运行历史（JSONL 格式）
```

### 任务文件格式

```json
[
  {
    "id": "job-uuid",
    "crontab": "0 * * * *",
    "description": "每小时同步代码",
    "content": "git pull",
    "enabled": true,
    "created_at": "2024-01-15T10:00:00Z",
    "next_run": "2024-01-15T11:00:00Z",
    "last_run": "2024-01-15T10:00:00Z",
    "last_status": "ok",
    "last_output": "Already up to date."
  }
]
```

### 运行历史格式 (JSONL)

```jsonl
{"id":1,"job_id":"job-uuid","started_at":"2024-01-15T10:00:00Z","finished_at":"2024-01-15T10:00:01Z","status":"ok","output":"Already up to date.","duration_ms":1000}
```

## 交互顺序

```
Agent → 创建定时任务 → 调度器管理 → 时间触发 → 执行任务 → 记录结果
```

## 在 Agent 中的作用

1. **定时执行**: 自动执行周期性任务
2. **任务调度**: 管理多个定时任务
3. **自动化**: 实现无人值守的自动化
4. **历史追踪**: 记录执行历史

## Agent 运行机制

### 架构

```
TimeWheel (时间轮调度器)
    │
    ├── cron_add → 插入任务
    ├── cron_remove → 移除任务
    ├── 定时触发 → 执行任务
    └── cron_runs → 查询历史
```

### 工作流程

```
1. Agent 创建定时任务
      ↓
2. 解析 cron 表达式
      ↓
3. 插入时间轮调度器
      ↓
4. 时间到达触发执行
      ↓
5. 执行任务并记录结果
```

### Cron 表达式

标准 5 字段格式：
```
┌───────────── 分钟 (0 - 59)
│ ┌─────────── 小时 (0 - 23)
│ │ ┌───────── 日期 (1 - 31)
│ │ │ ┌─────── 月份 (1 - 12)
│ │ │ │ ┌───── 星期 (0 - 6)
│ │ │ │ │
* * * * *
```

示例：
- `0 * * * *`: 每小时执行
- `0 0 * * *`: 每天午夜执行
- `*/5 * * * *`: 每 5 分钟执行

## 内部实现

### CronFileStore

```rust
pub(crate) struct CronFileStore {
    base: FileStore,
    paths: StoragePaths,
    run_id_counter: AtomicI64,
}

impl CronFileStore {
    pub(crate) async fn create_job(&self, job: &CronJob) -> Result<()>;
    pub(crate) async fn find_job(&self, job_id: &str) -> Result<CronJob>;
    pub(crate) async fn all_jobs(&self) -> Result<Vec<CronJob>>;
    pub(crate) async fn delete_job(&self, job_id: &str) -> Result<()>;
    pub(crate) async fn patch_job(&self, job_id: &str, patch: &CronJobPatch) -> Result<CronJob>;
    pub(crate) async fn find_due_jobs(&self, now: DateTime<Utc>) -> Result<Vec<CronJob>>;
    pub(crate) async fn save_run(&self, ...) -> Result<()>;
    pub(crate) async fn get_runs(&self, job_id: &str, limit: usize) -> Result<Vec<CronRun>>;
}
```

## 使用示例

### 添加任务

```json
{
  "name": "cron_add",
  "arguments": {
    "crontab": "0 * * * *",
    "content": "git pull",
    "description": "每小时同步代码"
  }
}
```

### 查看任务

```json
{
  "name": "cron_list",
  "arguments": {}
}
```

### 立即执行

```json
{
  "name": "cron_run",
  "arguments": {
    "job_id": "job-uuid"
  }
}
```

### 查看历史

```json
{
  "name": "cron_runs",
  "arguments": {
    "job_id": "job-uuid"
  }
}
```

## 设计理念

> Agent 可以安排自己的未来任务

Cron 系统让 Agent 能够：
1. **自我调度**: 安排周期性工作
2. **主动提醒**: 设置提醒任务
3. **自动化运维**: 定时执行维护任务
4. **持续运行**: 即使 Agent 空闲也有事可做

## 与 Background 的区别

| 特性 | Background | Cron |
|------|------------|------|
| 执行时机 | 立即 | 定时 |
| 用途 | 长时间任务 | 定时任务 |
| 生命周期 | 完成后结束 | 周期性 |
| 触发方式 | 主动 | 时间驱动 |

## 数据迁移

从 SQLite 迁移到文件存储：

```rust
use synthia_agent::tools::migration::migrate_from_sqlite;

let result = migrate_from_sqlite(
    &PathBuf::from(".agents/synthia.db"),
    None
).await?;
```
