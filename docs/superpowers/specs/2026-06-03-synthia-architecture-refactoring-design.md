# Synthia 架构重构设计文档

**日期**: 2026-06-03
**状态**: 已批准
**版本**: 1.0

---

## 1. 背景与目标

### 1.1 当前问题

```
问题本质: 职责分散 + 重复实现 + 缺少分层抽象

当前架构问题:
┌─────────────────────────────────────────────────────────────┐
│                     synthia-agent                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ stream_     │  │ legacy::    │  │ loop_detection.rs   │ │
│  │ builder/    │  │ build_stream│  │ LoopDetectorSet     │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ checkpoint  │  │ react.rs    │  │ (embedded CB)       │ │
│  │ .rs         │  │ ReActLoop   │  └─────────────────────┘ │
│  └─────────────┘  └─────────────┘                          │
└─────────────────────────────────────────────────────────────┘
         │                 │                    │
         ▼                 ▼                    ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐
│ synthia-    │  │ synthia-    │  │ synthia-guardian        │
│ session/    │  │ context/    │  │ loop_detector.rs        │
│ store.rs    │  │ assembler   │  │ circuit_breaker.rs      │
└─────────────┘  └─────────────┘  └─────────────────────────┘
         │                 │                    │
         ▼                 ▼                    ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐
│ Session     │  │ Compaction │  │ LoopDetector (again!)   │
│ FileStore   │  │ + Pruning  │  │ CircuitBreaker (again!) │
└─────────────┘  └─────────────┘  └─────────────────────────┘
```

### 1.2 设计目标

1. **单一职责**: 每个模块只做一件事
2. **统一抽象**: 相同功能只有一个实现
3. **清晰边界**: 层与层之间通过接口通信
4. **可测试性**: 核心逻辑可独立测试
5. **可观测性**: 统一 observability 接口

---

## 2. 目标架构

### 2.1 整体分层

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Application Layer                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │
│  │  CLI Interface  │  │  Server (HTTP)  │  │  Programmatic API   │  │
│  └────────┬────────┘  └────────┬────────┘  └──────────┬──────────┘  │
└───────────┼────────────────────┼────────────────────┼───────────────┘
            │                    │                    │
            ▼                    ▼                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         Agent Layer                                  │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    Agent Orchestrator                        │   │
│  │  - Session lifecycle                                         │   │
│  │  - ReAct loop coordination                                   │   │
│  │  - Event emission                                            │   │
│  └─────────────────────────────────────────────────────────────┘   │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────────┐      │
│  │ BuildExecutor  │  │ PlanExecutor   │  │ GeneralExecutor    │      │
│  │ (full access)  │  │ (read-only)    │  │ (sub-agent)        │      │
│  └────────────────┘  └────────────────┘  └────────────────────┘      │
└─────────────────────────────────────────────────────────────────────┘
            │                    │                    │
            ▼                    ▼                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     Infrastructure Layer                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────┐ │
│  │ Persistence  │  │   Context   │  │   Memory    │  │  Hook    │ │
│  │   Service    │  │   Service   │  │   Service   │  │ Registry │ │
│  │  (统一存储)   │  │ (统一组装)   │  │ (统一记忆)  │  │ (统一)   │ │
│  └──────────────┘  └──────────────┘  └──────────────┘  └──────────┘ │
└─────────────────────────────────────────────────────────────────────┘
            │                    │                    │
            ▼                    ▼                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       Shared Components                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │    Tool      │  │   Provider   │  │ Observability│              │
│  │  Registry    │  │   Registry   │  │   Bridge     │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 统一基础设施服务

**Persistence Service**

```rust
trait PersistenceService {
    async fn save_session(&self, session: &Session) -> Result<()>;
    async fn load_session(&self, id: &str) -> Result<Option<Session>>;
    async fn append_message(&self, session_id: &str, msg: &Message) -> Result<()>;
    async fn load_messages(&self, session_id: &str, range: MessageRange) -> Result<Vec<Message>>;
    async fn save_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()>;
    async fn load_checkpoint(&self, session_id: &str) -> Result<Option<CheckpointData>>;
}
```

**Context Service**

```rust
trait ContextService {
    fn assemble(&self, request: &ContextRequest) -> ContextResult;
    fn compact(&self, messages: &mut Vec<Message>, budget: &TokenBudget) -> CompactionResult;
    fn protect(&self, messages: &mut Vec<Message>, zone: &ProtectionZone);
}
```

**Memory Service**

```rust
trait MemoryService {
    async fn store(&self, event: MemoryEvent) -> Result<()>;
    async fn retrieve(&self, query: &MemoryQuery) -> Result<Vec<Memory>>;
    async fn consolidate(&self) -> Result<()>;
}
```

---

## 3. Crate 结构

### 3.1 最终 crate 划分

```
synthia/
├── crates/
│   ├── synthia-core/           # 公共工具 (ID, time, path, error, registry)
│   ├── synthia-provider/      # LLM 抽象层
│   ├── synthia-model-router/  # 模型路由
│   ├── synthia-permission/     # [合并] guardian + permission
│   ├── synthia-tool/           # 工具执行
│   ├── synthia-context/        # 上下文管理
│   ├── synthia-memory/         # 记忆系统
│   ├── synthia-session/        # 会话管理
│   ├── synthia-hook/           # Hook 系统
│   ├── synthia-agent/          # Agent 核心
│   ├── synthia-mcp/            # MCP 协议支持
│   ├── synthia-command/         # 命令系统
│   ├── synthia-task/           # 任务调度
│   ├── synthia-telemetry/      # 可观测性
│   ├── synthia-cli/            # CLI
│   └── synthia-server/         # HTTP Server
```

