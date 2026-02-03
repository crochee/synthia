---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 记忆系统

## 1. 概述

Synthia Agent 实现了完整的记忆系统，用于管理会话上下文、提取关键信息和维护长期记忆。记忆系统采用两阶段管道设计，支持会话记忆提取、记忆整合和上下文压缩。

## 2. 记忆类型

### 2.1 工作记忆（Working Memory）

工作记忆是 Agent 当前正在使用的上下文信息，包括：

- **当前对话历史**：用户和助手之间的消息交互
- **工具调用结果**：最近执行的工具及其输出
- **系统提示**：Agent 的指令和配置
- **技能指南**：当前加载的技能文档

**特点**：
- 存储在内存中，访问速度快
- 受上下文窗口限制
- 会话结束后清空

### 2.2 短期记忆（Short-term Memory）

短期记忆保存在会话存储中，包括：

- **会话消息**：完整的对话历史
- **会话元数据**：创建时间、更新时间、消息计数
- **会话状态**：当前执行状态、错误信息

**特点**：
- 持久化到数据库（SQLite）
- 跨请求保持
- 支持会话恢复

### 2.3 长期记忆（Long-term Memory）

长期记忆通过记忆提取和整合生成，包括：

- **关键决策**：会话中做出的重要决策
- **用户偏好**：用户的编码风格、工具偏好
- **项目知识**：项目结构、技术栈、依赖关系
- **错误教训**：失败的操作和解决方案

**特点**：
- 持久化到文件系统
- 跨会话保持
- 支持检索和查询

### 2.4 会话记忆（Session Memory）

会话记忆是一种特殊的长期记忆，用于维护会话级别的摘要：

- **当前项目**：项目名称和概述
- **关键决策**：会话中的重要决策
- **活跃上下文**：当前正在进行的工作
- **重要发现**：关键发现或洞察
- **待办任务**：需要关注的任务

**特点**：
- 定期从对话历史中提取
- 在上下文压缩后保持关键信息
- 支持手动和自动更新

## 3. 两阶段记忆管道

### 3.1 管道架构

```
┌─────────────────────────────────────────────────────────────┐
│                      Memory Pipeline                          │
│                                                              │
│  会话结束                                                     │
│     │                                                        │
│     ▼                                                        │
│  ┌──────────────┐                                            │
│  │   Phase 1    │  原始记忆提取                              │
│  │  Extraction  │  - 关键决策                                │
│  │              │  - 重要信息                                │
│  │              │  - 用户偏好                                │
│  └──────┬───────┘                                            │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐                                            │
│  │   Phase 2    │  记忆整合                                  │
│  │Consolidation │  - 主题分类                                │
│  │              │  - 摘要生成                                │
│  │              │  - 长期存储                                │
│  └──────┬───────┘                                            │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐                                            │
│  │   Storage    │  持久化存储                                │
│  │              │  - 文件系统                                │
│  │              │  - 数据库                                  │
│  └──────────────┘                                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Phase 1: 原始记忆提取

从会话历史中提取原始记忆：

**提取内容**：

| 类型 | 说明 |
|------|------|
| 关键决策 | 重要的技术选择、架构决策 |
| 重要信息 | 发现的bug、性能问题、安全漏洞 |
| 用户偏好 | 编码风格、工具选择、工作流程 |
| 项目知识 | 文件结构、依赖关系、配置信息 |

**提取触发**：

- 会话结束时
- 达到token阈值时（默认：30,000 tokens）
- 手动触发时

### 3.3 Phase 2: 记忆整合

将原始记忆整合为有意义的摘要：

**整合过程**：

1. **主题分类**：将记忆按主题分组
2. **去重合并**：合并相似的记忆项
3. **摘要生成**：生成主题级别的摘要
4. **长期存储**：持久化到文件系统

**整合触发**：

- 定期调度（Cron任务）
- 手动触发

## 4. 会话记忆系统

### 4.1 会话记忆配置

```rust
pub struct SessionMemoryConfig {
    pub minimum_message_tokens_to_init: usize,    // 初始化阈值（默认：30,000）
    pub minimum_tokens_between_update: usize,     // 更新间隔（默认：10,000）
    pub tool_calls_between_updates: usize,        // 工具调用间隔（默认：20）
}
```

### 4.2 会话记忆模板

会话记忆使用标准模板：

```markdown
# Session Memory

This file contains extracted session memory that captures key information from the conversation.

## Current Project

<!-- Project name and overview -->

## Key Decisions

<!-- Important decisions made during the session -->

## Active Context

<!-- Current work being performed -->

## Important Findings

<!-- Key discoveries or insights -->

## Pending Tasks

