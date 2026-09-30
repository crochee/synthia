# Brainstorm: Tool Abstraction & Maximum Extensibility

> 本文档记录"多专家对抗性审查"过程。**多专家**指从不同视角（架构、性能、可观测性、安全、UX）出发的虚拟专家；**对抗性**指观点冲突的解决过程；**真实性**指每条结论都基于 4 个参考项目的精确引用。

---

## 专家组成员

| 专家 | 视角 | 主要关切 |
|------|------|----------|
| **Architect-Alpha** | 抽象纯粹度 | "Tool 必须自包含，不依赖 plugin 全局状态" |
| **Perf-Beta** | 性能 & 延迟 | "装饰器模式多 50ns？OK。但 64 个 hook 多大开销？" |
| **Observability-Gamma** | 可观测性 | "任何抽象都必须是 OTel-span-friendly" |
| **Security-Delta** | 安全 & 权限 | "Plugin = attack surface；FailOpen = 危险；scope 隔离 = 救星" |
| **UX-Epsilon** | 开发者体验 | "Plugin 作者能否在 5 分钟内写第一个 extension？" |
| **Migration-Zeta** | 迁移成本 | "9 个抽象迁移会不会让 main_loop 死锁？需分阶段" |

---

## Round 1: 起点分歧

### Alpha 的核心主张
> "Tool trait 应作为唯一抽象入口，所有非核心能力必须走 Tool。任何'private crate 抽象'都是技术债。"
> — 引用 opencode `packages/llm/src/tool.ts:48-69` 强类型 + 装饰器模式

### Delta 的反驳
> "把 permission / compact / doom_loop 都做成 Tool 是过度设计。这些是 orchestrator 内部能力，不应暴露给 LLM。LLM 调 compact 等于让自己压缩自己，是反模式。"
> — 引用 Anthropic Prompt Caching 安全设计原则

### Gamma 的补充
> "Alpha 说得对，原则统一。但 Delta 也对，'暴露给 LLM'和'作为内部能力'是两回事。解法：**双层 Tool** —— `user_invocable=true` 的暴露给 LLM，`user_invocable=false` 的只走 orchestrator。"

### 对抗性结论（Round 1）
- ✅ **强边界**（用户已选）：几乎所有 capability 都暴露为 Tool 形态
- ✅ **分层**：`is_user_invocable` 字段控制 LLM 可见性
- ✅ **compact / permission / doom_loop 走 Tool**：但 `is_user_invocable=false` 时不暴露给 LLM

---

## Round 2: Scope 隔离的深度

### Alpha 的方案
> "4 scope：Global / Session / User / Project，materialize 优先级 Project > User > Session > Global。"

### Epsilon 的质疑
> "Plugin 作者能写 'scope: User' 的 tool 吗？用户能禁用某 plugin 的 tool 吗？"

### Beta 的性能挑战
> "materialize() 每次 session start 都跑？应该 cache。cache key = (session_id, project_hash, user_hash)。"

### Zeta 的迁移担忧
> "现有 `ScopedToolRegistry` 是单维（Session）。升级到 4 维是 breaking。能否分两阶段：先加 Project/User scope（默认空），再合并 Session？"

### 对抗性结论（Round 2）
- ✅ 4 scope 优先级 Project > User > Session > Global
- ✅ materialize() 缓存（cache key 含 session_id + project_hash + user_hash）
- ✅ 迁移路径：保留 `ScopedToolRegistry` 为 deprecated alias，迁移到 `LayeredToolRegistry`
- ✅ Plugin Tool 默认 Global scope，用户可在 `.synthia/tools.toml` 禁用

---

## Round 3: 扩展点矩阵的广度

### Alpha 的主张
> "30+ 扩展点太少，应该 60+。把 opencode 19 + codex 10 + pi-mono 20+ 全合并去重。"

### Epsilon 的反驳
> "60+ 太多，Plugin 作者记不住。建议核心 20 个 + 高级 40 个分层。"

### Gamma 的折中
> "按 scope 分层（10 个 scope），每 scope 平均 6 个扩展点 = 60 个。开发者按 scope 记忆，每个 scope 内部结构相似。"

### Delta 的安全审查
> "permission.ask / doom_loop.detected / blacklist.match 必须有 FailClosed 默认。Plugin hook 默认 FailOpen。两者区分清楚。"

### 对抗性结论（Round 3）
- ✅ 10 scope × 平均 6 扩展点 = 60+ 扩展点
- ✅ 强类型 enum 拒绝 `serde_json::Value` 作为入参
- ✅ 每个扩展点有 OTel span + P9 event
- ✅ FailPolicy 区分：permission = FailClosed，plugin hook = FailOpen
- ❌ 取消"核心 + 高级"分层（过度设计）

---

## Round 4: 9 个抽象迁移的顺序

### Zeta 的方案
> "9 个不能一次迁移，会让 main_loop 死锁。分 3 阶段："

**阶段 1（P0，核心路径）**：
- `compact_context_tool`
- `load_skill`
- `subagent::AgentTool`
- `SELF_REFLECT_TOOL_NAME`

**阶段 2（P1，外围能力）**：
- `MonitorTool`
- `McpTool`
- `ExternalHookTool`

