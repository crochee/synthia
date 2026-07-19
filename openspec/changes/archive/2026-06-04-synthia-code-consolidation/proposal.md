## Why

Synthia 项目存在严重的代码重复和架构混乱：3 套并行的 ReAct 实现、多个核心类型重复定义（AgentEvent/AgentConfig 各 3-5 处）、compaction/checkpoint/sandbox 等逻辑分散在 6+ 文件、10 个手写 registry 而 core 已有泛型实现未使用、6 个 crate 在磁盘但未加入 workspace。这导致维护困难、编译依赖混乱、功能边界不清晰。现在清理和合并，为后续生产级 agent 开发打下基础。

## What Changes

**AgentEvent 重复定义**
- From: 3 个独立的 AgentEvent 定义（events.rs 355行, types/event.rs 482行, synthia-agent-core）
- To: 统一到 types/event.rs 作为唯一权威版本
- Reason: 多处定义导致类型不兼容和维护困难
- Impact: 非破坏性，agent 内部调整

**ReAct 实现重复**
- From: 4 个 ReAct 来源（agent/src/react.rs 1179行, agent/src/agent/react.rs 725行, synthia-agent-core, synthia-react）
- To: 以 agent/src/agent/react.rs（结构化）为核心，整合顶层 react.rs 独有功能
- Reason: agent/react.rs 更模块化，适合作为核心
- Impact: 非破坏性，功能整合

**AgentConfig 分散**
- From: 5 个 AgentConfig 定义分散在各 crate
- To: 按层级分离（CLI → Server → Runtime），通过 From/Into 转换
- Reason: 各层职责不同，强统一反而不灵活
- Impact: 非破坏性，配置转换层新增

**MemoryStore trait 重复**
- From: 同一 crate 内定义了 3 次 MemoryStore
- To: types.rs 定义 trait 子类型，file_store.rs 实现读，cold/store.rs 实现写
- Reason: 读写操作确实不同，分离更清晰
- Impact: 非破坏性，内部重构

**LoopDetector 3 处实现**
- From: agent/loop_detector.rs、stream_builder/loop_detection.rs、guardian/loop_detector.rs
- To: 以 agent/loop_detector.rs 为主，其他委托调用
- Reason: agent 是主要消费者
- Impact: 非破坏性，guardian 改为委托

**Compaction 逻辑分散**
- From: 6+ 文件分散在 agent 和 context
- To: 统一到 synthia-context/src/compaction/
- Reason: context 是上下文管理的老巢
- Impact: 非破坏性，逻辑迁移

**Checkpoint 重复**
- From: agent/checkpoint.rs（678行）和 context/checkpoint.rs（372行）
- To: 统一到 synthia-context/checkpoint.rs
- Reason: context 是状态管理核心
- Impact: 非破坏性，agent 改为使用 context 的实现

**Sandbox 重复**
- From: synthia-guardian/src/sandbox.rs 和 synthia-exec/src/sandbox.rs
- To: exec 为主实现，guardian 只做策略检查
- Reason: exec 是沙箱消费者，guardian 应专注策略
- Impact: 非破坏性，guardian 改为委托

**Registry 手写实现**
- From: 10 个 crate 各自手写 registry
- To: 直接替换为 core::Registry<T>
- Reason: core 已有泛型实现，未被使用
- Impact: 非破坏性，接口统一

**孤儿 crate 未加入 workspace**
- From: 6 个 crate 在磁盘但不在 [workspace.members]
- To: 逐个评估后决定删除或迁移
- Reason: 混乱的 crate 状态需要清理
- Impact: 可能删除 dead code

## Capabilities

### New Capabilities
- `code-consolidation`: 统一的代码清理框架，包含所有合并决策的执行计划

### Modified Capabilities
- `agent-runtime`: ReAct 实现合并后，agent 运行时行为不变但内部结构更清晰
- `event-types`: AgentEvent 统一后，事件类型兼容性提升
- `config-management`: AgentConfig 分层后，配置管理更清晰
- `memory-store`: MemoryStore trait 重构后，读写分离更明确
- `loop-detection`: LoopDetector 统一后，循环检测逻辑集中
- `compaction-service`: Compaction 统一到 context 后，上下文压缩服务更集中
- `checkpoint-service`: Checkpoint 统一后，状态快照服务更集中
- `sandbox-security`: Sandbox 以 exec 为主后，安全策略和执行职责分离
- `registry-pattern`: Registry 统一到 core 后，所有 crate 使用相同的注册表模式

## Impact

- **编译依赖**: Cargo.toml workspace members 需要更新
- **代码结构**: 多个 crate 内部文件删除/迁移
- **测试**: 现有测试需要验证功能不受影响
- **无 API 变更**: 纯内部重构，不影响外部 API