# Team Tools

团队协作工具模块，提供多 Agent 协作能力，支持创建子代理、消息传递和任务认领。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `spawn_teammate` | 创建团队成员 |
| `list_teammates` | 列出所有团队成员 |
| `send_message` | 发送消息给团队成员 |
| `read_inbox` | 读取收件箱 |
| `broadcast` | 广播消息给所有成员 |
| `shutdown_request` | 请求关闭团队成员 |
| `shutdown_response` | 检查关闭请求状态 |
| `plan_approval` | 审批团队成员计划 |
| `idle` | 标记空闲状态 |
| `claim_task` | 认领任务 |
| `scan_unclaimed` | 扫描未认领任务 |

## 存储架构

团队数据存储在 `~/.agent/data/` 目录：

```
~/.agent/data/
├── teammates/
│   ├── index.json          # 队友索引
│   └── {name}.json         # 队友详情
├── teams/
│   ├── index.json          # 团队索引
│   └── {team_id}.json      # 团队详情
├── messages/
│   └── {recipient}.jsonl   # 消息队列（JSONL 格式）
└── protocol/
    ├── shutdown_requests.json  # 关闭请求
    └── plan_requests.json      # 计划请求
```

### 队友索引格式

```json
{
  "version": 1,
  "updated_at": 1705312200,
  "items": [
    {
      "id": "code-reviewer",
      "subject": "代码审查专家",
      "status": "active",
      "updated_at": 1705312200
    }
  ]
}
```

### 消息队列格式 (JSONL)

```jsonl
{"id":"msg-1","sender":"agent-1","recipient":"code-reviewer","type":"text","content":"请审查代码","timestamp":1705312200}
{"id":"msg-2","sender":"agent-2","recipient":"code-reviewer","type":"text","content":"收到","timestamp":1705312300}
```

## 交互顺序

```
Agent → 创建团队成员 → 分配任务 → 消息通信 → 协作完成
```

## 在 Agent 中的作用

1. **多 Agent 协作**: 创建多个子代理协同工作
2. **消息传递**: 团队成员间消息通信
3. **任务分配**: 通过任务板分配和认领任务
4. **生命周期管理**: 团队成员的启动和关闭

## 使用示例

### 创建团队成员

```json
{
  "name": "spawn_teammate",
  "arguments": {
    "name": "code-reviewer",
    "role": "负责代码审查"
  }
}
```

### 发送消息

```json
{
  "name": "send_message",
  "arguments": {
    "to": "code-reviewer",
    "message": "请审查这段代码"
  }
}
```

### 认领任务

```json
{
  "name": "claim_task",
  "arguments": {
    "taskId": "task-123"
  }
}
```

## 内部实现

### TeammateFileStore

```rust
pub(crate) struct TeammateFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl TeammateFileStore {
    pub(crate) async fn create_teammate(&self, teammate: Teammate) -> Result<()>;
    pub(crate) async fn get_teammate(&self, name: &str) -> Result<Option<Teammate>>;
    pub(crate) async fn list_teammates(&self) -> Result<Vec<Teammate>>;
    pub(crate) async fn update_teammate(&self, teammate: &Teammate) -> Result<()>;
}
```

### MessageFileStore (JSONL)

```rust
pub(crate) struct MessageFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl MessageFileStore {
    pub(crate) async fn send_message(&self, message: TeamMessage) -> Result<()>;
    pub(crate) async fn read_messages(&self, recipient: &str) -> Result<Vec<TeamMessage>>;
    pub(crate) async fn mark_delivered(&self, recipient: &str, message_ids: &[String]) -> Result<()>;
}
```

### ProtocolFileStore

```rust
pub(crate) struct ProtocolFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl ProtocolFileStore {
    pub(crate) async fn create_shutdown_request(&self, request: ShutdownRequest) -> Result<()>;
    pub(crate) async fn get_shutdown_request(&self, name: &str) -> Result<Option<ShutdownRequest>>;
    pub(crate) async fn create_plan_request(&self, request: PlanRequest) -> Result<()>;
    pub(crate) async fn get_plan_request(&self, name: &str) -> Result<Option<PlanRequest>>;
}
```

## 设计理念

> 团队协作提高效率

Team 系统解决的问题：
1. **并行处理**: 多个任务同时处理
2. **专业化**: 团队成员各司其职
3. **通信**: 成员间消息传递
4. **协调**: 任务分配和协调

## 索引缓存

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
