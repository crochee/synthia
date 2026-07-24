# Synthia Agent

生产级 AI Agent 框架，使用 Rust 实现，支持多模型、多工具、持久化会话等企业级功能。

## 核心模块

| 模块 | 功能描述 |
|------|----------|
| [agent](./agent/) | ReAct 推理循环核心实现 |
| [config](./config/) | 统一配置管理 |
| [context](./context/) | 上下文管理和压缩 |
| [error](./error/) | 错误类型定义 |
| [event_log](./event_log/) | 事件日志 |
| [guardian](./guardian/) | 安全审查机制 |
| [hooks](./hooks/) | 生命周期扩展 |
| [memories](./memories/) | 记忆提取系统 |
| [model_router](./model_router/) | 多模型路由 |
| [prompt](./prompt/) | 系统提示构建 |
| [shell](./shell/) | Shell 执行抽象 |
| [tools](./tools/) | 工具系统 |
| [types](./types/) | 核心类型定义 |
| [utils](./utils/) | 工具函数 |

## 工具系统

内置工具位于 [tools](./tools/) 目录：

| 类别 | 工具 |
|------|------|
| 文件系统 | ReadTool, WriteTool, EditTool, DeleteTool, CreateDirectoryTool, DirectoryTreeTool, GrepTool, ListDirectoryTool, MoveFileTool |
| 命令执行 | ExecTool |
| 上下文注入 | ContextInjectTool |
| 任务管理 | TodoWriteTool, task_create, task_update |
| 团队协作 | spawn_teammate, send_message, idle, claim_task |
| 后台任务 | background_start, background_status |
| 定时任务 | cron_add, cron_list |
| 技能加载 | SkillTool |
| 网络工具 | WebFetchTool, WebSearchTool |
| 工作树隔离 | worktree_create, worktree_run |
| 用户交互 | AskUserQuestionTool |
| 思考工具 | SequentialThinkingTool |
| 子代理 | SubagentTool |

## 快速开始

```rust,ignore
use synthia_agent::{
    Agent, AgentError, AgentStatus,
    config::AgentConfig,
    hooks::HookRegistry,
    model_router::FirstModelRouter,
    session::SessionFileStore,
    tools::{SkillTool, ToolRegistry},
    guardian::{Guardian, SimpleGuardian, GuardianConfig},
    agent::{AgentControl, AgentDeps},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let deps = AgentDeps {
        tools: Arc::new(ToolRegistry::new()),
        context: Arc::new(synthia_context::DefaultContextService::new(Default::default())),
        session: Arc::new(SessionFileStore::new()),
        router: Arc::new(FirstModelRouter::default()),
        hooks: Arc::new(HookRegistry::new()),
        skills: Arc::new(SkillTool::new(std::path::PathBuf::from("."))),
        guardian: Arc::new(SimpleGuardian::new(GuardianConfig::default())) as Arc<dyn Guardian>,
        control: Arc::new(AgentControl::new()),
    };

    let agent = Agent::new(Arc::new(AgentConfig::default()), deps);

    Ok(())
}
```

## ReAct 推理循环

```
    ┌──────────────┐
    │  User Input  │
    └──────┬───────┘
           ↓
    ┌──────────────┐
    │   Build      │ ← System Prompt + Context
    │   Prompt     │
    └──────┬───────┘
           ↓
    ┌──────────────┐
    │  Call LLM   │ ← Model Router 选择模型
    └──────┬───────┘
           ↓
    ┌──────────────┐
    │ stop_reason │
    │   ==        │──Yes──→ Return Response
    │ tool_use?   │
           │No
           ↓
    ┌──────────────┐
    │  Execute     │ ← Guardian 审查
    │   Tools     │
    └──────┬───────┘
           ↓
    ┌──────────────┐
    │  Append      │
    │  Results     │
    └──────┬───────┘
           ↓
      ┌────┴────┐
      │ Loop    │
      └─────────┘
```

## 安全机制

Guardian 模块提供三级安全审查：

1. **自动批准**: 低风险操作自动通过
2. **人工审批**: 中风险操作需要用户确认
3. **拒绝执行**: 高风险操作直接拒绝

## 上下文管理

四层压缩策略确保长会话正常运行：

1. **Level 1 (None)**: 使用率 < 70%，不压缩
2. **Level 2 (Soft Pruning)**: 70% <= 使用率 < 85%，软剪枝
3. **Level 3 (Hard Clearing + Summary)**: 85% <= 使用率 < 95%，硬清除 + 摘要
4. **Level 4 (Emergency Truncation)**: 使用率 >= 95%，紧急截断

## 持久化

- **Session**: 会话历史持久化
- **Memory**: 长期记忆向量存储
- **Task**: 任务和依赖关系持久化
- **Cron**: 定时任务持久化

## 测试

运行所有测试：

```bash
cargo test -p synthia-agent --lib
```

运行特定模块测试：

```bash
cargo test -p synthia-agent agent:: --lib
cargo test -p synthia-agent context:: --lib
cargo test -p synthia-agent tools:: --lib
```
