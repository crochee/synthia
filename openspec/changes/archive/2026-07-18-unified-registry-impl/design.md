## Context

Synthia 当前架构存在 6 维技术债：3 套并行 Tool 抽象、2 套并行 Hook 系统、3 套并行 Event 通道、11+ 丢弃的 AgentRunConfig 字段、5 个未触发的 Hook、无 Materialization 过期检测。根因是 **trait 丰富但无统一注册表**。

4 轮多专家对抗性审查（121 条发现）+ `unified-registry-design-review-fixes` 变更（16 条 High 修正已落地设计文档）已完成。本设计覆盖 **Phase 0 + Phase 1 + Phase 2 核心** 的实现，将 3→1 Tool 抽象、引入 Service 注册表、恢复 11 个丢弃字段。

**约束**：
- Rust 类型系统：`Arc<dyn Service>` 不可 downcast（B1），需 TypeId 索引
- 安全：`ToolContext` 不可持有全量 `ServiceRegistry`（B5），需 CapabilityBroker
- 向后兼容：旧 API 标记 `#[deprecated]`，feature flag 共存
- 层级规则：`service → [core]`，`tool → [core, loop, service]`

## Goals / Non-Goals

**Goals:**
1. 统一 Tool 抽象：3 套（Tool/ExecutableTool/ToolProvider）→ 1 套（Tool + ToolProvider + ToolRegistry）
2. 引入 Service 层：系统内部能力通过 Service trait + ServiceRegistry 注册
3. 恢复 11 个丢弃字段：通过 LoopServices 缓存服务引用
4. 安全修复 B5（CapabilityBroker）+ B6（ToolProvenance 命名空间 + 核心不可变）
5. Materialization 过期检测：ToolIdentity 值类型 + ToolGeneration 单调计数器
6. 新增 GoalService + SessionRunCoordinator（B7 最小覆盖）
7. Feature flag `unified-registry` 保证新旧路径共存

**Non-Goals:**
- Hook 统一（Phase 3，15 events → HookService）
- Plugin 统一（Phase 4，ExtensionRegistry + PluginManifest）
- EventBus 统一（Phase 5，3 channels → 1 EventBus）
- Session v1 移除（Phase 6）
- Streaming + MCP 多传输（Phase 7）
- CodeMode（V8 JS runtime，无限期推迟）
- Plugin 沙箱（WASM/seccomp，安全前置但大范围）

## Decisions

### D1：最小可行范围 = Phase 0 + Phase 1 + Phase 2 核心
- **选择**：实现 crate 重构 + Tool 统一 + Service 注册表 + 4 个热路径服务
- **理由**：Phase 0 是所有后续前置条件；Tool 统一是最高价值变更；Service 层是恢复丢弃字段的唯一路径
- **已考虑 alternative**：仅 Phase 1（Tool 统一）— 丢弃字段无法恢复，Tool 依赖无服务层接入

### D2：`synthia-service` 作为新 crate
- **选择**：独立 crate `synthia-service`
- **理由**：层级纯净（`service → [core]`）；可独立测试；避免 `synthia-agent` 职责膨胀
- **已考虑 alternative**：合并入 `synthia-core` — 破坏 Layer 1 纯类型定位

### D3：ServiceRegistry 双索引（TypeId + String）
- **选择**：`type_index: HashMap<TypeId, Arc<ServiceEntry>>` + `name_index: HashMap<String, Vec<Arc<ServiceEntry>>>`
- **理由**：TypeId 索引给热路径 O(1) 类型安全解析；String 索引给诊断/自省
- **已考虑 alternative**：仅 String 索引 — 丢失类型安全，热路径每次需 downcast

### D4：ToolContext 携带 CapabilityBroker 而非 ServiceRegistry
- **选择**：每工具 `ToolCapabilities` 布尔标志 + `CapabilityBroker` 门面
- **理由**：B5 安全阻断 — 全量 ServiceRegistry 允许跨服务提权（dump memory、whitelist self、fork session）
- **已考虑 alternative**：全量 ServiceRegistry + 审计日志 — 治标不治本，攻击在审计前已完成

### D5：热路径 4 服务优先迁移（Session, Hook, Permission, Memory）
- **选择**：先迁移 4 个每 turn 必触的服务，其余 8 个延后
- **理由**：证明 Service trait + ServiceRegistry + LoopServices::bootstrap 模式；降低风险
- **已考虑 alternative**：12 个一次性迁移 — 5-6 个月，风险高

### D6：LoopServices 每 run_stream 缓存一次
- **选择**：`OnceLock<LoopServices>` 在 `run_stream` 入口缓存
- **理由**：每 turn ~100 次 `get()` 调用 × ~µs = 显著开销节省；服务重注册极罕见
- **已考虑 alternative**：每 turn 缓存 — 增加复杂度，收益不明确

### D7：GoalService + RunCoordinator，其余 B5 延后
- **选择**：新增 GoalService（目标追踪 + token budget）+ SessionRunCoordinator（并行 run 仲裁）
- **理由**：GoalService 解决循环中止语义；RunCoordinator 防止并行子代理竞态；PendingMessageQueue 被 SteeringService 覆盖
- **已考虑 alternative**：全部延后 — 丢失循环正确性改进

### D8：Feature flag + 每 service E2E 对比验证
- **选择**：`#[cfg(feature = "unified-registry")]` 门控 + 逐服务 E2E 对比测试
- **理由**：P5 渐进降级 — 旧路径始终可回退；`cargo test --all-features` 在 CI 同时覆盖两条路径
- **已考虑 alternative**：直接替换 — 无法回退，行为回归风险高

