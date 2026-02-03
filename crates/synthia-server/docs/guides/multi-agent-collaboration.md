---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 多Agent协作

## 1. 概述

Synthia Agent 支持多 Agent 协作模式，允许多个专业化的 Agent 协同完成复杂任务。本文档说明子 Agent 架构、协作模式和任务分解策略。

## 2. 子Agent架构

### 2.1 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                      Multi-Agent System                      │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │                   Main Agent                         │    │
│  │  - 任务分解                                          │    │
│  │  - 任务调度                                          │    │
│  │  - 结果汇总                                          │    │
│  └──────────────────┬──────────────────────────────────┘    │
│                     │                                        │
│         ┌──────────┼──────────┬──────────┐                 │
│         │          │          │          │                 │
│         ▼          ▼          ▼          ▼                 │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│  │SubAgent 1│ │SubAgent 2│ │SubAgent 3│ │SubAgent N│      │
│  │(Reviewer)│ │(Tester)  │ │(Doc Gen) │ │(Custom)  │      │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 核心特性

1. **独立上下文窗口**：每个子 Agent 有独立的上下文
2. **专业化配置**：每个子 Agent 可配置特定的工具和技能
3. **任务委托**：主 Agent 通过 Task 工具委托任务
4. **结果汇总**：子 Agent 只返回相关发现，不返回完整上下文

## 3. 子Agent配置

### 3.1 配置示例

```yaml
agents:
  code-reviewer:
    description: "代码审查 Agent"
    model: "gpt-4"
    max_steps: 50
    allowed_tools:
      - read
      - grep
      - glob
    denied_tools:
      - exec
      - write
      - delete
    hidden: false
    color: "blue"
  
  test-generator:
    description: "测试生成 Agent"
    model: "gpt-4"
    max_steps: 30
    allowed_tools:
      - read
      - write
      - grep
    denied_tools: []
    hidden: false
  
  documentation-writer:
    description: "文档生成 Agent"
    model: "gpt-4"
    max_steps: 20
    allowed_tools:
      - read
      - write
      - glob
    denied_tools:
      - exec
      - delete
```

### 3.2 配置参数

| 参数 | 类型 | 说明 |
|------|------|------|
| `description` | string | Agent 描述 |
| `model` | string | 使用的模型 |
| `max_steps` | integer | 最大执行步数 |
| `allowed_tools` | array | 允许使用的工具列表 |
| `denied_tools` | array | 禁止使用的工具列表 |
| `hidden` | boolean | 是否隐藏（不在列表中显示） |
| `color` | string | 终端显示颜色 |

## 4. Task 工具

### 4.1 工具接口

主 Agent 通过 Task 工具调用子 Agent：

```rust
pub struct TaskToolInput {
    pub subagent_type: String,      // 子 Agent 类型
    pub description: String,        // 任务描述（3-5个词）
    pub prompt: String,             // 详细任务说明
}
```

### 4.2 使用示例

```rust
// 主 Agent 调用子 Agent
let task_input = TaskToolInput {
    subagent_type: "code-reviewer".to_string(),
    description: "review authentication code".to_string(),
    prompt: "Please review the authentication code in src/auth.rs. Focus on security issues and best practices.".to_string(),
};

let result = task_tool.execute(json!(task_input), context).await?;
```

### 4.3 返回格式

子 Agent 只返回相关发现：

```rust
pub struct TaskToolResult {
    pub findings: Vec<String>,      // 关键发现
    pub recommendations: Vec<String>, // 建议
    pub summary: String,            // 摘要
}
```

## 5. 协作模式

### 5.1 主从模式（Master-Worker）

主 Agent 负责任务分解和调度，子 Agent 执行具体任务：

```
┌──────────────┐
│  Main Agent  │
│  (Master)    │
└──────┬───────┘
       │
       │ Task 1
       ▼
┌──────────────┐
│ SubAgent 1   │────┐
│ (Worker)     │    │
└──────────────┘    │
                    │
       │ Task 2     │ Result 1
       ▼            │
┌──────────────┐    │
│ SubAgent 2   │────┤
│ (Worker)     │    │
└──────────────┘    │
                    │
       │ Task 3     │ Result 2
       ▼            │
┌──────────────┐    │
│ SubAgent 3   │────┘
│ (Worker)     │
└──────────────┘
```

