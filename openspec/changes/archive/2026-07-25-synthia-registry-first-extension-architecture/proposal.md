# Proposal: synthia-registry-first-extension-architecture

> Change #4 — Registry-First 扩展架构：从 Tool-First 升级为三正交维度扩展总线，实现极致扩展性

## Why

当前 Synthia 的扩展架构存在以下根本性问题：

### 扩展维度坍塌
- 用户设计理念"除了主逻辑外全部抽象为 Tool"方向正确但边界过于激进
- **Tool 语义不能覆盖所有扩展**：上下文压缩、循环检测、令牌预算、Rollout 追踪不是 LLM 可调用的行动能力，不适合 Tool 抽象
- 三个对比项目（OpenCode、Codex、pi-mono）均采用了多维度扩展而非单一 Tool 维度

### Agent 结构体膨胀
- 当前 Agent 有 17 个字段，其中 7 个是 `Option<Arc<dyn ...>>`，说明它们不是核心依赖而是"可能需要的扩展"
- 每添加一个新扩展能力就要修改 Agent 结构体，违反开放-封闭原则

### 三套安全机制并行
- `ToolProvider::before_execute/after_execute`（provider.rs）
- `ToolExtensionRegistry::fire_before/fire_after`（extension_points/tool.rs）
- `InterceptorChain::dispatch(BeforeTool/AfterTool)`（interceptor.rs）
- 三套机制意味着安全检查可能被绕过或不一致，是严重安全隐患

### InterceptorChain 全部占位
- `ApprovalInterceptor`、`RetryInterceptor`、`CompactInterceptor`、`LoopDetectInterceptor` 全部是 `// TODO: Phase 2` 占位
- 安全拦截实际上不存在

### 缺少关键生产级能力
- **Scope 生命周期**：工具注册后无自动清理（unregister 未实现）
- **Namespace 隔离**：工具名是平面字符串，多 MCP Server 同名工具冲突
- **DeferredTool**：所有工具启动时全量加载，大量工具场景 token 浪费
- **ContextFragment 模块化**：上下文组装是单一 Assembler，添加片段需修改核心代码
- **Skill 系统**：三个对比项目都有，Synthia 缺失
- **Rollout 追踪**：无文件变更版本控制和回滚能力
- **异步 ExtensionPoints**：handler 是同步函数，限制了异步扩展

## What Changes

核心设计修正：**"Tool-First" → "Registry-First, Tool-Primarily"**

不是所有东西都应该成为 Tool，但所有东西都应该通过 Registry 管理。三个正交扩展维度：

1. **Tool**（LLM 可调用的行动能力）→ `ToolRegistry`
2. **ContextFragment**（注入到 prompt 的上下文片段）→ `FragmentRegistry`
3. **Interceptor**（横切拦截器，可短路）→ `InterceptorChain`

加上两个辅助维度：
4. **Skill**（提示模板 + 工具组合）→ `SkillRegistry`
5. **Plugin**（打包分发单元）→ `PluginRegistry`

统一由 `ExtensionRegistry` 管理生命周期。

### 变更清单

| # | 变更 | 优先级 |
|---|------|--------|
| 1 | **ExtensionRegistry 统一扩展总线** — 替代 Agent 中 7+ Option 字段 | P0 |
| 2 | **Scope 生命周期** — 工具注册带 Scope，Drop 自动清理 | P0 |
| 3 | **Namespace 隔离** — ToolName 支持 `namespace::tool` 格式 | P0 |
| 4 | **权限守卫硬编码** — 统一三套安全机制为单一不可绕过守卫 | P0 |
| 5 | **InterceptorChain 实际实现** — 填补 TODO 占位 | P0 |
| 6 | **FragmentRegistry 上下文模块化** — 独立于 Tool 的上下文片段注册 | P1 |
| 7 | **DeferredTool 延迟加载** — 大量工具按需加载 | P1 |
| 8 | **异步 ExtensionPoints** — handler 从同步改为异步 | P1 |
| 9 | **Agent 结构体瘦身** — 17 字段 → 4 核心 + ExtensionRegistry | P1 |
| 10 | **Skill 系统** — 提示模板 + 工具组合 + 隐式调用检测 | P2 |
| 11 | **RolloutTracker** — 文件变更版本控制 + token 预算追踪 | P2 |
| 12 | **PluginRegistry** — 动态发现和加载第三方扩展包 | P3 |

## Capabilities

### New Capabilities

| Capability | Description |
|------------|-------------|
| `unified-extension-registry` | ExtensionRegistry 统一管理 Tool/Fragment/Interceptor/Skill/Plugin 五种扩展维度 |
| `tool-scope-lifecycle` | RegistrationScope + Drop 自动反注册，会话级工具自动清理 |
| `tool-namespace-isolation` | ToolName 支持 namespace::tool 格式，多源工具名冲突隔离 |
| `permission-guard-single-path` | 统一三套安全机制为 PermissionInterceptor 硬编码守卫 |
| `interceptor-actual-impl` | Approval/Retry/Compact/LoopDetect 四个 Interceptor 实际实现 |
| `fragment-registry` | FragmentRegistry + ContextFragment trait，上下文片段独立模块化 |
| `deferred-tool-loading` | ToolExposure::Deferred + tool_search 按需加载 |
| `async-extension-points` | ExtensionPoint handler 从同步 Fn 改为 async Fn |
| `agent-struct-slim` | Agent 从 17 字段瘦身到 4 核心 + 1 ExtensionRegistry |
| `skill-system` | SkillRegistry + Skill trait + 隐式调用检测 |
| `rollout-tracker` | RolloutTracker 文件变更版本控制 + token 预算追踪 |
| `plugin-registry` | PluginRegistry 动态发现和加载第三方扩展包 |

## Impact

- **Code**: Agent struct (重构), ExtensionRegistry (新增), FragmentRegistry (新增), SkillRegistry (新增), PluginRegistry (新增), InterceptorChain (填充实现), ToolRegistry (+Scope/+Namespace/+Deferred), ExtensionPoints (async 化变)
- **API**: Agent 公开字段减少（但 ExtensionRegistry 提供等效访问），ToolRegistry API 扩展（register_with_namespace, register_scoped, ToolExposure）
- **Dependencies**: 无新外部依赖
- **Backward compatibility**: Agent 字段通过 ExtensionRegistry 的 deref/getter 保持兼容；InterceptorChain 保持 dispatch 接口不变；ExtensionPoints handler 签名从 sync 变为 async 是 breaking change，需要迁移期