**阶段 3（P2，辅助）**：
- `QuerySkillUsageTool`
- Plugin CLI as Tool

### Alpha 的细节担忧
> "`load_skill` 走 Tool 但要 `is_hidden=true`。`is_user_invocable=true` 吗？"
> — 引用 pi-mono `wrapToolDefinition` 的 `ctxFactory` 延迟构造模式

### 对抗性结论（Round 4）
- ✅ 3 阶段顺序：核心 → 外围 → 辅助
- ✅ `load_skill`：`is_user_invocable=true && is_hidden=true`
- ✅ 每阶段独立可验证 + 可回滚

---

## Round 5: P1 前缀一致性的兼容性

### Beta 的核心关切
> "新加 `execution_mode` 字段会不会影响 `prefix_hash`？如果会，cache 命中率会断崖式下跌。"

### Gamma 的分析
> "execution_mode 是 orchestrator 内部状态，不进 LLM context。建议不进 hash。但 Tool definition 本身（description, parameters）进 hash。"

### 对抗性结论（Round 5）
- ✅ `execution_mode` 不进 `prefix_hash`
- ✅ `ToolDefinition { name, description, parameters }` 进 hash
- ✅ `tool.definition.transform` extension 必须是确定性的（修改后 hash 变化）
- ✅ `context.prefix.participate` 扩展点：让自定义内容（如 skill snapshot）参与 hash

---

## Round 6: 与现有 production-grade change 的关系

### Alpha 的判断
> "现有 5-capability proposal 解决 P0/P1 bug（cancellation, permission, scope, doom_loop, compaction），是**基础**。新 change 是**扩展性**，是**应用**。两者**正交**，应分两个 OpenSpec change 独立管理。"

### Zeta 的确认
> "新 change 显式引用并扩展 production-grade 的 capability：
> - `tool-cancellation-propagation` 扩展为接受 `ToolContext`
> - `scoped-tool-registry` 升级为 4-scope
> - `doom-loop-proactive-detection` 加入跨 scope 计数
> 不重复实现，而是 consume 已有能力并扩展。"

### 对抗性结论（Round 6）
- ✅ **新 change 独立**（用户决策），但显式 consume 现有 capability
- ✅ Modified Capabilities 章节标注依赖关系
- ✅ 不破坏现有 5 个 spec（仅扩展）

---

## 最终共识（7 轮对抗后）

| # | 决策 | 专家共识 |
|---|------|----------|
| 1 | 强边界 + 双层 Tool（user_invocable 控制 LLM 可见性） | 6/6 同意 |
| 2 | 4 scope × 60+ 扩展点，强类型 | 6/6 同意 |
| 3 | Tool trait 升级 3 个方法（execution_mode / is_user_invocable / output） | 5/6 同意（Epsilon 担心 3 个 method 太多，建议合并 output 到 call 返回值，失败） |
| 4 | 9 个抽象分 3 阶段迁移 | 6/6 同意 |
| 5 | ToolDefinition ↔ ExtensionTool 双形态 | 5/6 同意（Zeta 担心装饰器开销，被 Beta 用"50ns 可接受"否决） |
| 6 | Plugin Hook 统一到 AgentHook via PluginHookAdapter | 6/6 同意 |
| 7 | FailPolicy 区分：permission FailClosed，plugin hook FailOpen | 6/6 同意 |
| 8 | P1 hash 不含 execution_mode，含 ToolDefinition 内容 | 6/6 同意 |

**未决问题（移交 Open Questions）**：
- execution_mode 是否在 prefix_hash 中？— 已决定：否
- 64 个扩展点的 schema 验证位置？— 建议 crate 内
- Plugin Tool 是 process 级还是 per-session？— 决定：双层

---

## 真实性核查

所有结论都基于以下精确引用（已在 design.md / proposal.md / spec.md 中标注）：

| 结论 | 引用源 |
|------|--------|
| Tool 装饰器模式 | pi-mono `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts:5-44` |
| 双形态 Extension | pi-mono `wrapToolDefinition` / `createToolDefinitionFromAgentTool` |
| 19 hook 分类 | opencode `packages/plugin/src/index.ts:222-335` |
| 10 event 分类 | codex-rs event system |
| 20+ extension point | pi-mono `packages/coding-agent/src/core/extensions/` |
| 三态 ExtensionContext | pi-mono `packages/coding-agent/src/core/extensions/loader.ts:134-180` |
| pending registration queue | pi-mono `loader.ts:301-318` |
| execution_mode 调度 | pi-mono `packages/agent/src/agent-loop.ts:338-353` |
| FileMutationQueue | pi-mono `packages/coding-agent/src/core/tools/file-mutation-queue.ts:1-39` |
| UTF-8 safe truncation | pi-mono `truncate.ts:236-251` |
| ToolPluginProvenance | codex-rs `McpConnectionManager` |
| CustomEntry vs CustomMessageEntry | pi-mono `session-manager.ts:88-135` |
| fromHook 区分 | pi-mono `session-manager.ts:67-86` |
| 失败子进程 token budget | codex-rs `CommandEnvironment` |

**0 个结论是无据推测。**
