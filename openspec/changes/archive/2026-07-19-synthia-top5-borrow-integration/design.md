# Design — Synthia 仓库级架构重设 change #1: 架构基础设施

> **Architecture decisions, structure, trade-offs, migration** (per OpenSpec `superpowers-bridge` schema)
> **Scope**: change #1 only (~3 月)。change #2-#4 不在本文件范围。

---

## Context

Synthia master (`2f0a9ad`) v3 已合并但仅完成 tool-provider 边界。本 change #1 填实 8 个 infrastructure capability，为 change #2-#4 铺基线。**所有 decisions 围绕"如何在不动 main_loop/agent core/tool business 的前提下，让 8 个 capability 落地"**。

---

## D1 — EventV2 架构：双表 dual-table 还是 in-memory ring？

**Decision**：**dual-table (events table + projections table) via `rusqlite`**。

**Rationale**：
- opencode `event.ts:680` aggregateEvents + `truncate.ts:158` CleanupTask 依赖持久化层
- codex 持久 checkpoint 也通过 sqlite（`codex-rs/state.rs`）
- pi-mono extension/custom event 也需要 durable projection

**Alternatives considered**：
- A. in-memory ring (SPSC MPSC) — 主跨 crate EventV2 接口保留 mpsc，但**底层注入一个 `EventSink` trait**。`#cfg(feature = "event-v2")` 时默认 impl 是 sqlite dual-table；`#cfg(not(...))` 是 in-memory。sqlite 是可选外部 impl，非默认依赖。

**Structure**：
```
synthia-event-v2/
├── src/
│   ├── lib.rs           # EventBus trait, EventSink enum (InMemory | Sqlite)
│   ├── event.rs         # EventEnvelope<T>, EventVersion, EventMeta
│   ├── projector.rs     # Projector trait + CommitGuard
│   ├── store.rs         # EventStore trait
│   ├── sink/
│   │   ├── in_memory.rs # bounded ring + Drop 清理 (默认)
│   │   └── sqlite.rs    # dual-table, gated by event-v2 + sqlite feature
│   ├── aggregate.rs     # aggregate_events<EventType>() 统一 facade
│   └── cleanup.rs       # CleanupTask 7d retention
└── tests/
    ├── projection_consistency.rs
    ├── cleanup_retention.rs
    └── commit_guard.rs
```

**Trade-offs**：
- [+] opencode/codex 一致
- [+] 新增 7d retention + CleanupTask（tool-output-sanitizer 复用）
- [-] `rusqlite` 新增依赖（仅 sqlite impl 用，feature gate）
- 缓解：默认 impl 是 in-memory，不带 sqlite feature 时无 rusqlite 编译成本

---

## D2 — ServiceRegistry 反向依赖如何切断？

**Decision**：**`OutputBound::Service` trait 抽象 + `Capability` typed contract**（change #1 阶段不引入 CapabilityBroker，broker 在 change #2 引入）。

**Rationale**：
- 现有 `synthia-service::traits.rs` 是正向接口（service 提供方法）
- 反向调用（如 main_loop 调用 service）通过 `(ServiceRegistry).resolve()` 已在主路径使用
- 设计冲突来自：`synthia-service` 引用 `synthia-core` + `synthia-agent`，而 `synthia-core` 又想引用 `synthia-service`（design-review H9 blocking）

**解决**：**`OutputBound<T>` trait 提供 owned handle**（来自 opencode `outputBound.ts`）
```rust
pub trait OutputBoundService: Send + Sync {
    type Service: Send + Sync + 'static;
    fn bound(&self) -> Arc<Self::Service>;
}
// 用法：
let bus = registry.resolve::<dyn MyService>()?;  // 现有 API 保留
// 新用法：
let bus = registry.bound_service::<dyn MyService>()?;  // 返回 typed handle，不需 downcast
```

**Change #1 决策点（spec.md 明确记录）**：
- PR-3.1 `OutputBound` trait + `ServiceRegistry::bound_service()` (5 方法)
- PR-3.2 typed `Capability<T>` contract：标注 service 暴露的 capability 子集
- PR-3.3 reverse-dependency resolution：(不引入 broker) — 调用方仅可 resolve 已标注 capability 的 service
- PR-3.4 ServiceRegistry + ExtensionService 双注册