<!-- Tasks that need attention -->
```

### 4.3 会话记忆更新

会话记忆通过 LLM 自动更新：

```
1. 读取当前会话记忆
2. 分析自上次更新以来的对话历史
3. 识别新的关键信息
4. 更新会话记忆文件
```

**更新提示**：

```rust
pub fn build_session_memory_update_prompt(
    current_memory: &str,
    memory_path: &Path,
) -> String {
    format!(
        r#"You are a session memory extraction assistant. Your task is to analyze the conversation history and update the session memory file.

## Current Session Memory

{}

## Instructions

1. Read the conversation history since the last extraction
2. Identify new key information:
   - Important decisions or direction changes
   - Current task progress and context
   - Newly discovered information relevant to the project
   - Pending tasks or next steps
3. Update the session memory file at: {}

Only include substantive updates. If nothing significant has changed, preserve the existing memory unchanged.

## Output Format

Provide the complete updated memory content that should replace the current file contents.
"#,
        current_memory,
        memory_path.display()
    )
}
```

### 4.4 手动提取会话记忆

```rust
use synthia_agent::memories::session_memory::{
    manually_extract_session_memory,
    ManualExtractionResult,
};

let result = manually_extract_session_memory(
    &messages,
    &workspace_dir,
).await?;

println!("Memory content: {}", result.memory_content);
println!("Token count: {}", result.token_count);
```

## 5. 存储架构

### 5.1 存储结构

```
{workspace}/
├── .agents/
│   └── synthia.db              # SQLite 数据库
│       ├── sessions            # 会话表
│       ├── messages            # 消息表
│       ├── memories            # 记忆表
│       └── cron_jobs           # 定时任务表
│
└── memories/
    ├── raw_memories.md         # 原始记忆
    ├── session_memory.md       # 会话记忆
    ├── rollout_summaries/      # 会话摘要
    │   ├── thread-123.md
    │   └── thread-456.md
    └── consolidated/           # 整合记忆
        ├── architecture.md
        ├── dependencies.md
        └── decisions.md
```

### 5.2 数据库表结构

#### sessions 表

```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    name TEXT,
    created_at TEXT,
    updated_at TEXT,
    message_count INTEGER
);
```

#### messages 表

```sql
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT,
    role TEXT,
    content TEXT,
    created_at TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);
```

#### memories 表

```sql
CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    session_id TEXT,
    content TEXT,
    memory_type TEXT,
    importance TEXT,
    tags TEXT,
    embedding BLOB,
    created_at TEXT,
    updated_at TEXT,
    accessed_at TEXT,
    access_count INTEGER
);
```

### 5.3 SessionManager Trait

```rust
#[async_trait]
pub trait SessionManager: Send + Sync {
    async fn get_session(&self, session_config: &SessionConfig) -> Result<Option<Session>>;
    async fn create_session(&self) -> Result<Session>;
    async fn update_session(&self, session: &Session) -> Result<()>;
    async fn delete_session(&self, session_config: &SessionConfig) -> Result<()>;
    
    async fn add_message(&self, session_config: &SessionConfig, message: &SamplingMessage) -> Result<()>;
    async fn get_conversation(&self, session_config: &SessionConfig) -> Result<Vec<SamplingMessage>>;
    async fn replace_conversation(&self, session_config: &SessionConfig, conversation: &[SamplingMessage]) -> Result<()>;
    
    async fn get_recent_conversations(&self, limit: usize) -> Result<Vec<Session>>;
    async fn fix_conversation(&self, session_config: &SessionConfig) -> Result<Vec<SamplingMessage>>;
}
```

## 6. 记忆检索

### 6.1 MemoryQuery

```rust
pub struct MemoryQuery {
    pub session_id: Option<String>,
    pub memory_types: Option<Vec<MemoryType>>,
    pub min_importance: Option<Importance>,
    pub limit: usize,
}

let query = MemoryQuery {
    session_id: Some("session-123".to_string()),
    memory_types: Some(vec![MemoryType::Decision, MemoryType::Finding]),
    min_importance: Some(Importance::High),
    limit: 10,
};

let memories = storage.recall(&query).await?;
```

### 6.2 检索策略

| 策略 | 说明 | 适用场景 |
|------|------|----------|
| 按会话检索 | 检索特定会话的记忆 | 恢复会话上下文 |
| 按类型检索 | 检索特定类型的记忆 | 查找决策、发现 |
| 按重要性检索 | 检索高重要性记忆 | 获取关键信息 |
| 混合检索 | 组合多个条件 | 精确查找 |

## 7. 记忆持久化

### 7.1 会话持久化

会话数据自动持久化到 SQLite 数据库：

```rust
// 创建会话
let session = storage.create_session().await?;

// 添加消息
storage.add_message(&session_config, &message).await?;

// 获取会话
let session = storage.get_session(&session_config).await?;

// 删除会话
storage.delete_session(&session_config).await?;
```

### 7.2 记忆导出/导入

```rust
// 导出记忆
let memories = storage.export_memories().await?;

