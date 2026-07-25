# Plan: synthia-registry-first-extension-architecture

## 实施阶段

### Phase 1: 基础设施 (P0 — 安全与核心)

**目标**：补全安全守卫和核心扩展基础设施

1. **ToolName + Namespace** — 修改 `synthia-core/src/tool/` 中的 ToolName、ToolRegistry、ToolDescriptor
2. **RegistrationScope** — 实现 ToolRegistry::unregister 和 Scope Drop
3. **PermissionInterceptor** — 实现权限守卫，替换 TODO 占位
4. **LoopDetectInterceptor** — 适配现有 LoopDetectorSet
5. **ApprovalInterceptor** — 连接 ApprovalService
6. **RetryInterceptor** — 实现指数退避重试
7. **CompactInterceptor** — 触发上下文压缩
8. **统一安全路径** — 废弃 ToolProvider::before/after_execute，迁移到 InterceptorChain

**验证**：cargo check + cargo clippy + cargo test（受影响 crate）

### Phase 2: 扩展维度 (P1 — 模块化与延迟)

**目标**：实现 FragmentRegistry、DeferredTool、异步 ExtensionPoints、Agent 瘦身

1. **FragmentRegistry + ContextFragment trait** — 新增 synthia-context/src/fragment.rs
2. **内建 Fragment 迁移** — 将 ContextAssembler 逻辑拆分为独立 Fragment
3. **ToolExposure + DeferredTool** — 修改 ToolDescriptor 和 ToolRegistry::materialize()
4. **异步 ExtensionPoints** — handler 签名改为 async，提供 sync 兼容包装
5. **ExtensionRegistry** — 组合五个子 Registry
6. **Agent 瘦身** — 17 字段 → 4 核心 + ExtensionRegistry

**验证**：cargo check + cargo clippy + cargo test（全 workspace）

### Phase 3: 生产级能力 (P2-P3 — Skill + Rollout + Plugin)

**目标**：补齐 Skill、Rollout、Plugin 生产级能力

1. **SkillRegistry + Skill trait** — 新增 synthia-agent/src/skills/
2. **内建 Skills** — CodingSkill、SearchSkill、DebugSkill
3. **RolloutTracker** — 新增 synthia-agent/src/rollout/
4. **PluginRegistry + Plugin trait** — 新增 synthia-agent/src/plugins/

**验证**：cargo check + cargo clippy + cargo test（全 workspace）

## 关键依赖

- Phase 2 依赖 Phase 1（InterceptorChain 实际实现后才能统一安全路径）
- Phase 3 依赖 Phase 2（Skill 需要 FragmentRegistry 注入指令，Plugin 需要 ExtensionRegistry 协调跨维度注册）

## 风险

1. **异步 ExtensionPoints 是 breaking change**：所有现有同步 handler 需要迁移。缓解：提供 `register_before_sync()` 兼容方法
2. **Agent 瘦身可能影响下游代码**：缓解：通过 `#[deprecated]` getter 保持兼容
3. **ToolName 从 String 改为 struct 是 breaking change**：缓解：实现 `From<String>` 和 `Display` trait 保持兼容
