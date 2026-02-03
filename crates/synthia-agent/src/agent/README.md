# Agent 模块

Agent 核心模块，实现了基于 ReAct (Reasoning and Acting) 模式的 AI Agent。

## 核心组件

| 组件 | 文件 | 功能描述 |
|------|------|----------|
| `Agent` | [mod.rs](mod.rs) | 主 Agent 结构体 |
| `control` | [control.rs](control.rs) | 生命周期控制与状态管理 |
| `react` | [react.rs](react.rs) | ReAct 推理循环实现 |

## 交互顺序

```
用户输入 → 构建提示 → 调用LLM → 工具执行 → 结果返回 → 循环或结束
```

## Agent 架构

```
Agent
├── config: AgentConfig          # Agent 配置
├── AgentDeps                    # 依赖封装
│   ├── tool_registry: ToolRegistry # 工具注册表
│   ├── context_manager: ContextManager  # 上下文管理
│   ├── session_manager: SessionManager  # 会话管理
│   ├── model_router: ModelRouter   # 模型路由
│   ├── hook_registry: HookRegistry  # 生命周期钩子
│   ├── skill_tool: SkillTool      # 技能工具
│   ├── guardian: Guardian           # 安全审查
│   └── control: AgentControl      # 生命周期控制
├── prompt_state: PromptState    # 提示状态
└── loop_detector: LoopDetector  # 循环检测
```

## AgentControl

`AgentControl` 提供 Agent 生命周期管理和状态监控：

```rust
use synthia_agent::agent::AgentControl;
use synthia_agent::types::AgentStatus;

let control = AgentControl::new();

// 更新状态
control.update_status(AgentStatus::Running);

// 订阅状态变化
let mut receiver = control.subscribe_status();

// 检查是否为最终状态
if control.is_final_status() {
    println!("Agent has finished");
}
```

### AgentStatus 状态

| 状态 | 说明 |
|------|------|
| `PendingInit` | 等待初始化 |
| `Running` | 运行中 |
| `MaxStepsReached(u32)` | 达到最大步数 |
| `Completed` | 完成 |
| `Errored(String)` | 错误 |
| `Shutdown` | 已关闭 |
| `Cancelled` | 已取消 |

## ReAct 推理循环

`Agent::react()` 方法实现 ReAct 推理循环：

```
1. 接收用户消息
2. 检查退出条件（取消、最大步数）
3. 获取并压缩对话历史
4. 调用 LLM 获取响应
5. 检查 stop_reason:
   - 如果不是 tool_use → 返回文本响应
   - 如果是 tool_use → 执行工具
6. 将工具结果追加到对话
7. 循环回到步骤 2
```

## 核心方法

### Agent::new()

创建新的 Agent 实例：

```rust
use synthia_agent::agent::AgentDeps;

let deps = AgentDeps {
    tool_registry: Arc::new(ToolRegistry::new()),
    context_manager: Arc::new(DefaultContextManager::new(...)),
    session_manager: Arc::new(UnifiedStorage::in_memory().await?),
    model_router: Arc::new(FirstModelRouter::default()),
    hook_registry: Arc::new(HookRegistry::new()),
    skill_tool: Arc::new(SkillTool::new(PathBuf::from("."))),
    guardian: Arc::new(SimpleGuardian::new(GuardianConfig::default())) as Arc<dyn Guardian>,
    control: Arc::new(AgentControl::new()),
};

let agent = Agent::new(Arc::new(AgentConfig::default()), deps);
```

### Agent::get_filtered_tools()

获取过滤后的工具列表：

```rust
let tools = agent.get_filtered_tools().await;
```

### Agent::build_system_prompt()

构建系统提示：

```rust
let system_prompt = agent.build_system_prompt().await;
```

## 相关文档

- [ReAct 推理模式](https://arxiv.org/abs/2210.03629)
- [顶层 README](../README.md)
- [Tools 模块](../tools/README.md)
- [Context 模块](../context/README.md)