**适用场景**：
- 代码审查 + 测试生成
- 文档生成 + 代码审查
- 多文件并行处理

### 5.2 对等模式（Peer-to-Peer）

多个 Agent 平等协作：

```
┌──────────────┐     ┌──────────────┐
│   Agent A    │◀───▶│   Agent B    │
│  (Reviewer)  │     │   (Tester)   │
└──────┬───────┘     └──────┬───────┘
       │                    │
       │                    │
       └────────┬───────────┘
                │
                ▼
         ┌──────────────┐
         │   Agent C    │
         │  (Integrator)│
         └──────────────┘
```

**适用场景**：
- 交叉审查
- 多视角分析
- 共识决策

### 5.3 层级模式（Hierarchical）

多层级 Agent 协作：

```
┌─────────────────────────────────────────┐
│           Orchestrator Agent            │
│         (任务编排和协调)                 │
└──────────────────┬──────────────────────┘
                   │
         ┌─────────┼─────────┐
         │         │         │
         ▼         ▼         ▼
┌────────────┐ ┌────────────┐ ┌────────────┐
│ Manager A  │ │ Manager B  │ │ Manager C  │
│(代码质量)  │ │(测试覆盖)  │ │(文档完整)  │
└─────┬──────┘ └─────┬──────┘ └─────┬──────┘
      │              │              │
   ┌──┴──┐        ┌──┴──┐        ┌──┴──┐
   │     │        │     │        │     │
   ▼     ▼        ▼     ▼        ▼     ▼
┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐
│ A1  │ │ A2  │ │ B1  │ │ B2  │ │ C1  │ │ C2  │
└─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘
```

**适用场景**：
- 大型项目分析
- 多阶段工作流
- 复杂任务分解

## 6. 任务分解

### 6.1 分解策略

**按功能分解**：

```markdown
任务：代码质量检查

分解：
1. 代码审查 Agent：检查代码风格和潜在bug
2. 测试生成 Agent：生成单元测试
3. 文档生成 Agent：更新文档
```

**按文件分解**：

```markdown
任务：项目重构

分解：
1. Agent 1：重构 src/auth/
2. Agent 2：重构 src/api/
3. Agent 3：重构 src/db/
```

**按阶段分解**：

```markdown
任务：功能开发

分解：
1. 设计 Agent：设计架构
2. 开发 Agent：实现功能
3. 测试 Agent：编写测试
4. 文档 Agent：编写文档
```

### 6.2 任务调度

```rust
// 并发调度多个子 Agent
let tasks = vec![
    TaskInput {
        subagent_type: "code-reviewer",
        description: "review auth module",
        prompt: "Review src/auth/",
    },
    TaskInput {
        subagent_type: "code-reviewer",
        description: "review api module",
        prompt: "Review src/api/",
    },
];

// 并发执行
let results = futures::future::join_all(
    tasks.into_iter().map(|task| {
        task_tool.execute(json!(task), context.clone())
    })
).await;
```

## 7. Agent通信

### 7.1 消息传递

```rust
// 主 Agent 发送任务
let message = TaskMessage {
    task_id: "task-123".to_string(),
    subagent_type: "code-reviewer".to_string(),
    payload: TaskPayload {
        description: "review code".to_string(),
        prompt: "Review src/main.rs".to_string(),
    },
};

// 子 Agent 返回结果
let response = TaskResponse {
    task_id: "task-123".to_string(),
    status: TaskStatus::Completed,
    result: TaskResult {
        findings: vec![
            "Potential null pointer dereference".to_string(),
            "Missing error handling".to_string(),
        ],
        recommendations: vec![
            "Add null check".to_string(),
            "Use Result type".to_string(),
        ],
    },
};
```

### 7.2 共享状态

```rust
// 使用共享状态
pub struct SharedState {
    pub project_context: Arc<RwLock<ProjectContext>>,
    pub findings: Arc<RwLock<Vec<Finding>>>,
    pub decisions: Arc<RwLock<Vec<Decision>>>,
}

// 子 Agent 读取和更新共享状态
let mut findings = shared_state.findings.write().await;
findings.push(Finding {
    source: "code-reviewer".to_string(),
    content: "Security issue found".to_string(),
});
```

