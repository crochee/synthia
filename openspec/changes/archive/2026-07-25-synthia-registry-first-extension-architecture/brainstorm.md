# Brainstorm: synthia-registry-first-extension-architecture

## 多专家对抗性审查结论

### 专家 A：架构师 — 扩展性审查

**核心问题**：Tool-First 的边界在哪？"Tool-First" 不等于 "Tool-Only"。

| 适合作为 Tool 的 | 不适合作为 Tool 的 |
|---|---|
| Shell 执行、文件操作、代码搜索 | 上下文压缩（不是 LLM 主动调用的能力） |
| MCP 工具桥接 | 循环检测（是安全约束，不是可选能力） |
| 多 Agent 通信 | 令牌预算管理（是约束条件，不是行动选项） |
| 网络请求 | Rollout 追踪（是审计机制，不是 LLM 可调用功能） |

**Codex 的验证**：30+ ContextFragment 模块不是 Tool，而是注入到 system prompt 的上下文片段。它们通过注册机制管理，但不是 LLM 可调用的 Tool。

**结论**：区分三个正交的扩展维度：Tool（行动能力）、Fragment（上下文注入）、Interceptor（横切拦截）。

### 专家 B：安全专家 — 安全陷阱审查

**核心问题**：三套并行安全机制是严重隐患。

当前 Synthia 有：
1. `ToolProvider::before_execute/after_execute`
2. `ToolExtensionRegistry::fire_before/fire_after`
3. `InterceptorChain::dispatch(BeforeTool/AfterTool)`

安全检查可能被绕过（只在一处注册但不在另一处注册），或者不一致（三处各检查不同的东西）。

**Codex 的做法**：安全检查在 dispatch 路径硬编码为第一步，不可跳过。

**结论**：安全检查必须统一为单一守卫路径。PermissionCheck 硬编码为不可绕过的第一步。

### 专家 C：扩展性专家 — 扩展机制审查

**核心问题**：
- Scope 生命周期缺失（unregister 未实现）
- Namespace 隔离缺失（工具名冲突）
- DeferredTool 缺失（大量工具 token 浪费）
- ExtensionPoints 是同步的（限制异步扩展）
- Action<T> 模式虽好但不够用

**结论**：补全 Scope、Namespace、Deferred、async 四项扩展基础设施。

### 专家 D：生产可靠性专家 — 运维审查

**核心问题**：
- 缺少 Skill 系统（三个对比项目都有）
- 缺少 Rollout/WorldState（无变更追踪和回滚）
- 缺少 Plugin 动态发现（无运行时加载）
- InterceptorChain 全部 TODO 占位

**结论**：Skill、Rollout、Plugin 三项生产级能力必须补齐。

## 对比项目启发

### OpenCode
- **ApplicationTools + ToolRegistry**：全局不可变 + 会话级可覆盖，Scope 自动清理（Effect.addFinalizer）
- **Tool.make()**：类型安全的工具定义，Schema 编解码，withPermission 装饰器
- **Skill**：特殊的 Tool，用于加载和注入系统指令
- **PermissionV2.Ruleset**：wildcard 匹配、deny/allow 规则链
- **Plugin**：异步加载、生命周期管理、插件宿主

### Codex
- **CoreToolRuntime trait**：共享运行时契约，pre/post hooks，telemetry，exposure
- **ToolExposure (Direct/Deferred)**：延迟加载，首次调用才展开
- **ToolName (namespace::tool)**：命名空间隔离
- **ExtensionToolAdapter**：外部 ToolExecutor 适配到内部 CoreToolRuntime
- **ToolSearchHandler**：BM25 搜索发现 Deferred 工具
- **ContextFragment (30+ 模块)**：每个关注点独立，注册式组合
- **Skills + Plugins + Hooks**：三层扩展体系
- **RolloutBudget**：文件变更版本控制和 token 预算

### pi-mono
- **Extension Loader**：jiti 加载器，动态加载 TypeScript 扩展模块
- **Extension Runner**：EventBus、工具注册、命令注册、事件处理
- **Skills**：Markdown frontmatter 解析，递归搜索技能目录
- **Compaction**：上下文压缩，CompactionPreparation + CompactionResult

## 设计原则

1. **Registry-First**：所有扩展通过 Registry 管理，无硬编码引用
2. **Three Orthogonal Dimensions**：Tool / Fragment / Interceptor 三维度正交，互不干扰
3. **Scope as Lifecycle Boundary**：注册带 Scope，Scope 结束自动清理
4. **Namespace as Isolation Unit**：不同来源的工具通过 Namespace 隔离
5. **Permission as Immutable Guard**：权限检查硬编码为不可绕过的第一步
6. **Fragment as Composable Context**：上下文由 Fragment 组合，Fragment 通过 Registry 注册
7. **Deferred as Performance Optimization**：大量工具按需加载，减少 token 消耗
8. **Async as Default**：ExtensionPoints handler 默认异步，兼容远程调用
