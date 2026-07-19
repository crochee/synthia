# Agent Loop Refactor - Brainstorming Output

## 背景

分析 synthia-agent 当前实现与生产级 AI Agent 的差距：
- `stream_builder/mod.rs` 的 `build_stream()` ~1100 行单函数处理所有逻辑
- `react.rs` 中的 `ReActLoop` 有独立的 self-reflection，但从未被主循环调用
- 多处重复逻辑：会话恢复、self-reflection、VecMessageReader
- 缺少多Agent协作的抽象层

## 用户确认的决策

### Q1: 迁移策略
- **选择**：方案 B - 一步到位重写
- **理由**：生产环境需要清晰的架构，不要 feature flag 式的渐进迁移

### Q2: Self-reflection 时机
- **选择**：主循环结束后执行
- **理由**：设计意图是在主循环完成后做的

### Q3: AgentBus 集成方式
- **选择**：独立的 Agent 间通信层
- **要求**：泛型抽象，支持各种实现方式（内存、文件等）
- **现状**：MessageProxy 已实现 gRPC UDS 通信

### Q4: 优先级
- **选择**：单Agent稳定与多Agent能力同等优先级

## 设计方案

### 架构目标

```
stream_builder/
├── legacy.rs           # 保留旧实现作为备份
├── mod.rs              # 新主循环（替换 ~1100 行单函数）
├── builder.rs          # StreamBuilder 填充
├── context_builder.rs
├── hook_builder.rs
├── loop_detection.rs
├── steps/              # 步骤拆分
│   ├── mod.rs
│   ├── sample.rs       # LLM 调用
│   ├── tool_execute.rs # 工具执行
│   ├── compact.rs      # 压缩
│   └── reflect.rs      # Self-reflection
└── agent_bus/          # 新增：多Agent通信抽象
    ├── mod.rs          # AgentBus trait
    ├── memory.rs       # 内存实现
    ├── file.rs         # 文件实现
    └── proxy.rs        # MessageProxy 适配器
```

### 核心设计决策

1. **AgentBus trait**:
   - `register(agent_id)` - 注册当前 Agent
   - `send(to, payload)` - 单播
   - `broadcast(recipients, payload)` - 广播
   - `subscribe()` - 订阅消息流

2. **Self-reflection 后置**:
   - 主循环结束（Completed 且 iteration > 0）后执行
   - 结果存入 HotMemory

3. **实现优先级**:
   - 第一阶段：统一主循环，Self-reflection 后置
   - 第二阶段：AgentBus trait + MemoryAgentBus
   - 第三阶段：FileAgentBus + MessageProxy 适配器

### Self-reflection 存入 HotMemory

```rust
// 主循环结束后
let reflection = self.step_self_reflection(&ctx).await?;
self.memory_bridge.store_reflection(reflection).await;

// HotMemory key: "reflection/{session_id}/{iteration}"
```

### 向后兼容

- 保留 `legacy.rs` 作为备份
- 旧测试不受影响
- 新架构验证通过后再删除 legacy

## 关键问题与回答

1. **builder.rs 和 stream_builder/mod.rs 的关系？**
   - 是的，准备迁移到 builder 模式

2. **legacy.rs 的 build_stream 是唯一主循环吗？**
   - 是的，但需要重构为更清晰的架构

3. **self-reflection 设计意图？**
   - 主循环完成后做

4. **多Agent协作需求？**
   - 单agent场景下也需要多agent协作，通过 MessageProxy 模块的 trait 实现

5. **生产要求？**
   - 是的，需要稳定可用的实现