## Risks / Trade-offs

[Risk] TypeId 注册不一致导致 `get()` 返回 None → Mitigation: `debug_assert!` 在 `register_provider` 中验证 TypeId 一致性 + 注册文档示例
[Risk] Feature flag 组合爆炸（2^8 = 256 CI 配置） → Mitigation: 固定 CI 为 stable + `--all-features`（2 配置）
[Risk] 迁移破坏 E2E 测试 → Mitigation: 逐服务 E2E 对比；feature flag 回退
[Risk] `parking_lot::RwLock` 写锁 contention → Mitigation: 写锁仅在注册/卸载时获取（极低频）；读锁 ~µs uncontended
[Risk] Materialization 过期检测误报 → Mitigation: `ToolGeneration` 单调递增；仅在实际注册变更时 bump
[Risk] LoopServices 缓存在 plugin hot-reload 时过期 → Mitigation: `LoopServices::invalidate()` + Materialization stale 检测
[Risk] 范围蔓延至延后 Phase → Mitigation: 严格范围边界；每个延后 Phase = 独立 OpenSpec change

[Trade-off] 集中注册 vs 灵活注入 → 接受：集中注册是解决 11 丢弃字段的唯一路径
[Trade-off] 类型安全 vs dyn 兼容 → 接受：debug_assert 在测试期捕获误注册
[Trade-off] 核心工具不可变 vs LIFO 覆盖 → 接受：核心不可变防 shadowing（B6），本地 LIFO 允许用户覆盖

## Migration Plan

### 阶段 0：Crate 重构（~2 周）
1. 创建 `synthia-service` crate（Layer 3）：`Service` trait + `ServiceRegistry` + `ServiceProvider` + `LoopServices`
2. 创建 `synthia-extension` crate（Layer 4 骨架）：仅类型定义，不迁移 HookRunner
3. 创建 `synthia-event` crate（Layer 4 骨架）：仅 `EventBus` trait + `AgentEvent` 类型，不替换现有通道
4. 更新 `Cargo.toml` workspace + 依赖图
5. 验证：`cargo check --workspace` + `cargo clippy --all-targets --all-features`

### 阶段 1：Tool 统一（~4 周）
1. 在 `synthia-core` 定义新 `Tool` trait（3 方法：name, execute, descriptor）
2. 定义 `ToolProvider` trait + `ToolRegistry` + `Materialization` + `ToolIdentity` + `ToolGeneration`
3. 实现 `BuiltinToolProvider`（applications + local 双 map）
4. 实现 `McpToolProvider`（`Arc<dyn McpConnection>`）
5. 实现 `PluginToolProvider`（命名空间 `plugin:<id>:<tool>`）
6. 迁移 5-7 个内置工具到新 trait
7. 实现 `OutputBound` + `SanitizationPolicy` + `bound_output` async
8. 实现 `ToolCapabilities` + `CapabilityBroker`
9. Feature flag 门控：`#[cfg(feature = "unified-registry")]` 新 trait, `#[deprecated]` 旧 trait
10. 验证：`cargo test -p synthia-tool --all-features` + E2E 对比

### 阶段 2a：Service 注册表 + 4 热路径服务（~6 周）
1. 实现 `ServiceRegistry`（TypeId + String 双索引）
2. 实现 `LoopServices::bootstrap`（required vs optional 分离 + no-op fallback）
3. 实现 `OperationContext`（cancellation + deadline 传播）
4. 迁移 SessionService：`impl Service for DefaultSessionService` + `SessionService` subtrait
5. 迁移 HookService：`impl Service for HookRegistry` + `HookService` subtrait
6. 迁移 PermissionService：`impl Service for MergedPolicy` + sync `evaluate` + async `request_approval`
7. 迁移 MemoryService：`impl Service for DefaultMemoryService` + 4-tier subtraits
8. 重构 `AgentRunConfig`：11 字段 → `services: Arc<ServiceRegistry>` + `loop_services: OnceLock<LoopServices>`
9. 重构 `main_loop.rs`：字段访问 → `services.<field>.<method>()`
10. 验证：每服务 E2E 对比 + `cargo test --workspace --all-features`

### 阶段 2b：GoalService + RunCoordinator（~2 周）
1. 定义 `GoalService` trait（current, set, status, budget）
2. 实现 `DefaultGoalService`
3. 定义 `SessionRunCoordinator`（run/wake/interrupt/await_idle）
4. 集成到 `main_loop.rs` step 1a（goal status check）
5. 验证：并行子代理 run 集成测试

### Rollback
- 任何阶段回归均可通过关闭 `unified-registry` feature flag 回退到旧路径
- `#[deprecated]` 旧 API 保留 1 个 release cycle
- `cargo test` 不带 feature flag 仅测试旧路径

## Open Questions

1. **Q-TypeId-Stability**：`TypeId` 在不同编译单元间是否稳定？如果跨 dylib 加载插件，TypeId 可能不一致。当前所有插件以静态链接加载，暂不构成问题，但 WASM 插件需另行设计。
2. **Q-ToolInput-Parsed**：`ToolInput.parsed: Box<dyn erased_serde::Serialize + Send + Sync>` 的实际使用率？如果大多数工具直接 deserialize `raw`，`parsed` 字段可移除以简化 API。
3. **Q-CapabilityBroker-Granularity**：当前 `ToolCapabilities` 是布尔标志（memory_read, session_fork 等）。是否需要更细粒度（如 memory_read 仅限特定 key namespace）？初始版本用布尔标志，后续按需细化。
