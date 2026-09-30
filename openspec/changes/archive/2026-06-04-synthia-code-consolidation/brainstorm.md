# Synthia 代码清理与合并 - Brainstorming Decision Log

**日期**: 2026-06-02
**状态**: 设计已批准

---

## 背景

Synthia 项目存在严重的代码重复问题：
- 3 套并行的 ReAct 实现（4 个来源）
- `AgentEvent` / `AgentConfig` / `SessionConfig` 等核心类型重复定义（2-5 处）
- `MemoryStore` trait 在同一 crate 内定义了 3 次
- `LoopDetector` 有 3 个独立实现
- Compaction 逻辑分散在 6+ 文件
- Checkpoint 在 agent 和 context 两处重复
- Sandbox 在 guardian 和 exec 两处重复
- 10 个手写 registry，core 已有泛型 `Registry<T>` 未被使用
- 6 个 crate 在磁盘上但未加入 `[workspace.members]`

---

## 决策链

### Q1: 孤儿 crate 处理策略

**问题**: 5 个 crate 存在于磁盘但未加入 workspace（`synthia-agent-core`、`synthia-react`、`synthia-so`、`synthia-guardian`、`synthia-model-router`、`synthia-tracing`）

**选项**:
1. 直接删除 — 废弃代码直接移除
2. 评估后决定 — 先检查功能重叠再决定
3. 保留作为 fallback

**决策**: 选项 2 — 逐个评估后再决定去留
**理由**: 需要先确认是否有独特功能后再处理

---

### Q2: ReAct 实现合并策略

**问题**: 4 个 ReAct 实现来源（`synthia-agent/src/react.rs` 1179行, `synthia-agent/src/agent/react.rs` 725行, `synthia-agent-core`, `synthia-react`）

**选项**:
1. 以顶层 `react.rs` 为主 — 最大的实现
2. 以 `agent/react.rs` 为主 — 更结构化
3. 用孤儿 crates 重写 — 看起来更干净的重构设计
4. 暂不合并 — 留到架构评审后

**决策**: 选项 2 — 以 `agent/react.rs` 为核心实现
**理由**: `agent/react.rs` 更结构化，适合作为核心；顶层 1179 行中独有功能整合进去

---

### Q3: AgentEvent 合并策略

**问题**: 3 个 `AgentEvent` 定义（`events.rs` 355行, `types/event.rs` 482行, `synthia-agent-core`）

**选项**:
1. 以 `types/event.rs` 为主 — 最完整
2. 以 `events.rs` 为主 — 结构化版本
3. 评估后按功能拆分

**决策**: 选项 1 — 以 `types/event.rs` 的 `AgentEvent` 为主
**理由**: 482行最完整，作为权威版本

---

### Q4: AgentConfig 合并策略

**问题**: 5 个 `AgentConfig` 定义分散在不同 crate

**选项**:
1. 统一到 `agent_config.rs`（396行）
2. 统一到服务端配置
3. 按层级分离（CLI → Server → Runtime）

**决策**: 选项 3 — 按层级分离
**理由**: 各层职责不同，通过 `From`/`Into` 转换更清晰

---

### Q5: MemoryStore trait 合并策略

**问题**: 同一 crate 内 `MemoryStore` 定义了 3 次

**选项**:
1. 统一到 `types.rs`
2. 统一到 `cold/store.rs`
3. 读写分离但接口统一

**决策**: 选项 3 — 读写分离
**理由**: `file_store.rs` 负责读，`cold/store.rs` 负责写，通过子 trait 统一访问

---

### Q6: LoopDetector 合并策略

**问题**: 3 个 LoopDetector 实现分散在 agent 和 guardian

**选项**:
1. 以 `agent/loop_detector.rs` 为主
2. 提取到 `synthia-guardian`
3. 各子系统独立但接口统一

**决策**: 选项 1 — 以 `agent/loop_detector.rs` 为主
**理由**: agent 是主要消费者，其他通过委托调用

---

### Q7: Compaction 合并策略

**问题**: compaction 逻辑分散在 6+ 文件

**选项**:
1. 统一到 `synthia-context/compaction/`
2. 统一到 `synthia-agent`
3. error_recovery 特殊逻辑保留

**决策**: 选项 1 — 统一到 context
**理由**: context 是上下文管理的老巢，compaction 作为 context 服务的一部分

---

### Q8: Checkpoint 合并策略

**问题**: checkpoint 在 agent（678行）和 context（372行）两处重复

**选项**:
1. 统一到 `synthia-context/checkpoint.rs`
2. 统一到 `synthia-agent/checkpoint.rs`
3. agent 负责快照生成，context 负责持久化

**决策**: 选项 1 — 统一到 context
**理由**: context 是状态管理的核心

---

### Q9: Sandbox 合并策略

**问题**: sandbox 在 `synthia-guardian` 和 `synthia-exec` 两处重复

**选项**:
1. 删除 exec 的，用 guardian 的
2. 删除 guardian 的，用 exec 的
3. 合并到一个 crate

**决策**: 选项 2 — 以 exec 为主，guardian 只做策略检查
**理由**: exec 是实际的沙箱消费者

---

### Q10: Registry 合并策略

**问题**: 10 个手写 registry，core 已有 `Registry<T>`

**选项**:
1. 直接替换为 core 的
2. 逐步迁移
3. 暂不处理

**用户补充**: core 模块中已经有 `Registry<T>` 的定义

**决策**: 选项 1 — 直接替换
**理由**: core 已有定义，直接替换最直接

---

## 设计决策总结

| 合并项 | 决策 | 主实现位置 |
|--------|------|------------|
| 孤儿 crates | 评估后决定 | — |
| ReAct | 以 `agent/react.rs` 为主 | `synthia-agent/src/agent/react.rs` |
| AgentEvent | 以 `types/event.rs` 为主 | `synthia-agent/src/types/event.rs` |
| AgentConfig | 按层级分离 | CLI → Server → Runtime |
| MemoryStore | 读写分离 | `synthia-memory/src/types.rs` |
| LoopDetector | 以 agent 为主 | `synthia-agent/src/agent/loop_detector.rs` |
| Compaction | 统一到 context | `synthia-context/src/compaction/` |
| Checkpoint | 统一到 context | `synthia-context/src/checkpoint.rs` |
| Sandbox | 以 exec 为主 | `synthia-exec/src/sandbox.rs` |
| Registry | 直接替换 | `core::Registry<T>` |