### 3.2 删除的文件/模块

| 文件/模块 | 原因 |
|-----------|------|
| `synthia-session/src/file_store.rs` | 功能重复，由 Store 统一替代 |
| `synthia-agent/src/stream_builder/legacy.rs` | 内联逻辑迁移到 services |
| `synthia-agent/src/stream_builder/loop_detection.rs` | LoopDetector 移至 synthia-permission |
| `synthia-agent/src/compaction.rs` | 重复，调用 context service |
| `synthia-agent/src/checkpoint.rs` | 合并到 session service |
| `synthia-guardian/` (整个 crate) | 合并到 synthia-permission |

---

## 4. 关键设计决策

### 4.1 依赖注入方式: Builder Pattern

```rust
pub struct AgentDependencies {
    circuit_breaker: Option<Arc<dyn CircuitBreaker>>,
    loop_detector: Option<Arc<dyn LoopDetector>>,
    tool_registry: Option<Arc<ToolRegistry>>,
    hook_registry: Option<Arc<HookRegistry>>,
    context_service: Option<Arc<dyn ContextService>>,
    persistence: Option<Arc<dyn PersistenceService>>,
    memory_service: Option<Arc<dyn MemoryService>>,
}

impl AgentDependencies {
    pub fn build(self) -> Agent {
        Agent {
            circuit_breaker: self.circuit_breaker.unwrap_or_else(|| Arc::new(DefaultCircuitBreaker::new())),
            loop_detector: self.loop_detector.unwrap_or_else(|| Arc::new(DefaultLoopDetector::new())),
            // ...
        }
    }
}
```

### 4.2 Event 系统: 保持当前同步 Stream 方式

保持现有的 `AgentEvent` stream 方式，不升级为多 Observer 模式。

### 4.3 Checkpoint 策略: 智能触发 (可配置)

```rust
enum CheckpointTrigger {
    BeforeToolCall,
    AfterToolCall,
    OnCompaction,
    EveryNSteps(u32),
}

impl CheckpointTrigger {
    fn should_checkpoint(&self, state: &CheckpointState) -> bool {
        match self {
            CheckpointTrigger::BeforeToolCall => state.pending_tool_calls,
            CheckpointTrigger::AfterToolCall => !state.pending_tool_calls && state.last_was_tool_call,
            CheckpointTrigger::OnCompaction => state.just_compacted,
            CheckpointTrigger::EveryNSteps(n) => state.iteration % n == 0,
        }
    }
}
```

---

## 5. 迁移计划

### Phase 1: 基础设施统一 (2-3 周)

**目标**: 统一重复组件，建立基础设施服务

**步骤**:
1. 创建 synthia-permission crate (合并 guardian + permission)
2. 统一 CircuitBreaker - 删除 legacy.rs 中的重复实现
3. 统一 LoopDetector - 删除 loop_detection.rs
4. 统一 Persistence - 删除 file_store.rs，在 Store 中添加 checkpoint 支持

**验证标准**:
- `cargo check --workspace` 通过
- 所有 CircuitBreaker 引用指向 synthia-permission
- 所有 LoopDetector 引用指向 synthia-permission
- SessionFileStore 不再存在

### Phase 2: 核心服务提取 (3-4 周)

**目标**: 提取 ContextService, MemoryService, 完善 PersistenceService

**步骤**:
1. 创建 ContextService trait
2. 简化 ContextAssembler (移除重复逻辑)
3. 创建 MemoryService trait
4. 删除 synthia-agent/src/compaction.rs，改为调用 context service
5. Agent 依赖注入重构

**验证标准**:
- ContextService trait 存在并被 agent 使用
- MemoryService trait 存在并被 agent 使用
- `cargo check --workspace` 通过
- compaction 逻辑只存在于 synthia-context

### Phase 3: Agent 重构 (3-4 周)

**目标**: 实现 Build/Plan/General Executor 分层

**步骤**:
1. 创建 orchestrator.rs - 统一编排器
2. 创建 executor/ 目录 - BuildExecutor, PlanExecutor, GeneralExecutor
3. 重构 react.rs - 简化为纯 ReAct 逻辑，委托给 services
4. 删除 stream_builder/legacy.rs (或完全重构)

**验证标准**:
- 可以创建 BuildExecutor 和 PlanExecutor 实例
- GeneralExecutor 可以被 BuildExecutor 调用
- 原有测试通过
- E2E 测试通过

### Phase 4: 清理与优化 (1-2 周)

**目标**: 删除废弃代码，优化性能

**步骤**:
1. 删除所有废弃文件
2. 验证没有循环依赖 (`cargo tree -t`)
3. 运行完整测试
4. 运行 clippy

**验证标准**:
- `cargo test --workspace` 通过
- `cargo clippy --workspace -- -D warnings` 通过

---

## 6. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 破坏现有功能 | 高 | 每个 phase 有验证标准，测试驱动 |
| 循环依赖 | 中 | Phase 4 验证 cargo tree |
| 性能下降 | 中 | 对比 benchmark 前后 |
| 迁移周期长 | 低 | 分阶段交付价值 |

---

## 7. 后续优化 (非本次重构范围)

- [ ] Observability 增强 (metrics, distributed tracing)
- [ ] Resource Management (rate limiting, quota)
- [ ] Tool Sandbox 完善
- [ ] 多 agent 并发调度