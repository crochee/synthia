## Context

Synthia 是一个模块化的 Rust AI agent 框架，基于 ReAct（Reasoning + Acting）模式。当前项目存在严重的代码重复和架构混乱问题，导致维护困难、编译依赖混乱。

**问题根源**:
- 开发过程中多次尝试重构但未完成，形成多套并行的实现
- 孤儿 crate（未加入 workspace）积累了废弃代码
- 缺乏统一的类型定义规范，导致核心类型重复定义
- 职责边界不清晰，相似功能分散在多个 crate

**约束条件**:
- 必须保证功能完整性，清理过程不能破坏现有功能
- 需要更新 Cargo.toml 依赖图
- 测试必须持续通过
- 不改变外部 API

## Goals / Non-Goals

**Goals:**
- 消除 3 套 ReAct 实现合并为 1
- 统一 AgentEvent/AgentConfig 等核心类型定义
- 将 compaction/checkpoint 逻辑统一到 context crate
- 以 exec sandbox 为主，guardian 只做策略
- 用 core::Registry<T> 替换 10 个手写 registry
- 清理 6 个未加入 workspace 的孤儿 crate
- 保持所有测试通过

**Non-Goals:**
- 不重构 synthia-web 前端代码
- 不改变 CLI 业务逻辑（配置转换以外）
- 不修改 MCP/tool/skill 等功能性 crate 的内部实现
- 不引入新的外部依赖

## Decisions

### D1: ReAct 实现以 agent/react.rs 为核心

- **選擇**: 以 `synthia-agent/src/agent/react.rs`（725行）为权威版本
- **理由**: `agent/react.rs` 采用更结构化的子模块组织方式，适合作为核心；顶层 `react.rs`（1179行）虽然更大，但包含较多未分离的逻辑
- **已考慮 alternative**: 以顶层 `react.rs` 为主 → 拒绝：逻辑过于集中，耦合度高；以孤儿 `synthia-react` 重写 → 拒绝：功能未经验证，且已有稳定实现

### D2: AgentEvent 以 types/event.rs 为权威

- **選擇**: 以 `synthia-agent/src/types/event.rs`（482行）为主
- **理由**: 482行是最完整的定义，包含事件变化（AgentEvent variants）的完整枚举
- **已考慮 alternative**: 以 `events.rs` 为主 → 拒绝：355行不够完整，且 `events.rs` 的职责更偏向事件广播而非类型定义

### D3: AgentConfig 按层级分离

- **選擇**: CLI Config → Server Config → Agent Runtime Config，通过 `From`/`Into` 转换
- **理由**: 各层职责不同：CLI 负责 YAML 解析验证，Server 负责服务级配置合并，Agent Runtime 是最终执行上下文。强统一会导致不必要的耦合
- **已考慮 alternative**: 统一到 `agent_config.rs` → 拒绝：CLI 和 Server 的配置结构与运行时配置需求不一致

### D4: MemoryStore 读写分离

- **選擇**: `types.rs` 定义 trait 子类型，`file_store.rs` 实现读，`cold/store.rs` 实现写
- **理由**: 读操作（搜索、检索）和写操作（记录、压缩）确实是不同的操作模式，分离后各自优化空间更大
- **已考慮 alternative**: 统一到 `types.rs` → 拒绝：读写实现差异大，强制统一会导致代码不清晰

### D5: LoopDetector 以 agent 为主

- **選擇**: `synthia-agent/src/agent/loop_detector.rs` 作为主实现，其他委托调用
- **理由**: agent 是循环检测的主要消费者，其他子系统（guardian、stream_builder）的循环检测需求可以通过 trait 委托给 agent 的实现
- **已考慮 alternative**: 提取到 guardian → 拒绝：guardian 专注安全策略，不应承担循环检测职责

### D6: Compaction 统一到 context crate

- **選擇**: 统一到 `synthia-context/src/compaction/`
- **理由**: context 是上下文管理的老巢，compaction 作为上下文服务的一部分放在 context 最合理；agent 和 memory 的 compaction 都调用 context 的实现
- **已考慮 alternative**: 统一到 agent → 拒绝：context 是真正的上下文存储者，compaction 应该围绕它设计

### D7: Checkpoint 统一到 context crate

- **選擇**: 统一到 `synthia-context/src/checkpoint.rs`
- **理由**: context 是状态管理的核心，checkpoint 作为快照机制放在 context 更符合单一职责原则；agent 负责快照生成请求，context 负责持久化
- **已考慮 alternative**: 统一到 agent → 拒绝：agent 是快照的消费者而非存储者

### D8: Sandbox 以 exec 为主

- **選擇**: `synthia-exec/src/sandbox.rs` 作为主实现，guardian 只做策略检查
- **理由**: exec 是沙箱的实际执行者，guardian 应该专注于安全策略和审批，不自己实现 sandbox
- **已考慮 alternative**: 以 guardian 为主 → 拒绝：guardian 的设计更适合策略层而非执行层

### D9: Registry 直接替换为 core 实现

- **選擇**: 其他 crate 的 registry 直接改用 `core::Registry<T>`
- **理由**: core 已有泛型 `Registry<T>` 实现，直接替换最简单；若业务逻辑有额外行为，通过组合方式扩展
- **已考慮 alternative**: 逐步迁移 → 拒绝：一次性替换更干净，且已有明确的 core 实现

### D10: 孤儿 crate 逐个评估

- **選擇**: 逐个读取源码，对比功能后决定删除或迁移
- **理由**: 部分孤儿 crate 可能包含尚未迁移到主实现的独特功能，需要先评估
- **评估原则**: 若功能已被主实现覆盖 → 删除；若包含独特功能 → 迁移或保留

## Risks / Trade-offs

[Risk] 合并过程中引入 bug → Mitigation: 每次合并后运行测试，验证功能完整性
[Risk] 删除孤儿 crate 后发现仍有依赖 → Mitigation: 先评估依赖关系，必要时通过 cargo build 验证编译
[Risk] Registry 替换后发现 API 不完全兼容 → Mitigation: 通过组合 `pub struct XxxRegistry { inner: Registry<T> }` 方式扩展

[Trade-off] 清理范围大，一次性完成风险高 → 接受理由: 问题已经过充分探索，设计决策明确，可以系统性地分 phase 执行

## Migration Plan

**Phase 1: 评估孤儿 Crates**
1. 逐个读取孤儿 crate 源码
2. 与主实现对比功能
3. 决定：删除 / 迁移 / 保留

**Phase 2: 核心类型统一**
1. 选定 `types/event.rs` 为权威版本
2. 删除 `events.rs` 中的重复
3. 更新所有引用

**Phase 3: ReAct 实现整合**
1. 以 `agent/react.rs` 为核心
2. 整合顶层 `react.rs` 独有功能
3. 验证后删除顶层版本

**Phase 4: 跨 Crate 依赖清理**
1. 删除孤儿 crate 中的重复类型
2. 将 guardian sandbox 改为委托给 exec
3. 将 compaction 迁移到 context crate

**Phase 5: Registry 统一**
1. 确认 `core::Registry<T>` API 覆盖所有场景
2. 替换 10 个手写 registry
3. 删除废弃文件

**Rollback**: 若任何 phase 失败，通过 git 恢复修改的单个文件即可回滚

## Open Questions

- 孤儿 `synthia-model-router` 的路由逻辑是否已在其他地方实现？
- `synthia-tracing` 与现有 tracing 实现的具体差异是什么？是否包含独特的 trace 处理逻辑？
- `synthia-so` 除了被 `synthia-react` 依赖外是否还有其他消费者？