## 8. 结果汇总

### 8.1 汇总策略

```rust
pub struct ResultAggregator {
    findings: Vec<Finding>,
    recommendations: Vec<Recommendation>,
    conflicts: Vec<Conflict>,
}

impl ResultAggregator {
    pub fn aggregate(&mut self, results: Vec<TaskResult>) -> AggregatedResult {
        // 1. 收集所有发现
        for result in results {
            self.findings.extend(result.findings);
            self.recommendations.extend(result.recommendations);
        }
        
        // 2. 检测冲突
        self.detect_conflicts();
        
        // 3. 优先级排序
        self.prioritize();
        
        // 4. 生成摘要
        self.generate_summary()
    }
}
```

### 8.2 冲突解决

```rust
pub enum ConflictResolution {
    FirstWins,      // 第一个结果优先
    LastWins,       // 最后一个结果优先
    Merge,          // 合并结果
    AskUser,        // 询问用户
}

impl ResultAggregator {
    fn resolve_conflict(&self, conflict: &Conflict) -> Resolution {
        match conflict.severity {
            Severity::Critical => ConflictResolution::AskUser.resolve(conflict),
            Severity::High => ConflictResolution::Merge.resolve(conflict),
            _ => ConflictResolution::LastWins.resolve(conflict),
        }
    }
}
```

## 9. 最佳实践

### 9.1 Agent 设计原则

1. **专业化**：每个子 Agent 应该专注于一个领域
2. **独立性**：子 Agent 应该能够独立完成任务
3. **可组合性**：子 Agent 应该可以灵活组合
4. **可观测性**：子 Agent 的执行过程应该可追踪

### 9.2 任务分解原则

1. **粒度适中**：任务不要太小也不要太大
2. **边界清晰**：任务之间应该有明确的边界
3. **依赖最小化**：减少任务之间的依赖
4. **可并行化**：尽可能设计可并行执行的任务

### 9.3 通信原则

1. **信息最小化**：只传递必要的信息
2. **格式标准化**：使用标准化的消息格式
3. **错误处理**：正确处理通信错误
4. **超时控制**：设置合理的超时时间

## 10. 示例场景

### 10.1 代码审查工作流

```yaml
# 配置
agents:
  security-reviewer:
    description: "安全审查 Agent"
    allowed_tools: [read, grep, glob]
  
  performance-reviewer:
    description: "性能审查 Agent"
    allowed_tools: [read, grep, glob]
  
  style-reviewer:
    description: "代码风格审查 Agent"
    allowed_tools: [read, grep, glob]
```

```markdown
# 工作流

1. 主 Agent 接收代码审查请求
2. 分解为三个并行任务：
   - 安全审查
   - 性能审查
   - 风格审查
3. 并发执行三个子 Agent
4. 汇总结果，检测冲突
5. 生成综合审查报告
```

### 10.2 测试生成工作流

```yaml
# 配置
agents:
  unit-test-generator:
    description: "单元测试生成 Agent"
    allowed_tools: [read, write, grep]
  
  integration-test-generator:
    description: "集成测试生成 Agent"
    allowed_tools: [read, write, grep]
  
  test-runner:
    description: "测试运行 Agent"
    allowed_tools: [read, exec]
```

```markdown
# 工作流

1. 主 Agent 接收测试生成请求
2. 顺序执行：
   a. 单元测试生成
   b. 集成测试生成
   c. 测试运行和验证
3. 汇总测试结果
4. 生成测试报告
```

## 11. 相关文档

- [Agent执行流程](../core-concepts/agent-execution.md)
- [工具系统](../core-concepts/tool-system.md)
- [配置说明](../configuration/CONFIGURATION.md#44-子-agent-配置)

## 12. 参考资料

- [Anthropic SubAgent Pattern](https://www.anthropic.com/engineering/building-agents-with-the-claude-agent-sdk)
- [OpenAI Agent Handoffs](https://platform.openai.com/docs/assistants)
- [CrewAI Multi-Agent](https://docs.crewai.com/)
