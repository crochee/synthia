## Why

当前 synthia-agent 的主循环实现存在架构问题：~1100 行的 `build_stream()` 单函数处理所有逻辑（循环检测、Circuit Breaker、Token预算、Self-reflection、Hook调用、工具执行），难以维护和测试。多处代码重复（ReActLoop vs legacy self-reflection、VecMessageReader 等），且缺少生产级多Agent协作的抽象层。统一主循环架构并引入 AgentBus 抽象是解决这些问题的必要步骤。

## What Changes

**StreamBuilder 主循环统一**
- From: `legacy.rs` 中 ~1100 行单函数 `build_stream()` 处理所有循环逻辑
- To: 基于 `StreamBuilder` + `LoopContext` 的清晰分层架构，`steps/` 目录下各步骤独立实现
- Reason: 单一职责、可测试性、便于维护
- Impact: non-breaking，legacy.rs 保留作为备份

**Self-reflection 后置**
- From: 主循环内每5轮迭代执行一次 self-reflection
- To: 主循环结束后统一执行 self-reflection，结果存入 HotMemory
- Reason: 设计意图是主循环完成后做 reflection
- Impact: non-breaking，行为变化但兼容现有接口

**AgentBus 多Agent通信抽象**
- From: 无统一的 Agent 间通信抽象
- To: 新增 `AgentBus` trait，支持内存/文件/MessageProxy 等多种实现
- Reason: 单Agent场景下也需要多agent协作能力
- Impact: non-breaking，新增 API

**Modified Capabilities**
- `agent-loop`: 主循环架构重构，Self-reflection 时机调整

## Capabilities

### New Capabilities

- `stream-builder-v2`: 基于 StreamBuilder + LoopContext 的新一代主循环实现，包含步骤拆分（sample/tool_execute/compact/reflect）和清晰的分层架构
- `agent-bus`: Agent 间通信抽象层，支持 register/send/broadcast/subscribe 接口，以及内存、文件、MessageProxy 等多种后端实现
- `self-reflection-hotmemory`: 主循环结束后的 Self-reflection 结果存储到 HotMemory

### Modified Capabilities

- `agent-loop`: 现有 agent-loop 的行为变更：Self-reflection 从每5轮移到主循环结束后执行

## Impact

**Affected Code:**
- `crates/synthia-agent/src/stream_builder/mod.rs` - 重构为主循环入口
- `crates/synthia-agent/src/stream_builder/legacy.rs` - 保留备份
- `crates/synthia-agent/src/react.rs` - 简化，移除内联 self-reflection
- `crates/synthia-agent/src/loop_context.rs` - 扩展支持新架构

**New Files:**
- `crates/synthia-agent/src/stream_builder/steps/` - 步骤拆分目录
- `crates/synthia-agent/src/agent_bus/` - AgentBus trait 及实现

**Dependencies:**
- `synthia-memory` (HotMemory)
- `synthia-message-proxy` (MessageProxy 适配器)

**No Breaking Changes:** 现有 API 保持兼容，legacy.rs 作为备份保留