**Trade-offs**：
- [+] 切断 service → core 反向依赖
- [+] Capability 显式标注更易审计（H9 修）
- [−] PR-3.3 仅"反向依赖追踪"而非"动态解析"，change #2 才引 broker
- 后续 change #3 引入 `CapabilityBroker` 替代 `Arc<ServiceRegistry>` 作为 ToolContext 字段（H9 完整闭环）

---

## D3 — HookOutcome 三态如何并行兼容现有双系统？

**Decision**：**双系统并行 3 月，`synthia-hook::HookRunner` 重命名 `synthia_hook::legacy::HookRunner`**，新统一 trait 在 `synthia-hook::Hook` (新)。

**Rationale**：
- 现有 `AgentHook` trait（`synthia-agent::hooks.rs`）在 30+ 处使用
- 现有 `HookRunner`（`synthia-hook::runner.rs`）外进程 plugin 协议在 plugin SDK 使用
- 合并需要 3 个月 deprecation window（plugin 协议变更需要 1 个 release cycle）

**HookOutcome**（来自 codex）：
```rust
pub enum HookOutcome {
    Allow,                          // 默认
    Deny { reason: String },        // 拒绝 + 终止当前 step
    ForwardToMainAgent { hint },    // 转发到 main_agent 不阻塞 subagent
}
```

**10 events**（来自 codex proposal）：
```rust
pub enum HookEvent {
    SessionStart, SessionEnd,
    UserPromptSubmit, PreToolUse, PostToolUse,
    PreResponse, PostResponse,
    PreCompact, PostCompact,
    PreMessageDrop,  // Synthia 独有（JSONL 流中断感知）
}
```

**Migration path**：
- PR-4.1: 新 `HookOutcome` 3 态 + 10 events 落地 `synthia-hook::Hook`，现有 `AgentHook` 标注 deprecated
- PR-4.2: `synthia-extension::Extension` trait 实现 `Hook`（同时存在）
- PR-4.3: 6 月后删除 deprecated `AgentHook`

---

## D4 — GoalService 是否独立 crate？

**Decision**：**独立 crate `synthia-goal-service`**。

**Rationale**：
- codex `codex-core/src/state/goal_service.rs` 420 行，强 isolated 状态
- Weak runtime + Keep/Set OCC 是 7-TaskGoal 排序支撑
- 现有 `synthia-service::goal` 仅 190 行 stub Mutex<Option>，需要全量重写
- 需要 strong isolation：单独 crate 强制单向依赖（agent → goal-service 单向）

**Structure**：
```
synthia-goal-service/
├── src/
│   ├── lib.rs           # GoalService trait
│   ├── code.rs          # CodeGoalService 唯一 impl
│   ├── task.rs          # TaskGoal + 7 状态
│   ├── semaphore.rs     # Arc<Semaphore> rate limit
│   ├── runtime.rs       # Weak runtime + idle eviction
│   └── occ.rs           # Keep/Set OCC retry
└── tests/
    ├── occ_retry.rs
    ├── eviction.rs
    └── semaphore_admission.rs
```

---

## D5 — Materialization 现有 LIFO/RAII 是否要破坏？

**Decision**：**保留不动，仅添加 4 个新 field**。

**Rationale**：
- `synthia-tool::scoped_registry::ScopedToolRegistry` 618 行 LIFO + RAII 已稳定
- design-review B5 仅要求 identity，**不允许破坏现有 LIFO/RAII 语义**（破坏等于 fork 现有 `_in_test` 行为）

**新增**：
```rust
pub struct Materialization {
    pub id: ToolId,                              // uuid + scope-prefix
    pub provider_id: ProviderId,                 // v3 ToolProvider 标识
    pub visibility: ToolVisibility,              // Always | Dynamic { schedule }
    pub wholly_disabled: bool,                   // 整个材料化禁用
    pub provenance: ToolProvenance,              // 来自 builtin | plugin | ephemeral
    pub scope_fork: Option<Arc<Scope>>,          // fork 子 scope
}

impl ScopedToolRegistry {
    pub fn materialize(&self, ...)->Materialization {
        // 现有 LIFO/RAII 不变
        // 新增 identity = self.id + wholly_disabled self check
    }
}
```