// 导入记忆
storage.import_memories(&memories).await?;
```

## 8. 记忆与上下文管理的关系

### 8.1 协作机制

记忆系统与上下文管理紧密协作：

```
┌─────────────────────────────────────────────────────────────┐
│                 Memory & Context Interaction                 │
│                                                              │
│  用户消息                                                     │
│     │                                                        │
│     ▼                                                        │
│  ┌──────────────┐                                            │
│  │  加载会话    │  从数据库加载会话                           │
│  │  上下文      │                                            │
│  └──────┬───────┘                                            │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐                                            │
│  │  加载会话    │  从文件加载会话记忆                         │
│  │  记忆        │                                            │
│  └──────┬───────┘                                            │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐                                            │
│  │  构建完整    │  合并上下文和记忆                           │
│  │  上下文      │                                            │
│  └──────┬───────┘                                            │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐                                            │
│  │  执行Agent   │  ReAct 循环                                │
│  └──────┬───────┘                                            │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐                                            │
│  │  上下文压缩  │  必要时压缩上下文                           │
│  └──────┬───────┘                                            │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐                                            │
│  │  更新会话    │  提取并更新会话记忆                         │
│  │  记忆        │                                            │
│  └──────────────┘                                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 8.2 上下文压缩后的记忆恢复

当上下文被压缩时，会话记忆确保关键信息不丢失：

```
压缩前：
┌─────────────────────────────────────────┐
│ 完整对话历史（100条消息）                │
│ - 用户消息                              │
│ - 助手消息                              │
│ - 工具调用                              │
│ - 工具结果                              │
└─────────────────────────────────────────┘

压缩后：
┌─────────────────────────────────────────┐
│ 会话记忆摘要                            │
│ - 关键决策：选择 PostgreSQL             │
│ - 当前任务：实现用户认证                │
│ - 重要发现：密码需要加盐                │
│ - 待办任务：编写测试                    │
└─────────────────────────────────────────┘
┌─────────────────────────────────────────┐
│ 最近对话（10条消息）                    │
│ - 用户：继续实现登录功能                │
│ - 助手：好的，我来实现...               │
└─────────────────────────────────────────┘
```

## 9. 最佳实践

### 9.1 配置合理的记忆阈值

```rust
let config = SessionMemoryConfig {
    minimum_message_tokens_to_init: 30_000,  // 30k tokens 后初始化
    minimum_tokens_between_update: 10_000,   // 每增加 10k tokens 更新
    tool_calls_between_updates: 20,          // 每 20 次工具调用更新
};
```

### 9.2 定期备份记忆

```bash
# 备份记忆目录
cp -r .agents/memories/ backup/memories_$(date +%Y%m%d)/

# 备份数据库
cp .agents/synthia.db backup/synthia_$(date +%Y%m%d).db
```

### 9.3 使用记忆检索增强上下文

```rust
// 在开始新任务前，检索相关记忆
let query = MemoryQuery {
    memory_types: Some(vec![MemoryType::Decision, MemoryType::Finding]),
    min_importance: Some(Importance::High),
    limit: 5,
};

let relevant_memories = storage.recall(&query).await?;

// 将记忆注入到系统提示中
let system_prompt = format!(
    "{}\n\n## Relevant Context from Previous Sessions\n\n{}",
    base_system_prompt,
    format_memories(&relevant_memories)
);
```

### 9.4 监控记忆使用

```rust
// 检查记忆文件大小
let memory_size = tokio::fs::metadata("memories/session_memory.md")
    .await?
    .len();

if memory_size > 100_000 {  // 100KB
    tracing::warn!("Session memory file is large, consider consolidation");
}
```

## 10. 故障排查

### 10.1 记忆未更新

**症状**：会话记忆文件内容过时

**排查步骤**：
1. 检查是否达到更新阈值
2. 检查 LLM 调用是否成功
3. 检查文件写入权限

### 10.2 记忆检索失败

**症状**：无法检索到记忆

**排查步骤**：
1. 检查数据库连接
2. 检查查询条件是否正确
3. 检查记忆是否已存储

### 10.3 记忆文件损坏

**症状**：记忆文件无法读取

**排查步骤**：
1. 检查文件格式是否正确
2. 从备份恢复
3. 重新提取记忆

## 11. 相关文档

- [Agent执行流程](agent-execution.md)
- [上下文管理](context-management.md)
- [会话管理](../api-reference/API_GUIDE.md#4-会话管理)

## 12. 参考资料

- [OpenAI Thread Management](https://platform.openai.com/docs/assistants/how-it-works/managing-threads-and-messages)
- [LangChain Memory Module](https://python.langchain.com/docs/modules/memory/)
- [Agent-Zero Memory Architecture](https://github.com/frdel/agent-zero)
