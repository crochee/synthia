## Context

synthia-agent 是 Rust 实现的多能力 AI Agent 框架。当前主循环实现存在以下问题：

1. **单函数膨胀**：`stream_builder/mod.rs` 中 `build_stream()` 约 1100 行，处理循环检测、Circuit Breaker、Token 预算、Self-reflection、Hook 调用、工具执行等所有逻辑

2. **代码重复**：
   - `react.rs` 的 `ReActLoop` 有独立的 self-reflection 实现，但从未被主循环调用
   - `legacy.rs` 内联了 self-reflection 逻辑
   - `VecMessageReader` 在多处重复定义

3. **架构不清晰**：
   - `builder.rs` 的 `StreamBuilder` 基本是空壳
   - `LoopContext` 使用不统一
   - 步骤之间耦合度高

4. **多Agent协作缺失**：MessageProxy 已实现 gRPC UDS 通信，但缺少统一的 AgentBus 抽象

当前用户确认：采用方案 B（一步到位重写），不要 feature flag，Self-reflection 在主循环结束后执行，AgentBus 作为独立通信层（支持内存/文件等多种实现）。

## Goals / Non-Goals

**Goals:**
- 统一主循环到 StreamBuilder + LoopContext 模式
- Self-reflection 移到主循环结束后执行
- 新增 AgentBus trait 作为多Agent通信抽象层
- Self-reflection 结果存入 HotMemory
- 保留 legacy.rs 作为备份验证

**Non-Goals:**
- 不做渐进式 feature flag 迁移
- 不改变现有公开 API（向后兼容）
- 不实现跨机器的网络Agent通信（文件实现仅支持本机进程间）

## Decisions

### D1: 主循环架构 - StreamBuilder + LoopContext

**選擇**：基于 Builder 模式重构主循环，将步骤拆分为独立模块

**理由**：
- 单一职责：各步骤（sample/tool_execute/compact/reflect）独立实现
- 可测试性：每个步骤可单独单元测试
- 可维护性：职责清晰，便于定位和修改问题

**已考慮 alternatives**:
- 方案 A（渐进式）：风险低但维护两套代码，增加复杂度
- 方案 C（最小改动）：未解决架构问题，只是表面修缮

### D2: Self-reflection 时机 - 主循环结束后

**選擇**：主循环正常结束后（Completed 且 iteration > 0）执行一次 self-reflection

**理由**：
- 用户明确设计意图是在主循环完成后做
- 避免主循环内重复执行造成的上下文碎片化
- 基于完整会话历史生成更有价值的 reflection

**已考慮 alternatives**:
- 每 N 轮执行：增加主循环复杂度，且 reflection 碎片化
- 每次工具调用后：过于频繁，无实际价值

### D3: AgentBus trait 设计 - 泛型抽象

**選擇**：
```rust
pub trait AgentBus: Send + Sync {
    async fn register(&self, agent_id: &str) -> Result<(), BusError>;
    async fn send(&self, to: &str, payload: Vec<u8>) -> Result<(), BusError>;
    async fn broadcast(&self, recipients: &[&str], payload: Vec<u8>) -> Result<usize, BusError>;
    fn subscribe(&self) -> impl Stream<Item = BusMessage>;
}
```

**理由**：
- 最小接口：覆盖所有必需操作
- 泛型后端：Memory（进程内）、File（跨进程）、MessageProxy（现有 gRPC）
- 易于测试：可注入 mock 实现

**已考慮 alternatives**:
- 细分 trait（RegisterBus/SendBus/BroadcastBus）：过度设计
- 同步接口：与异步架构不符

### D4: 实现优先级

**選擇**：
1. 第一阶段：统一主循环 + Self-reflection 后置
2. 第二阶段：AgentBus trait + MemoryAgentBus
3. 第三阶段：FileAgentBus + MessageProxy 适配器

**理由**：主循环是核心功能，先稳定后再扩展通信能力

### D5: Self-reflection 存储 - HotMemory

**選擇**：reflection 结果存入 HotMemory，key 格式为 `reflection/{session_id}/{iteration}`

**理由**：
- HotMemory 是快速访问层，适合运行时参考
- Session 级别的组织便于检索
- 与现有 memory 系统集成良好

**已考慮 alternatives**:
- EpisodicMemory：更适合长期记忆，reflection 偏向即时参考
- ColdStorage：访问速度慢，不适合运行时

## Risks / Trade-offs

[Risk] 一步重写风险高 → Mitigation: 保留 legacy.rs 作为备份，新架构验证通过后再删除

[Risk] Self-reflection 后置可能影响迭代内的问题发现 → Mitigation: 主循环内仍保留基本的错误处理和循环检测，reflection 是补充能力

[Risk] AgentBus 抽象可能过度设计 → Mitigation: 先实现最小接口（register/send/broadcast/subscribe），根据实际使用扩展

[Trade-off] 新增文件结构增加学习成本 → 接受理由：清晰的架构比快速上手更重要，且有文档和测试辅助

## Migration Plan

1. **第一阶段：主循环重构**
   - 创建 `stream_builder/steps/` 目录
   - 实现 `steps/sample.rs`、`steps/tool_execute.rs`、`steps/compact.rs`、`steps/reflect.rs`
   - 重构 `mod.rs` 使用新的步骤组件
   - 保留 `legacy.rs`
   - 运行现有测试验证
   - 替换 `legacy.rs` 调用

2. **第二阶段：Self-reflection 后置**
   - 在 `StreamBuilder::run()` 末尾添加 `step_self_reflection()`
   - 集成 HotMemory 存储
   - 测试 reflection 生成和存储

3. **第三阶段：AgentBus 抽象**
   - 创建 `agent_bus/mod.rs` 定义 trait
   - 实现 `MemoryAgentBus`（进程内共享）
   - 实现 `FileAgentBus`（基于文件系统）
   - 适配现有 `MessageProxy`

4. **验证与清理**
   - 运行完整测试套件
   - 删除 `legacy.rs`（确认稳定后）
   - 更新文档

## Open Questions

1. MemoryAgentBus 的进程内共享方式？使用 `Arc<RwLock<HashMap>>` 还是 channel-based？
2. FileAgentBus 的文件路径约定？是否需要配置？
3. Self-reflection 生成失败时的 fallback 行为？
4. AgentBus 消息的 payload 格式？是否需要序列化约定？