**5 个 PR**：
- PR-5.1: `ToolId` + `ProviderId` 新 type，独立 `synthia-tool-materialization` crate
- PR-5.2: `Materialization` + identity 在 `scoped_registry::materialize()` 落
- PR-5.3: `ToolProvenance` + builtin/plugin/ephemeral 区分
- PR-5.4: `Scope.fork` + `whollyDisabled` filter

---

## D6 — OutputSanitizer 在 change #1 范围还是 change #3？

**Decision**：**仅 OutputBound + CleanupTask + ToolContext::take_output。bound 是 change #1；tree-sitter 等是 change #3**。

**Rationale**：
- tool-output-sanitizer 此处不开 tree-sitter（codex & opencode 都在 change #3 才讨论）
- opencode `outputBound.ts` 是 registry-level trait，与 Materialization 紧密相关

**2 PR**：
- PR-6.1: `OutputBound` 60 行实现，挂在 `synthia-tool-materialization` 下
- PR-6.2: `CleanupTask` 7d retention + ToolContext::take_output 在 `synthia-event-v2::cleanup.rs` 复用

---

## D7 — Extension 是否独立 crate？

**Decision**：**是，独立 `synthia-extension-hook`**（与现有 `synthia-extension` 并存 3 月）。

**Rationale**：
- 现有 `synthia-extension/src/lib.rs` 1 行 stub，需要保留兼容
- 19 typed events 太多，导致 1 个 crate 即可编译 100s
- 需要 `wasmtime` 依赖 sandboxing，size 隔离

**Structure**：
```
synthia-extension-hook/
├── src/
│   ├── lib.rs           # Extension trait + ExtensionManifest + 19 typed events
│   ├── manifest.rs      # ExtensionManifest (replaces stub)
│   ├── registry.rs      # ExtensionRegistry (typed)
│   ├── sandbox.rs       # typed capability-scoped execution
│   └── events.rs        # all 19 HookEvent payload
└── tests/
```

---

## D8 — CustomEvent 落地位置？

**Decision**：**`synthia-agent::events::AgentEvent::Custom` 新 variant + `synthia-extension-hook::EventRenderer` registry**。

**Rationale**：
- pi-mono `extensions/types.ts` Custom 50 行
- 现有 28-variant `AgentEvent` 加 1 variant，向后兼容

**3 PR**：
- PR-7.1: `AgentEvent::Custom { type: String, data: serde_json::Value }`
- PR-7.2: `EventRenderer` trait + builtin JSON renderer
- PR-7.3: Custom event 投影到 AgentMessage

---

## D9 — 是否引入 feature flag 灰度？

**Decision**：**是，每个 capability 一个 flag，默认 ON**。

```toml
[features]
default = ["event-v2", "extension-v1", "hook-unified", "goal-service-v1",
           "tool-materialization-v1", "tool-output-sanitizer-v1", "custom-event-v1"]
event-v2 = ["dep:synthia-event-v2"]
extension-v1 = ["dep:synthia-extension-hook"]
hook-unified = ["dep:synthia-hook"]  # 替换双系统的统一 trait
goal-service-v1 = ["dep:synthia-goal-service"]
tool-materialization-v1 = ["dep:synthia-tool-materialization"]
tool-output-sanitizer-v1 = ["dep:synthia-tool-materialization/output-sanitizer"]
custom-event-v1 = []
```

**禁用某个 flag 的副作用**：仅编译 in-process 内部 API，对外语义不变。

---

## D10 — 是否引入 wasm 沙箱？

**Decision**：**否，change #1 仅 typed capability-scoped sandbox（typed Rust trait 范围 + flag 限制），WASM 沙箱推后到 change #3**。

**Rationale**：
- opencode 也是 typed trait 而非 WASM；wasmtime 引入额外风险
- 1+ 年路线不背额外技术风险

---

## 25 PR 列表（实施计划）

按 capability → PR 拆（每 PR < 500 LOC，可独立 review + revert）：

### EventV2 (PR-1.1 ~ PR-1.5)
- PR-1.1: 创建 `synthia-event-v2` crate skeleton + `EventBus` trait + `EventSink` enum
- PR-1.2: `EventEnvelope<T>` + `EventVersion` + `EventMeta` (含 PrefixTracker 三段 hash)
- PR-1.3: `in_memory` sink impl (bounded ring 1024 + Drop) — 主 impl
- PR-1.4: `sqlite` sink impl (dual-table, 仅 enabled with `sqlite` feature) — 可选
- PR-1.5: `Projector` trait + `CommitGuard` + `aggregate_events()` facade + 投影到 protocol header

