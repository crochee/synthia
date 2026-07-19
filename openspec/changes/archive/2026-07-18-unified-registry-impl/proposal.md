## Why

Synthia 存在 3 套并行 Tool 抽象、2 套 Hook 系统、3 套 Event 通道、11+ 丢弃的 AgentRunConfig 字段，根因是 trait 丰富但无统一注册表。多专家审查（121 条发现）确认了 10 条 Blocking + 40 条 High 级别问题。不统一注册表，每新增能力都需代码变更而非 plugin manifest，且安全缺陷（B5 跨服务提权、B6 tool shadowing）无法在当前架构下修复。

## What Changes

**统一 Tool 抽象**
- From: 3 套并行 trait（Tool 11 方法 / ExecutableTool / ToolProvider）
- To: 单一 `Tool` trait（3 方法）+ `ToolProvider` 注册 + `ToolRegistry` 动态注册表
- Reason: 消除同一工具需实现多 trait 才能参与不同注册表的现状
- Impact: non-breaking（feature flag 共存 + `#[deprecated]` 旧 trait）

**引入 Service 注册表**
- From: 服务通过 `AgentRunConfig` 字段 + `Arc<X>` 直接注入，11+ 字段被丢弃
- To: `Service` trait + `ServiceRegistry`（TypeId 双索引）+ `LoopServices` 缓存
- Reason: 恢复丢弃字段、类型安全服务解析、可测试服务替换
- Impact: non-breaking（旧字段标记 `#[deprecated]`，新字段共存）

**ToolContext 安全：CapabilityBroker 替代 ServiceRegistry**
- From: `ToolContext` 持有 `Arc<ServiceRegistry>`，工具可访问所有服务
- To: `ToolContext` 持有 `CapabilityBroker`，每工具声明 `ToolCapabilities` 允许列表
- Reason: B5 安全阻断 — 防止跨服务提权
- Impact: non-breaking（默认空允许列表，纯函数工具无影响）

**ToolProvenance 命名空间 + 核心不可变**
- From: 所有工具按裸名注册，LIFO last-wins，无来源标识
- To: 核心工具不可变（拒絕重注册），plugin 工具命名空间 `plugin:<id>:<tool>`，descriptor 含 provenance
- Reason: B6 安全阻断 — 防止 tool shadowing
- Impact: non-breaking（核心工具行为不变，plugin 工具 LLM 看到命名空间前缀）

**Materialization 过期检测**
- From: LLM 获取工具列表后无过期检测，plugin 卸载导致 resolve panic
- To: `ToolIdentity` 值类型 + `ToolGeneration` 单调计数器 + `Materialization` snapshot
- Reason: opencode 借鉴 — 安全并发工具解析
- Impact: non-breaking（新增检测层，旧路径无变化）

**新增 GoalService + SessionRunCoordinator**
- From: 无目标追踪（循环无法因 goal blocked 中止），无 run 仲裁（并行子代理竞态）
- To: `GoalService`（目标 + token budget）+ `SessionRunCoordinator`（run/wake/interrupt）
- Reason: B7 最小覆盖 — 循环正确性
- Impact: non-breaking（optional 服务，缺失时 no-op fallback）

**Feature flag 共存**
- From: 无迁移路径，重构即破坏
- To: `#[cfg(feature = "unified-registry")]` 门控新旧路径 + `cargo test --all-features` CI
- Reason: P5 渐进降级 — 旧路径始终可回退
- Impact: non-breaking

## Capabilities

### New Capabilities
- `unified-tool-registry`: 统一 Tool trait + ToolProvider + ToolRegistry + Materialization + BuiltinToolProvider + McpToolProvider + PluginToolProvider + OutputBound + SanitizationPolicy
- `service-registry-layer`: Service trait + ServiceProvider + ServiceRegistry（TypeId 双索引）+ LoopServices + OperationContext + 服务生命周期状态机
- `tool-capability-broker`: ToolCapabilities + CapabilityBroker（最小权限服务访问）
- `tool-provenance-namespace`: ToolProvenance 枚举 + 核心不可变 + plugin 命名空间 + prompt_visible_provenance
- `goal-service`: GoalService trait + DefaultGoalService + GoalBudget + GoalStatus
- `session-run-coordinator`: SessionRunCoordinator + RunGuard + RunState

### Modified Capabilities
- `synthia-tool-refactor`: Tool trait 签名从 11 方法缩减为 3 方法（name, execute, descriptor）
- `agent-react-loop`: AgentRunConfig 从 11+ 字段重构为 services + tools + identity；main_loop.rs 从字段访问改为服务解析
- `permission-fail-closed`: PermissionService 新增 evaluate_doom_loop + PolicyStale decision + PolicySnapshot generation

## Impact

- **Crates**: 新增 3 个（synthia-service, synthia-extension 骨架, synthia-event 骨架）；修改 synthia-core, synthia-tool, synthia-agent, synthia-permission, synthia-memory, synthia-hook, synthia-session-v2, synthia-mcp
- **APIs**: Tool trait 从 11 方法→3 方法（`#[deprecated]` 旧方法）；AgentRunConfig 字段重组；新增 ServiceRegistry/ToolRegistry 公开 API
- **Dependencies**: 新增 parking_lot（RwLock）、dashmap（ExtensionRegistry 骨架）
- **Feature flags**: 新增 `unified-registry`（门控所有新 trait + registry）
- **CI**: `cargo test --all-features` 覆盖新旧两条路径
