## ADDED Requirements

### Requirement: Workflow struct
Workflow 组合多个 AgentHandle 按序执行：
- stages: Vec<Arc<AgentHandle>> — 阶段列表

### Requirement: Workflow.run
run(input) 语义：
1. current = input
2. for stage in stages:
   a. tool = agent_as_tool(stage)
   b. output = tool.call(current)
   c. current = output.text()
3. Ok(current)

### Requirement: Workflow supports mixed agents
stages 中可混合本地 AgentHandle 和远程 agent（SendMessage Tool）。