### Extension (PR-2.1 ~ PR-2.4)
- PR-2.1: 创建 `synthia-extension-hook` skeleton + `Extension` trait + manifest 替换 1 行 stub
- PR-2.2: 19 typed events payload struct
- PR-2.3: typed sandbox infrastructure (capability-scoped)
- PR-2.4: `ExtensionRegistry` + 双注册 (ServiceRegistry + ExtensionRegistry)

### ServiceRegistry + GoalService (PR-3.1 ~ PR-3.4 + 3.5 ~ 3.7)
- PR-3.1: `OutputBound::Service` trait + `ServiceRegistry::bound_service()` 5 方法
- PR-3.2: typed `Capability<T>` contract
- PR-3.3: reverse-dependency resolution (不引入 broker)
- PR-3.4: ExtensionService 双注册接入
- PR-3.5: 创建 `synthia-goal-service` skeleton + GoalService trait
- PR-3.6: `CodeGoalService` via `Arc<Semaphore>` + Weak runtime
- PR-3.7: Keep/Set OCC retry + eviction

### Hook (PR-4.1 ~ PR-4.3)
- PR-4.1: `HookOutcome` 3-state (Allow/Deny/ForwardToMainAgent) + 10 events (含 Synthia 独有 PreMessageDrop)
- PR-4.2: 新统一 `Hook` trait (替换双系统) + 旧 `HookRunner` 标注 deprecated
- PR-4.3: LoopDetector 集成 + 6 月后删除 deprecated

### Tool Materialization (PR-5.1 ~ PR-5.4)
- PR-5.1: `ToolId` + `ProviderId` + `ToolVisibility` 新 type
- PR-5.2: `Materialization` struct + identity 在 `scoped_registry::materialize()`
- PR-5.3: `ToolProvenance` 区分 builtin/plugin/ephemeral
- PR-5.4: `Scope.fork` + `whollyDisabled` filter + tool_id 投影到 session

### Tool OutputSanitizer (PR-6.1 ~ PR-6.2)
- PR-6.1: `OutputBound` 60 行 + Contentlen histogram + 50KiB/2K 行 cap
- PR-6.2: `CleanupTask` 7d retention (event-v2::cleanup.rs) + ToolContext::take_output

### CustomEvent (PR-7.1 ~ PR-7.3)
- PR-7.1: `AgentEvent::Custom` variant
- PR-7.2: `EventRenderer` trait + builtin JSON renderer registry
- PR-7.3: Custom event 投影到 AgentMessage 投影层

---

## Migration Plan

### 阶段 1 (Change #1, ~3 月)：基础设施真化

```
Week 1-2: PR-1.1~1.3 (EventV2 in_memory 默认实现)
Week 3-4: PR-3.1~3.4 (ServiceRegistry 真化 + 反向依赖切断)
Week 5-6: PR-3.5~3.7 (GoalService 独立 crate)
Week 7-8: PR-2.1~2.4 (Extension v2 + sandbox)
Week 9-10: PR-4.1~4.3 (Hook 统一 + LoopDetector)
Week 11-12: PR-5.1~5.4 (Materialization identity)
Week 13-14: PR-6.1~6.2 (OutputBound + CleanupTask)
Week 15-16: PR-7.1~7.3 (CustomEvent)
```

### 阶段 2 (Change #2, ~3 月)：loop/agent/turn
- main_loop 减负 / convertToLlm / SteeringQueue / turn-state machine

### 阶段 3 (Change #3, ~3 月)：tool/orchestrator/permission
- tree-sitter AST / sandbox 二阶段 / orchestrator / router / permission

### 阶段 4 (Change #4, ~2 月)：server/cli/protocol/MCP
- server CLI / MCP / OAuth / 背压 / 分布式

---

## Open Decisions (change #2+ 才定)

- 是否在 change #2 引入 `synthia-pipeline` crate (替代 StreamBuilder)
- change #3 ToolContext `Arc<ServiceRegistry>` → `CapabilityBroker` 升级
- change #3 是否所有 v3 ToolProvider double-register 到 Materialization
- change #4 server 异步 IO 选择 (tokio vs compio)
- 4 change 之间的 service/hook/extension ownership boundary 在本次 verify.md 锁定