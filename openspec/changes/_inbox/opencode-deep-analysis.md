# Opencode 深度架构分析 — Production-grade Agent 参考实现

> 本文档对 `/home/crochee/workspace/opencode` 进行**面向 Rust/synthia 工程复用**的深度架构分析。
> 重点不是“opencode 怎么写”，而是“哪些设计决策可以低成本迁移到 Rust 生态”。
> 所有结论均给出**精确文件:行号引用**，并对每一条给出对 synthia 的可借鉴实现思路。

---

## 目录

1. [总体架构鸟瞰](#1-总体架构鸟瞰)
2. [Plugin / Extension 架构](#2-plugin--extension-架构)
3. [Tool 系统](#3-tool-系统)
4. [Event / Bus 系统](#4-event--bus-系统)
5. [Cache Policy 系统](#5-cache-policy-系统)
6. [Session 持久化与回放](#6-session-持久化与回放)
7. [Effect-TS 抽象在 core 中的应用](#7-effect-ts-抽象在-core-中的应用)
8. [Permission / Safety 模型](#8-permission--safety-模型)
9. [可扩展性总结：什么让添加新功能“廉价”](#9-可扩展性总结什么让添加新功能廉价)
10. [synthia Rust 借鉴清单（按优先级）](#10-synthia-rust-借鉴清单按优先级)

---

## 1. 总体架构鸟瞰

Opencode 是一个**单进程多边界**（server / cli / tui / web / desktop）的 AI agent 框架，核心引擎位于 `packages/core`（V2 已重写为 Effect-TS 风格），LLM 适配层独立于 `packages/llm`，Plugin 抽象在 `packages/plugin` 中**类型化暴露**给外部作者。

```
┌──────────────────────────────────────────────────────────────┐
│  CLI / TUI / App / Desktop / Web / Slack / GitHub / VSCode   │
│  ─── 都通过 SDK + 事件流 + Tool 渲染协议 ───                 │
└──────────────────────────────────────────────────────────────┘
            │                              ▲
            ▼                              │ (SSE / Stream)
┌──────────────────────────────────────────────────────────────┐
│  packages/server (HTTP RPC)                                  │
│  packages/opencode (main 入口)                               │
└──────────────────────────────────────────────────────────────┘
            │                              ▲
            ▼                              │
┌──────────────────────────────────────────────────────────────┐
│  packages/core   ─ V2 Effect-TS 化的服务层                   │
│    ├─ session/  (run loop, history, projector)               │
│    ├─ event/    (typed event bus + DB durable log)           │
│    ├─ tool/     (Tool.make, registry, materialize)           │
│    ├─ permission/ (rule-based, deferred ask)                 │
│    ├─ plugin/   (typed hooks, effect-scoped)                 │
│    ├─ agent/    (immer-drafted state)                        │
│    └─ policy/   (low-level rule evaluator)                   │
└──────────────────────────────────────────────────────────────┘
            │                              ▲
            ▼                              │
┌──────────────────────────────────────────────────────────────┐
│  packages/llm   ─ 协议无关的 LLM 抽象 + Cache Policy          │
│    ├─ schema/   (Effect Schema 强类型 LLMRequest/Tool)       │
│    ├─ route/    (per-protocol transport)                     │
│    ├─ tool-runtime.ts (decode/execute/encode pipeline)      │
│    └─ cache-policy.ts (auto/none/object 策略)                │
└──────────────────────────────────────────────────────────────┘
```

**核心设计哲学（贯穿全文）**：
- **类型化优先**：所有边界（tool、event、permission、plugin hook）都是 Effect `Schema` 派生的强类型。
- **可观察的内核**：每个服务（event、tool、session）都把“序列号、seq、id、metadata”显式建模。
- **可重建（replayable）状态**：session 的所有变化都是“event → projector → 物化状态”，任意时刻可以从 `initial()` 重建。
- **本地优先 + 协议无关**：核心服务对外是 Effect Service，UI / Server / Plugin 都消费同样的 Service 接口。
- **Effect Service + Layer DI**：服务声明 `Context.Service`，实现放在 `Layer.effect`，依赖通过 `yield*` 注入。Rust 侧可 1:1 映射到 `trait + async fn + Arc<Context>`。

---

## 2. Plugin / Extension 架构

opencode 的插件系统是**双形态**的：服务端插件（影响 LLM 行为）和 TUI 插件（影响 UI 渲染）。二者通过**完全独立的 module 形态**隔离，编译期就拒绝 `server + tui` 同存。

### 2.1 插件模块的二元性

`packages/plugin/src/index.ts:74-80`：

```ts
export type Plugin = (input: PluginInput, options?: PluginOptions) => Promise<Hooks>

export type PluginModule = {
  id?: string
  server: Plugin
  tui?: never          // ← 编译期禁止一个 module 同时声明 server+tui
}
```

`packages/plugin/src/tui.ts:630-634`：

```ts
export type TuiPluginModule = {
  id?: string
  tui: TuiPlugin
  server?: never       // ← 镜像
}
```

**对 synthia 的借鉴**：在 Rust 侧可以这样建模——

```rust
pub enum PluginKind {
    Server(ServerPlugin),
    Tui(TuiPlugin),
}
// 不允许一个 .so/.dll 同时是两种，编译期通过 enum exhaustive check 强制。
```

### 2.2 Plugin 生命周期 = Effect Scope

`packages/core/src/plugin.ts:92-181` 是 opencode V2 插件系统的精华：

```ts
add: Effect.fn("Plugin.add")(function* (input) {
  yield* locks.withLock(input.id)(                // ← 同一 plugin.id 并发安全
    Effect.gen(function* () {
      const existing = hooks.find(...)
      if (existing) yield* Scope.close(existing.scope, Exit.void).pipe(Effect.ignore)
      const childScope = yield* Scope.fork(scope)  // ← 每个 plugin 一个子 scope
      const result = yield* input.effect.pipe(
        Scope.provide(childScope),                 // ← 插件 effect 在自己的 scope 跑
        Effect.withSpan("Plugin.load", { attributes: { "plugin.id": input.id } }),
        Effect.onExit((exit) => (Exit.isFailure(exit) ? Scope.close(childScope, exit) : Effect.void)),
      )
      hooks = [...hooks.filter(...), { id, hooks: result ?? {}, scope: childScope }]
      yield* events.publish(Event.Added, { id })   // ← 把"plugin added"也变成 event
    }),
  )
})
```

四个关键点：
1. **每个插件一个 child scope**——`Scope.fork` 拿到一个隔离的 Effect 作用域。`onExit` 注册了**插件 effect 失败时自动 close 子 scope** 的回调，相当于 Rust 的 `Drop + scopeguard`。
2. **`KeyedMutex` 按 plugin.id 加锁**——`KeyedMutex.makeUnsafe<ID>()`（`packages/core/src/effect/keyed-mutex.ts`），保证同一插件 add/remove 不会并发。
3. **插件 effect 返回值就是 Hook 函数表**——`HookFunctions` 是个结构化对象（`packages/core/src/plugin.ts:58-60`），每个字段对应一个 hook 名。
4. **插件加载/卸载都 publish event**——下游消费者（UI、telemetry）通过订阅 `plugin.added` 即可看到插件变化。

### 2.3 Hook 输入/输出是 typed contract

`packages/core/src/plugin.ts:23-65` 用 TypeScript mapped type 把 `HookSpec` 转成 `Hooks` 类型：

```ts
type HookSpec = {
  "catalog.transform": { input: Catalog.Editor; output: {} }
  "aisdk.language":   { input: { model: ModelV2.Info; sdk: any; ... }; output: { language?: LanguageModelV3 } }
  ...
}
export type Hooks = {
  [Name in keyof HookSpec]: Readonly<HookSpec[Name]["input"]> & {
    -readonly [Field in keyof HookSpec[Name]["output"]]: HookSpec[Name]["output"][Field] extends object
      ? Draft<HookSpec[Name]["output"][Field]>   // ← Immer draft，插件可原地改 output
      : HookSpec[Name]["output"][Field]
  }
}
```

`Draft<T>` 来自 `immer`（mutable draft of immutable state）——这意味着插件可以**在 hook 回调里原地修改** output（`output.language = ...`），但 Rust 不需要这种 mutation，synthia 只需把 output 设计成 builder pattern 或 builder struct。

### 2.4 插件触发 = draft + traverse

`packages/core/src/plugin.ts:135-168`：

```ts
triggerFor: Effect.fn("Plugin.triggerFor")(function* (id, name, input, output) {
  const draftEntries = new Map<string, ReturnType<typeof createDraft>>()
  const event = { ...input, ...output } as Record<string, unknown>

  for (const [field, value] of Object.entries(output)) {
    if (value && typeof value === "object") {
      draftEntries.set(field, createDraft(value))  // ← Immer 包装
      event[field] = draftEntries.get(field)
    }
  }

  for (const item of hooks) {
    if (id !== ID.make("*") && item.id !== id) continue
    const match = item.hooks[name]
    if (!match) continue
    yield* match(event as any).pipe(Effect.withSpan(`Plugin.hook.${name}`, {
      attributes: { plugin: item.id, hook: name },
    }))
  }

  for (const [field, draft] of draftEntries) {
    event[field] = finishDraft(draft)              // ← 收集修改
  }
  return event as any
})
```

要点：
- **`triggerFor(pluginID, hookName, input, output)`** 同时支持**全局触发**（`id = "*"`）和**按插件 ID 触发**。
- 输出对象是 Immer draft：插件 A 改的字段会被插件 B 看到（`finishDraft` 在循环外统一结算）。
- `Effect.withSpan` 给每次 hook 调用加了 OTel span——observability 一等公民。

### 2.5 旧式 Hooks（v1 兼容 + 实验 hook）

`packages/plugin/src/index.ts:222-335` 是**对外暴露给第三方作者**的 Hook 接口，包括：

| Hook | 签名（节选） | 文件:行号 |
|------|---|---|
| `event` | `(input: { event: Event }) => Promise<void>` | `index.ts:224` |
| `tool` | `{ [key: string]: ToolDefinition }` | `index.ts:226-228` |
| `auth` | `AuthHook` | `index.ts:229` |
| `provider` | `ProviderHook` | `index.ts:230` |
| `chat.message` | `(input, output) => Promise<void>` | `index.ts:234-243` |
| `chat.params` | 改 `temperature/topP/topK/maxOutputTokens/options` | `index.ts:247-256` |
| `chat.headers` | 注入 `headers: Record<string, string>` | `index.ts:257-260` |
| `permission.ask` | 覆盖 `status: "ask" \| "deny" \| "allow"` | `index.ts:261` |
| `command.execute.before` | 改 `parts: Part[]` | `index.ts:262-265` |
| `tool.execute.before` | 改 `args: any` | `index.ts:266-269` |
| `shell.env` | 注入 `env: Record<string, string>` | `index.ts:270-273` |
| `tool.execute.after` | 改 `title/output/metadata` | `index.ts:274-281` |
| `experimental.chat.messages.transform` | 整段改 messages 数组 | `index.ts:282-290` |
| `experimental.chat.system.transform` | 整段改 system 数组 | `index.ts:291-296` |
| `experimental.provider.small_model` | 选择 sub-agent 小模型 | `index.ts:297` |
| `experimental.session.compacting` | 自定义 compaction prompt | `index.ts:305-308` |
| `experimental.compaction.autocontinue` | 跳过/允许 auto-continue turn | `index.ts:316-326` |
| `experimental.text.complete` | 改最终 text 输出 | `index.ts:327-330` |
| `tool.definition` | 改 LLM 看到的 tool description + parameters | `index.ts:331-334` |

**关键观察**：所有“改 output”的 hook 都遵循一个范式——**input 是不可变快照，output 是可变更 buffer**。这种 `input / output` 命名稳定地映射到 Rust 的 `(Input, &mut Output)`，synthia 可以用 `Hook::on_chat_params(input: &ChatParams, output: &mut ChatParams)` 这种签名。

### 2.6 真实插件示例（`tui-smoke.tsx`）

`/home/crochee/workspace/opencode/.opencode/plugins/tui-smoke.tsx` 是一个 1000+ 行的真实插件，演示了：
- 注册新 **route**（`api.route.register([...])`，`tui-smoke.tsx:997-1006`）
- 注入新 **slot**（`api.slots.register(item)`，`tui-smoke.tsx:1009-1011`）
- 注入新 **command + keybinding layer**（`api.keymap.registerLayer({...})`，`tui-smoke.tsx:870-978`）
- 自定义 **post-process effect**（`api.renderer.addPostProcessFn`，`tui-smoke.tsx:991-995`）
- 注入 **theme**（`api.theme.install/set`，`tui-smoke.tsx:984-985`）
- 使用 **dialog stack** 弹窗（`api.ui.dialog.replace/clear`，`tui-smoke.tsx:220-232`）

**对 synthia 的借鉴**：synthia 不必一次性实现所有 19 个 hook。可以分阶段：
- Phase 1：`tool` + `event` + `chat.params/headers`（最常用）
- Phase 2：`tool.execute.before/after`（最影响 LLM 表现）
- Phase 3：`experimental.*` 全部

### 2.7 Plugin 服务端入口

`packages/core/src/plugin.ts:81`（legacy v1 接口）和 `packages/core/src/plugin/index.ts`（`boot.ts`、`agent.ts`、`command.ts`、`provider.ts`、`skill.ts`）都存在，意味着 Plugin 的不同 *facet* 单独加载。**这是一个非常重要的可扩展性原则**：插件不是单一对象，而是按“aspect”分别实例化。

---

## 3. Tool 系统

Tool 系统是 agent 框架的“原子操作层”。opencode 把 Tool 设计为**强类型 + 协议无关 + Effect-native**。

### 3.1 Tool 的二元抽象（core ↔ llm）

`packages/llm/src/tool.ts:48-69` 定义**协议无关的 Tool**：

```ts
export interface Tool<Parameters extends ToolSchema<any>, Success extends ToolSchema<any>> {
  readonly description: string
  readonly parameters: Parameters           // Effect Schema
  readonly success: Success
  readonly execute?: ToolExecute<Parameters, Success>
  readonly toModelOutput?: ToolToModelOutput<Parameters, Success>
  readonly toStructuredOutput?: (output: Success["Encoded"]) => unknown
  /** @internal */
  readonly _decode: (input: unknown) => Effect.Effect<...>
  readonly _encode: (value: ...) => Effect.Effect<...>
  readonly _project: (...) => ToolOutputType
  readonly _legacyResult: boolean
  readonly _definition: ToolDefinitionClass   // ← 已构造好的 LLM 协议对象
}
```

`packages/core/src/tool/tool.ts:20-114` 定义**core 侧的 Tool 包装**：

```ts
export interface Definition<Input extends SchemaType<any>, Output extends SchemaType<any>> {
  readonly [TypeId]: { readonly _Input: Input; readonly _Output: Output }
}

type Config<...> = {
  readonly description: string
  readonly input: Input
  readonly output: Output
  readonly execute: (input, context) => Effect.Effect<Output, ToolFailure>
  readonly toModelOutput?: (input: { input, output }) => ReadonlyArray<Content>
}
```

二者通过 `runtimes: WeakMap<AnyTool, Runtime>`（`tool.ts:60`）解耦——core 工具注册时不需要知道 llm 协议细节，protocol 适配在 `_definition` 懒构建时完成。

### 3.2 `Tool.make` 双模式：typed vs dynamic

`packages/llm/src/tool.ts:133-165` 重载 4 种签名：

```ts
// Typed: Schema 已知
Tool.make({ description, parameters: Schema.Struct({...}), success: Schema.Struct({...}), execute })
// Dynamic: JSON Schema 来自外部（MCP、plugin manifest）
Tool.make({ description, jsonSchema: {...}, outputSchema: {...}, execute })
```

**对 synthia 的借鉴**：
- 静态分支用 `schemars` derive + trait `JsonSchema`
- 动态分支直接接受 `serde_json::Value` 描述的 JSON Schema
- **两种模式合成一个 `Tool` trait**，synthia 工具箱可以接受 typed tool（MCP、buildin）和 dynamic tool（外部 plugin）混用

### 3.3 ToolContext：runtime 元数据

`packages/plugin/src/tool.ts:3-27`（给插件作者看的 v1 视图）：

```ts
export type ToolContext = {
  sessionID: string
  messageID: string
  agent: string
  directory: string        // ← 当前 project 目录，优先用这个而不是 process.cwd()
  worktree: string         // ← worktree 根，用于生成稳定相对路径
  abort: AbortSignal       // ← 可取消信号
  metadata(input: { title?: string; metadata?: { [key: string]: any } }): void
  ask(input: AskInput): Promise<void>
}
```

`packages/core/src/tool/tool.ts:9-14`（core 侧 v2）：

```ts
export interface Context {
  readonly sessionID: SessionSchema.ID
  readonly agent: AgentV2.ID
  readonly assistantMessageID: SessionMessage.ID
  readonly toolCallID: string
}
```

**对 synthia 的借鉴**：
- `directory` + `worktree` 两个字段是**Rust 工具最缺的**——大多数 agent 都假设 `process::current_dir()`，但实际工作目录在 multi-workspace 场景下会漂移。
- `abort: AbortSignal` 必须贯穿整条调用链。Rust 侧建议用 `tokio_util::sync::CancellationToken` 而不是 `tokio::sync::oneshot`，因为它支持**层级取消**。
- `metadata()` 和 `ask()` 是“输出反馈 + 交互”通道，synthia 可以拆成 `ctx.report_progress(...)` 和 `ctx.request_permission(...)`。

### 3.4 Tool 工具注册 = scoped stack（**LIFO override**）

`packages/core/src/tool/registry.ts:47-104`：

```ts
type Registration = { readonly identity: object; readonly tool: AnyTool }
const local = new Map<string, Array<{ readonly token: object; registration: Registration }>>()

register: Effect.fn("ToolRegistry.register")(function* (tools) {
  ...
  yield* Effect.uninterruptible(
    Effect.gen(function* () {
      const token = {}
      for (const [name, tool] of entries)
        local.set(name, [...(local.get(name) ?? []), { token, registration: { identity: {}, tool } }])
      yield* Effect.addFinalizer(() =>                           // ← Scope finalizer
        Effect.sync(() => {
          for (const [name] of entries) {
            const registrations = local.get(name)?.filter((registration) => registration.token !== token) ?? []
            if (registrations.length > 0) local.set(name, registrations)
            else local.delete(name)
          }
        }),
      )
    }),
  )
})
```

`materialize` 取**每个工具名的栈顶**（`entries.at(-1)?.registration`）：

```ts
materialize: ... {
  const registrations = new Map(applications.entries())   // 1. built-in
  for (const [name, entries] of local) {                 // 2. plugin/tool 覆盖
    const registration = entries.at(-1)?.registration
    if (registration) registrations.set(name, registration)
  }
  for (const [name, registration] of registrations)      // 3. permission 过滤
    if (whollyDisabled(permission(registration.tool, name), permissions)) registrations.delete(name)
  return { definitions, settle: ... }
}
```

**这是 opencode 最优雅的扩展点之一**：
- **栈式 override** = 最新的注册赢。plugin 加载到 scope，scope 退出时自动 pop。
- **`identity: object`** 用于 tool 替换后让旧 tool call 立即失效（`advertised && registration.identity !== advertised` 触发 stale error）。
- **三段组合**：`built-in → plugin override → permission filter` 是可推理的。

**对 synthia 的借鉴**：
- 把 tool registry 设计成 `Arc<RwLock<HashMap<String, Vec<ToolEntry>>>>`，entry 持有 `token: u64`。
- `register(tools) -> RegistrationToken` 拿一个 token，drop 时 unregister。Rust 没有 effect scope，可以用 `Drop` + `Arc::strong_count` 模拟。
- 启动时 `materialize(permission_set)`：built-in → plugin override → permission filter，**与注册顺序无关**。

### 3.5 Tool 调度：parallel + 取消感知

`packages/core/src/session/runner/llm.ts:191, 245-284` 实际跑工具：

```ts
const toolFibers = yield* FiberSet.make<void, ToolOutputStore.Error>()
...
const providerStream = llm.stream(request).pipe(
  Stream.runForEach((event) =>
    Effect.gen(function* () {
      ...
      if (event.type !== "tool-call" || event.providerExecuted) return
      needsContinuation = true
      const assistantMessageID = yield* publisher.assistantMessageID(event.id)
      yield* Effect.uninterruptibleMask((restore) =>
        restore(
          toolMaterialization.settle({
            sessionID: session.id, agent: agent.id, assistantMessageID, call: event,
          }),
        ).pipe(
          Effect.flatMap((settlement) =>
            publish(LLMEvent.toolResult({...}), settlement.outputPaths ?? []),
          ),
        ),
      ).pipe(FiberSet.run(toolFibers))   // ← 并发执行多个 tool call
    }),
  ),
  ...
)
```

要点：
1. **`FiberSet`** = 一组有界 fiber。多个 tool call 同时跑（parallel）。
2. **`Effect.uninterruptibleMask`** 包裹 tool 执行本身——用户中断信号不会把执行到一半的 tool 杀掉，只会**标记为 interrupted**（fail with "Tool execution interrupted"），避免半成品状态。
3. **provider-executed tool 不本地执行**——`event.providerExecuted` 表示是 LLM 端执行的（如 OpenAI code-interpreter），只发 `toolResult` 即可。
4. **run 之后 await 所有 fiber**——`awaitToolFibers` 等待所有并行 tool 完成（`runner/llm.ts:137-138`）：

```ts
const awaitToolFibers = (fibers: FiberSet.FiberSet<void, ToolOutputStore.Error>) =>
  Effect.raceFirst(FiberSet.join(fibers), FiberSet.awaitEmpty(fibers))
```

**对 synthia 的借鉴**：
- `tokio::task::JoinSet<ToolError>` + `tokio_util::sync::CancellationToken`。
- Tool execution 必须包在 `tokio::select! { _ = token.cancelled() => Err(Interrupted), _ = tool.run() => ... }` 里，且默认 `CancellationToken` 是 **child token**（不直接取消子操作）。

### 3.6 Tool 错误模型

`packages/llm/src/schema/errors.ts`（不在本次阅读范围但已被引用）+ `packages/llm/src/tool-runtime.ts:31-35`：

```ts
return decodeAndExecute(tool, call).pipe(
  Effect.map((value) => result(call, value)),
  Effect.catchTag("LLM.ToolFailure", (failure) =>
    Effect.succeed(result(call, { type: "error", value: failure.message }, failure.error)),
  ),
)
```

`ToolFailure` 是**唯一**的 tool 错误类型——任何**未映射**的错误都会让 stream fail 退出（这是有意的安全网）。

**对 synthia 的借鉴**：
- 定义一个 `enum ToolError { Failure(String), Interrupted, PermissionDenied, Stale, SchemaMismatch }`，让 `Display` 友好输出。
- 不要在 tool 内做 retry，retry 责任上提到 LLM orchestrator。

### 3.7 工具 `toModelOutput` 与 `toStructuredOutput` 分离

`packages/llm/src/tool.ts:48-54, 196-205, 238-249`：

```ts
interface Tool<...> {
  ...
  readonly toModelOutput?: ToolToModelOutput<Parameters, Success>  // 人类/LLM 看到的文本
  readonly toStructuredOutput?: (output: Success["Encoded"]) => unknown  // 持久化的结构化数据
  ...
}

const project = (toModelOutput, toStructuredOutput, parameters, callID, output) =>
  ToolOutput.make(
    toStructuredOutput?.(output) ?? output,                       // structured
    toModelOutput?.({...}) ?? [...],                                // 文本
  )
```

**两层输出**的价值：
- `toModelOutput`：丢给 LLM 看，可能含 `Warnings:`、`Exit code: 1` 等人类友好行。
- `toStructuredOutput`：存到 DB，未来 replay/分析时不丢字段。

`packages/core/src/tool/bash.ts:65-71, 122` 的 bash 工具就是经典案例——`toModelOutput` 加 `\n\nCommand exited with code N.`，`toStructuredOutput` 是 JSON（`{ command, cwd, exitCode, output, truncated, ... }`）。

**对 synthia 的借鉴**：
- Rust 侧建议 `trait Tool { type Output: Serialize + Deserialize; fn to_model_output(&Output) -> String; fn to_structured_output(&Output) -> serde_json::Value; }`。
- 这样**LLM 看到的是 terse 摘要，DB 存的是 full schema**，未来可独立演化。

---

## 4. Event / Bus 系统

opencode V2 把 Event 升级为**带版本号 + 持久化 + 可回放 + 多 pubsub fan-out** 的“事件流数据库”，**不再**是单纯的进程内 bus。

### 4.1 事件定义 = Schema

`packages/core/src/event.ts:96-133`：

```ts
export function define<const Type extends string, Fields extends Schema.Struct.Fields>(input: {
  readonly type: Type
  readonly sync?: { readonly version: number; readonly aggregate: string }   // ← 可持久化版本
  readonly schema: Fields
}): Schema.Schema<Payload<Definition<Type, Schema.Struct<Fields>>>> & Definition<Type, Schema.Struct<Fields>>
```

`packages/core/src/session/event.ts:50-58`（session 事件的典型用法）：

```ts
export const AgentSwitched = EventV2.define({
  type: "session.next.agent.switched",
  ...options,                                                    // ← sync: { aggregate: "sessionID", version: 1 }
  schema: { ...Base, messageID: SessionMessageID.ID, agent: Schema.String },
})
```

**`sync`** 字段把事件分成两类：
- **durable**（`sync` 已设）→ 写入 SQLite（`packages/core/src/event/sql.ts`），可跨进程 replay。
- **ephemeral**（无 `sync`）→ 仅进程内 PubSub，崩溃即丢。如 `session.next.text.delta`（`session/event.ts:236-244`）—— stream 增量没必要持久化。

### 4.2 事件 ID 与 cursor

`packages/core/src/event.ts:13-19, 26-27, 81-83`：

```ts
export const ID = Schema.String.check(Schema.isStartsWith("evt_")).pipe(Schema.brand("Event.ID"), ...)
export const Cursor = NonNegativeInt.pipe(Schema.brand("EventV2.Cursor"))  // 聚合内的严格递增序号
export function versionedType(type: string, version: number) {
  return `${type}.${version}`   // ← DB 里存 "session.next.step.ended.2"
}
```

**对 synthia 的借鉴**：
- Rust 侧：`struct EventId(String)` + `struct EventCursor(u64)`，二者用 newtype 隔离，编译期防止误用。
- `versioned_type("foo", 2)` → `"foo.2"` 模式是**事件 schema 演化的安全网**——DB 里同时存了 v1、v2 的事件，replay 时按 version 选择 decoder。

### 4.3 publish / subscribe / project / replay 一体

`packages/core/src/event.ts:147-173`：

```ts
export interface Interface {
  readonly publish: <D extends Definition>(definition: D, data: Data<D>, options?: PublishOptions) => Effect.Effect<Payload<D>>
  readonly subscribe: <D extends Definition>(definition: D) => Stream.Stream<Payload<D>>
  readonly all: () => Stream.Stream<Payload>
  readonly aggregateEvents: (input: { aggregateID: string; after?: Cursor }) => Stream.Stream<CursorEvent>
  readonly sync: (handler: Sync) => Effect.Effect<Unsubscribe>      // ← 跨进程同步（如 CLI → server）
  readonly listen: (listener: Listener) => Effect.Effect<Unsubscribe>  // ← 进程内 listener
  readonly beforeCommit: (guard: CommitGuard) => Effect.Effect<void>   // ← 提交前钩子（一致性检查）
  readonly project: <D extends Definition>(definition: D, projector: Projector<D>) => Effect.Effect<void>  // ← 物化到 read-model
  readonly replay: (event: SerializedEvent, options?: ...) => Effect.Effect<void>
  readonly replayAll: (events: SerializedEvent[], options?: ...) => Effect.Effect<string | undefined>
  readonly remove: (aggregateID: string) => Effect.Effect<void>
  readonly claim: (aggregateID: string, ownerID: string) => Effect.Effect<void>   // ← 多 owner 抢占
}
```

### 4.4 事务：commit guards + projectors + DB 写入

`packages/core/src/event.ts:255-379` 是核心 commit 逻辑，单个事务里：

```ts
return yield* Effect.uninterruptible(
  Effect.gen(function* () {
    const committed = yield* db
      .transaction(() =>
        Effect.gen(function* () {
          const row = yield* db.select({ seq, ownerID }).from(EventSequenceTable)
                                    .where(eq(EventSequenceTable.aggregate_id, aggregateID)).get().pipe(Effect.orDie)
          const latest = row?.seq ?? -1
          const encoded = syncRegistry.get(versionedType(definition.type, sync.version))!.encode(event.data) as Record<string, unknown>

          // 1. owner 校验（防止两个 node 抢同一 aggregate）
          if (input?.strictOwner && row?.ownerID && row.ownerID !== input.ownerID) {
            yield* Effect.die(new InvalidSyncEventError({ ... message: `Replay owner mismatch...` }))
          }

          // 2. replay 时如果 seq 已存在，跳过（idempotent）
          if (input && input.seq <= latest) {
            const stored = yield* db.select().from(EventTable)....get().pipe(Effect.orDie)
            if (stored?.id === event.id && stored.type === versionedType(...) && isDeepStrictEqual(stored.data, encoded)) {
              // ... claim ownership if needed
              return                                  // ← 已写入过，幂等退出
            }
            yield* Effect.die(new InvalidSyncEventError({ message: `Replay diverged...` }))
          }

          // 3. 提交前钩子（业务级校验）
          for (const guard of commitGuards) yield* guard(event)

          // 4. 投影器（更新 read-model）
          for (const projector of list) yield* projector({ ...event, seq } as Payload)

          // 5. 事务性 commit callback（仅同步事件）
          if (commit) yield* commit(seq)

          // 6. 写 EventSequenceTable（aggregate → 最新 seq）
          yield* db.insert(EventSequenceTable).values([{ aggregate_id: aggregateID, seq, owner_id: input?.ownerID }])
                  .onConflictDoUpdate({...}).run().pipe(Effect.orDie)

          // 7. 写 EventTable（事件主体）
          yield* db.insert(EventTable).values([{ id: event.id, aggregate_id, seq, type: versionedType(...), data: encoded }]).run()
          return { aggregateID, seq }
        }),
        { behavior: "immediate" },  // ← 立即 BEGIN，避免 BEGIN ... SLEEP
      ).pipe(Effect.orDie)
    if (committed) {
      // 8. 通知 aggregate 订阅者
      yield* Effect.forEach(synchronized.get(committed.aggregateID) ?? [], (pubsub) => PubSub.publish(pubsub, undefined), { discard: true })
    }
    return committed
  }),
)
```

这是 Event Sourcing 的教科书实现：
- **Aggregate 根（sessionID）** = 一致性边界。
- **EventSequenceTable** = 聚合版本号 + owner 字段，支持多进程抢占（`claim()`）。
- **事务内 commit guard + projector**：要么全部成功，要么全部回滚。
- **seq 单调递增 + deep-equal idempotency**：重放同一 seq 自动幂等。

### 4.5 进程内 pubsub

`packages/core/src/event.ts:184-213`：

```ts
const all = yield* PubSub.unbounded<Payload>()
const synchronized = new Map<string, Set<PubSub.PubSub<void>>>()   // per-aggregate 唤醒器
const typed = new Map<string, PubSub.PubSub<Payload>>()             // per-type 事件流
const projectors = new Map<string, AnyProjector[]>()
const commitGuards = new Array<CommitGuard>()
const listeners = new Array<Listener>()
const syncHandlers = new Array<Sync>()

yield* Effect.addFinalizer(() =>
  Effect.gen(function* () {
    yield* PubSub.shutdown(all)
    ...shutdown all pubsubs...
  }),
)
```

`streamEvents`（`event.ts:606-628`）把**历史 replay + 实时订阅**无缝拼接：

```ts
const streamEvents = (input: { aggregateID: string; after?: Cursor }): Stream.Stream<CursorEvent> =>
  Stream.unwrap(
    Effect.gen(function* () {
      const synchronized = yield* subscribeSynchronized(input.aggregateID)
      let cursor = input.after ?? -1
      const read = Effect.suspend(() => readAfter(input.aggregateID, cursor))...
      const historical = yield* read                                                // ← 启动时拉一次历史
      const live = Stream.fromSubscription(synchronized).pipe(                     // ← 之后订阅新事件
        Stream.mapEffect(() => read),
        Stream.flattenIterable,
      )
      return Stream.concat(Stream.fromIterable(historical), live)                  // ← 历史+实时
    }),
  )
```

`readAfter` 在 `> cursor` 处用事务隔离读（`event.ts:562-584`），保证拉到的事件**严格 > 之前 cursor**，不会因为其他 node 写入而漏/重。

### 4.6 listener vs projector 区别

- **listener**（`event.ts:56, 630-637`）：纯函数，进程内副作用（如发 webhook）。失败只 log（`observe(event, "listener", listener)` catch Cause），不影响 commit。
- **projector**（`event.ts:53, 254, 653-658`）：**事务内**调用，作用是更新 read-model。失败会让整个 commit 回滚。
- **commit guard**（`event.ts:55, 333-335, 648-651`）：**事务内**调用，做一致性检查（不允许非法状态被持久化）。

**对 synthia 的借鉴**：
- 区分 “read-model 投影” vs “side-effect listener” vs “invariant guard” 三种回调，是 Event Sourcing 系统的核心 pattern。
- synthia 即使不用 SQLite，也可以用 in-memory `Arc<RwLock<...>>` + `tokio::sync::broadcast` 实现简化版（用于 phase 0），后续 phase 再升级。

### 4.7 Event registry（schema 自动注册）

`packages/core/src/event.ts:85, 119-122`：

```ts
export const registry = new Map<string, Definition>()
const syncRegistry = new Map<string, SyncDefinition>()

const existing = registry.get(input.type)
if (input.sync === undefined || existing?.sync === undefined || input.sync.version >= existing.sync.version) {
  registry.set(input.type, definition)
}
```

注意：**新版本≥旧版本**才能覆盖 registry。这允许旧 DB 中 v1 事件 + 新代码发布 v2 时**共存**——但反之不行（新代码发布 v1 不会覆盖旧代码的 v2）。

**对 synthia 的借鉴**：Rust 侧 `LazyLock<Mutex<HashMap<String, EventDef>>>`，新 schema 必须 bump version。

### 4.8 全局 EventType 列表（看 session 就够）

`packages/core/src/session/event.ts:471-500` 列出了 28+ 个 session 事件，按 namespace 分组：

| Namespace | 关键事件 |
|---|---|
| `session.next.*` | agent.switched / model.switched / moved / prompted / interrupt.requested / context.updated / synthetic |
| `session.next.prompt.*` | admitted / promoted |
| `session.next.shell.*` | started / ended |
| `session.next.step.*` | started / ended / failed |
| `session.next.text.*` | started / **delta** / ended |
| `session.next.reasoning.*` | started / **delta** / ended |
| `session.next.tool.*` | input.{started,delta,ended} / called / progress / success / failed |
| `session.next.compaction.*` | started / **delta** / ended |
| `permission.v2.*` | asked / replied |
| `plugin.*` | added |

`Delta` 系列（`text.delta`、`reasoning.delta`、`tool.input.delta`、`compaction.delta`）都是 **ephemeral**——只走 PubSub 不写 DB。`Ended` 才是 durable 边界（`session/event.ts:235, 273, 317` 注释明确说“Stream fragments are live-only; Ended is the replayable full-value boundary”）。

**对 synthia 的借鉴**：在 spec 里**强制**把 stream-fragment 和 full-snapshot 拆成两个事件名，避免"我不知道这段是 real-time 增量还是 replay" 的歧义。

### 4.9 plugin 同步跨进程

`packages/core/src/event.ts:639-646` + `core/src/event/sql.ts`：plugin 可以注册 `sync` 处理器，把**指定**事件通过 server push 到 TUI/desktop 客户端。这是 `Bus.subscribe` 跨进程版本——在 client/server 架构中非常有用。

---

## 5. Cache Policy 系统

`packages/llm/src/cache-policy.ts`（共 110 行）是 opencode 在**协议无关**层面做 prompt cache 优化的核心。

### 5.1 三种 policy 形态

`packages/llm/src/schema/options.ts:190-220`：

```ts
export class CacheHint extends Schema.Class<CacheHint>("LLM.CacheHint")({
  type: Schema.Literals(["ephemeral", "persistent"]),
  ttlSeconds: Schema.optional(Schema.Number),
}) {}

export const CachePolicyObject = Schema.Struct({
  tools:    Schema.optional(Schema.Boolean),
  system:   Schema.optional(Schema.Boolean),
  messages: Schema.optional(Schema.Union([
    Schema.Literal("latest-user-message"),
    Schema.Literal("latest-assistant"),
    Schema.Struct({ tail: Schema.Number }),
  ])),
  ttlSeconds: Schema.optional(Schema.Number),
})

export const CachePolicy = Schema.Union([
  Schema.Literal("auto"),
  Schema.Literal("none"),
  CachePolicyObject,
])
```

`cache-policy.ts:18-37` 三档默认：

```ts
const AUTO: CachePolicyObject = { tools: true, system: true, messages: "latest-user-message" }
const NONE: CachePolicyObject = {}

const resolve = (policy: CachePolicy | undefined): CachePolicyObject => {
  if (policy === undefined || policy === "auto") return AUTO
  if (policy === "none") return NONE
  return policy
}
```

### 5.2 应用：只对 inline-hint 协议有效

`cache-policy.ts:42-43, 100`：

```ts
const RESPECTS_INLINE_HINTS = new Set(["anthropic-messages", "bedrock-converse"])
...
export const applyCachePolicy = (request: LLMRequest): LLMRequest => {
  if (!RESPECTS_INLINE_HINTS.has(request.model.route.id)) return request  // ← OpenAI/Gemini 跳过
  ...
}
```

原因：OpenAI 的 implicit prefix caching 不需要 hint，Gemini 用 `CachedContent` out-of-band。**只有 Anthropic 走 inline `cache_control: { type: "ephemeral" }` 协议**。

### 5.3 三个断点的算法

`cache-policy.ts:47-97`：

```ts
const markLastTool = (tools, hint) => {
  if (tools.length === 0) return tools
  const last = tools.length - 1
  if (tools[last]!.cache) return tools                              // ← 已设的不覆盖
  return tools.map((tool, i) => (i === last ? new ToolDefinition({ ...tool, cache: hint }) : tool))
}

const markLastSystem = (system, hint) => { ... 同模式 ... }

const markMessages = (messages, strategy, hint) => {
  if (messages.length === 0) return messages
  if (strategy === "latest-user-message") return markMessageAt(messages, lastIndexOfRole(messages, "user"), hint)
  if (strategy === "latest-assistant")   return markMessageAt(messages, lastIndexOfRole(messages, "assistant"), hint)
  const start = Math.max(0, messages.length - strategy.tail)
  let next = messages
  for (let i = start; i < messages.length; i++) next = markMessageAt(next, i, hint)
  return next
}
```

**关键设计**：
- **每个 part 最多一个 hint**（`if (target.content[markAt]!.cache) return messages`）——手动设置 hint 不会被自动覆盖。
- **不开新数组**（`messages.slice()` 替代 `messages.map()`）—— `cache-policy.ts:79-80` 注释明确说“Long conversations call this on every request, so avoid `.map()` here — its closure dispatch and identity copies show up in profiling.” 这种**性能意识**值得借鉴。
- **三段断点 = tool list 末尾 + system 末尾 + latest user message**——这覆盖了 agent 循环中**最常被重用的 prefix**（provider cache invalidation 顺序：tools → system → messages，Anthropic 20-block lookback 足够覆盖）。

### 5.4 Idempotency 保证

```ts
if (tools === request.tools && system === request.system && messages === request.messages) return request
return LLMRequest.update(request, { tools, system, messages })
```

**引用相等**判断：如果三个数组都没被改（全部已 cache），直接返回原 request。LLMRequest 应当是不可变的（`update` 返回新对象），所以这种身份比较是可靠的。

### 5.5 provider 兼容性表

| 协议 | 是否走 cache policy | 实际机制 |
|---|---|---|
| `anthropic-messages` | ✅ | `cache_control: { type: "ephemeral", ttl: "5m" \| "1h" }` |
| `bedrock-converse` | ✅ | 同上（Bedrock 透传 Anthropic） |
| `openai-completions` / `openai-responses` | ❌ | 自动 prefix caching，无需 hint |
| Gemini | ❌ | `CachedContent` API，out-of-band |
| 其他 | ❌ | skip |

### 5.6 对 synthia 的借鉴

```rust
pub enum CachePolicy {
    Auto,
    None,
    Object { tools: bool, system: bool, messages: MessagesPolicy, ttl_seconds: Option<u32> },
}
pub enum MessagesPolicy {
    LatestUser,
    LatestAssistant,
    Tail(u32),
}

pub fn apply_cache_policy(req: &mut LlmRequest) {
    if !matches!(req.model.route(), "anthropic-messages" | "bedrock-converse") { return; }
    let policy = resolve(&req.cache);
    let hint = CacheHint::ephemeral(policy.ttl_seconds);
    if policy.tools { mark_last_tool(&mut req.tools, hint); }
    if policy.system { mark_last_system(&mut req.system, hint); }
    if let Some(m) = policy.messages { mark_messages(&mut req.messages, m, hint); }
}
```

- 三档断点（tool list 尾 / system 尾 / latest user）直接复用。
- 5min TTL 在 Anthropic 是 **1.25x write, 0.1x read**——`cache-policy.ts:28-29` 注释里直接给了数学。
- 关闭 cache 的场景（`policy=none`）仍允许手动 `cache: CacheHint`——这种“自动 + 手动” 混合模式是优雅的。
- **幂等性**靠“reference equality”就够了，Rust 侧无需特别处理（`&mut` 模式下部分路径 no-op）。

---

## 6. Session 持久化与回放

opencode 的 session 是**事件溯源**模型（Event Sourcing）。`packages/core/src/session.ts`（主体服务）+ `session/event.ts`（事件定义）+ `session/projector.ts`（read-model 更新）+ `session/store.ts`（in-memory read cache）。

### 6.1 Session 的两个面

| 面 | 角色 | 存储 |
|---|---|---|
| **durable event log** | 不可变的事件流 | SQLite `event` + `event_sequence` 表 |
| **read model** | 投影后的 query 友好状态 | SQLite `session` + `session_message` 表 + in-memory `SessionStore` |

**write path**：
1. `session.prompt(input)` → 创建 `SessionInput.Admitted` event → `commit` 到 event log（事务）
2. 同一个事务触发 `projector` 把 `SessionInfo` 写到 `session` 表
3. 通知 pubsub + 同步处理器
4. `execution.wake(sessionID)` fork 一个 runner 跑 agent loop

**read path**：
- `get(id)` → 查 `SessionStore`（in-memory cache）→ miss 时查 DB → 投影历史事件到 `SessionInfo` → cache
- `events(afterCursor)` → `event.aggregateEvents({ aggregateID, after })` → 拼接历史 + 实时

### 6.2 SessionStore 的两阶段 cache

`session/store.ts`（未读取，但被 `session.ts:140-142` 引用）维护两个数据结构：
- `get(sessionID) → SessionInfo | undefined`：已投影到 read-model 的会话
- `message(messageID) → { sessionID, message } | undefined`：按 messageID 反查

**关键**：`Session.prompt` 时**先检查 `store.get(id)`**（`session.ts:202-203`）——如果 session 已存在（projection 已完成），直接返回，**避免重复 create**。

### 6.3 树形结构支持

`packages/core/src/session/schema.ts:25-49`：

```ts
export class Info extends Schema.Class<Info>("SessionV2.Info")({
  id: ID,
  parentID: ID.pipe(optionalOmitUndefined),   // ← 支持 session 树/分支
  projectID: ProjectV2.ID,
  ...
}) {}
```

**parentID** 字段允许**session fork**——比如 user 想要“从第 N 轮分叉一个新会话”。这种设计不需要额外表，只需要 read-model 多一列即可。

**对 synthia 的借鉴**：
- 即使 phase 0 不用 ES，也可以把 `parent_id` 存到 `Session` 表里。
- 子 session 的 event log 可以是独立的 aggregate（同 `aggregateID` 集合 vs 不同 ID）——具体看 fork 是“复制历史”还是“引用历史”。

### 6.4 Event Sourcing 下的并发处理

`session.ts:348-376` 的 `prompt()`：

```ts
prompt: Effect.fn("V2Session.prompt")((input) =>
  Effect.uninterruptible(
    Effect.gen(function* () {
      yield* result.get(input.sessionID)
      const returnPrompt = Effect.fnUntraced(function* (admitted: SessionInput.Admitted) {
        if (input.resume !== false) yield* enqueueWake(admitted)         // ← fork 异步执行
        return admitted
      }, Effect.uninterruptible)
      const messageID = input.id ?? SessionMessage.ID.create()
      const delivery = input.delivery ?? "steer"
      const expected = { sessionID: input.sessionID, messageID, prompt: input.prompt, delivery }
      const admitted = yield* SessionInput.admit(db, events, {...}).pipe(
        Effect.catchDefect((defect) =>
          defect instanceof SessionInput.LifecycleConflict
            ? new PromptConflictError({ sessionID: input.sessionID, messageID })
            : Effect.die(defect),
        ),
      )
      if (!SessionInput.equivalent(admitted, expected))
        return yield* new PromptConflictError({ sessionID: input.sessionID, messageID })  // ← 乐观锁
      return yield* returnPrompt(admitted)
    }),
  ),
),
```

要点：
- **`expected` 模式** = 乐观锁。admitted 事件如果不等于 client 期望，prompt 被拒绝（`PromptConflictError`）。
- **`uninterruptible`** 包裹整个 prompt——避免用户中断留下“已 admit 但未 enqueue”的中间态。
- **`enqueueWake` 是 `Effect.forkIn(scope, { startImmediately: true })`**——async 触发，不阻塞 API 返回。

### 6.5 replay 机制

`packages/core/src/event.ts:163-171, 453-516`：

```ts
readonly replay: (event: SerializedEvent, options?: { publish?: boolean; ownerID?: string; strictOwner?: boolean }) => Effect.Effect<void>
readonly replayAll: (events: SerializedEvent[], options?: ...) => Effect.Effect<string | undefined>
```

`replay` 用法：
- 启动时从 DB 拉历史 → 对每条调用 `replay` → 走正常 commit path → `beforeCommit` 跳过（`event.ts:279-307` 检测到 seq 已存在就 skip），projector 重新跑。
- 跨节点同步：新节点接管旧 aggregate → 用 `ownerID + strictOwner` 校验。

**对 synthia 的借鉴**：
- 即使不用 ES，也可以**为每个 session 存一个 `version: u64` + 一组 events**。phase 0 用 JSONL append-only 文件，phase 1 升级到 SQLite。
- **`strictOwner` + `claim` 模式**是**多 node 抢占**的关键——synthia 如果将来要支持分布式部署，现在就把 owner 字段加上，不要后期改。

### 6.6 树形 vs 平铺 session

opencode 的 session 是**单 root + parentID** 模式，不是“节点可以 fork 出多个并行子节点”的树。这是**有意为之**的简化：
- 一个 session 一次只能有一个 active runner（`run-coordinator.ts`，未读）。
- 子 session 只能通过 explicit `fork` 工具创建。
- `delivery: "steer" | "queue"` 决定新 prompt 是立即打断还是排到当前 turn 后。

**对 synthia 的借鉴**：synthia 可以一开始就用 `parent_id` 列，但 phase 0 不实现 fork，只暴露 `id` + `version`。

---

## 7. Effect-TS 抽象在 core 中的应用

`packages/core/test/lib/effect.ts`（共 53 行）只是测试 helper，但揭示了 Effect 在 opencode 中扮演的角色。真正的 Effect 应用遍布 `core/src/`。

### 7.1 Effect 解决了什么

| Effect 概念 | Rust 对应 | 解决的问题 |
|---|---|---|
| `Effect<A, E, R>` | `Future<Output=Result<A, E>> + R 是依赖类型` | 显式建模错误 + 依赖 |
| `Layer<L, E, R>` | 组合 `Provider` 的 trait object | 依赖注入 + 生命周期管理 |
| `Context.Service<Tag, Interface>` | `trait Tag { ... }` | 类型化服务标识 |
| `Effect.gen(function* () { ... yield* ... })` | `async fn` + `?` 运算符 | 结构化异步 + 自动错误传播 |
| `Effect.scoped` | RAII guard | 自动资源释放 |
| `Effect.addFinalizer` | `Drop` + cleanup | 显式释放 |
| `Effect.catchTag("Tag", handler)` | `Result::map_err` for specific error | 精确错误处理 |
| `Effect.catchCause` | `match error` | 错误类型分类 |
| `Effect.raceFirst(a, b)` | `tokio::select!` | 并发取最先 |
| `Effect.uninterruptibleMask(restore => ...)` | `tokio::select!` 但不取消 critical section | 关键路径不可中断 |
| `Effect.interrupt` | `CancellationToken::cancel` | 主动中断 |
| `Effect.forkIn(scope, ...)` | `tokio::spawn` | 后台执行 |
| `Effect.forkDaemon` | `tokio::spawn` + 守护 | 父取消不传播 |
| `Stream<A, E>` | `futures::Stream<Item=Result<A, E>>` | 异步序列 |
| `PubSub<A>` | `tokio::sync::broadcast` | 多订阅 pubsub |
| `Semaphore` | `tokio::sync::Semaphore` | 并发限制 |
| `FiberSet<A, E>` | `JoinSet<Result<A, E>>` | 有界并行任务集 |
| `Effect.withSpan("name", attrs)` | `tracing::instrument` | OTel 追踪 |
| `Effect.fn("name")` | `tracing::instrument` 的命名 | 调用栈可追踪 |

### 7.2 测试用 effect abstraction

`packages/core/test/lib/effect.ts:22-50`：

```ts
const make = <R, E>(testLayer: Layer.Layer<R, E>, liveLayer: Layer.Layer<R, E>) => {
  const effect = <A, E2>(name: string, value: Body<A, E2, R | Scope.Scope>, opts?: number | TestOptions) =>
    test(name, () => run(value, testLayer), opts)
  effect.only = ...
  effect.skip = ...
  const live = <A, E2>(name: string, value: Body<A, E2, R | Scope.Scope>, opts?: ...) =>
    test(name, () => run(value, liveLayer), opts)
  ...
  return { effect, live }
}

const testEnv = Layer.mergeAll(TestConsole.layer, TestClock.layer())
const liveEnv = TestConsole.layer

export const it = make(testEnv, liveEnv)
export const testEffect = <R, E>(layer: Layer.Layer<R, E>) =>
  make(Layer.provideMerge(layer, testEnv), Layer.provideMerge(layer, liveEnv))
```

**两条 test track**：
- **`it.effect(name, body)`** = 用 `TestClock + TestConsole`（虚拟时间 + 输出捕获），适合测时序逻辑。
- **`it.live(name, body)`** = 用真实时钟 + 真实 console，但其他 layer 还是 mock 的。

**对 synthia 的借鉴**：
- 同一个 layer 同时提供 `test_layer` 和 `live_layer`，二者只在最外层差异——Rust 侧可以用 `trait TestClock { fn now() -> Instant; }` 提供 `MockClock` 和 `SystemClock` 两种实现。
- `Layer.provideMerge` 是 “提供这些 service + 保留 layer 自己提供的 service”——Rust 侧用 `Arc<dyn Service>` + `tokio::task_local!` 模拟。
- **`Effect.fn("name")`** + `Effect.withSpan(...)` 是 OpenTelemetry trace 的核心——synthia 如果用 `tracing` + `tracing-subscriber` + OTLP，可以 1:1 对齐。

### 7.3 Effect 在 core 中的关键应用

#### 7.3.1 Tool 执行的不中断关键路径

`session/runner/llm.ts:259-280`：

```ts
yield* Effect.uninterruptibleMask((restore) =>
  restore(
    toolMaterialization.settle({...}),
  ).pipe(
    Effect.flatMap((settlement) =>
      publish(LLMEvent.toolResult({...}), settlement.outputPaths ?? []),
    ),
  ),
).pipe(FiberSet.run(toolFibers))
```

`uninterruptibleMask` 是 Effect 的“critical section”——用户中断在 restore 块**外**生效，块**内**完成。Rust 侧没有等价的语法糖，但可以用：

```rust
let tool_output = tokio::select! {
    biased;
    _ = cancellation_token.cancelled() => Err(ToolError::Interrupted),
    output = tool.run() => output,
}?;
// tool_output 一定不是 partial（cancel 不会进这里）
persist_tool_output(tool_output).await?;
```

#### 7.3.2 PubSub 风格的事件分发

`event.ts:184-213`：

```ts
const all = yield* PubSub.unbounded<Payload>()               // ← 全体事件
const synchronized = new Map<string, Set<PubSub.PubSub<void>>>()  // ← per-aggregate 唤醒
const typed = new Map<string, PubSub.PubSub<Payload>>()     // ← per-type
```

**对 synthia 的借鉴**：
- Rust 侧 `tokio::sync::broadcast` 等价于 `PubSub.unbounded`，但容量无界可能爆内存——synthia 应用 `broadcast::channel(capacity)`。
- `Stream.fromPubSub` 是**事件 + 历史拼接**的标准实现：`Stream::concat(historical, live)`，synthia 侧用 `futures::stream::iter(historical).chain(live)`。

#### 7.3.3 FiberSet = 有界并行

`session/runner/llm.ts:191`：

```ts
const toolFibers = yield* FiberSet.make<void, ToolOutputStore.Error>()
...
yield* Effect.uninterruptibleMask((restore) => restore(toolMaterialization.settle({...}))).pipe(
  FiberSet.run(toolFibers),
)
...
const settled = yield* restore(awaitToolFibers(toolFibers)).pipe(Effect.exit)
```

**`FiberSet`** = 一组 fiber，每条 tool call 一个，**自动追踪 + 全部完成后 join**。Rust 对应 `tokio::task::JoinSet`：

```rust
let mut tool_fibers: JoinSet<Result<ToolOutput, ToolError>> = JoinSet::new();
for call in pending_calls {
    tool_fibers.spawn(async move { tool.run(call).await });
}
while let Some(result) = tool_fibers.join_next().await {
    match result? { ... }
}
```

#### 7.3.4 失败分类 + 重试

`session/runner/llm.ts:285-338` 的 `runTurnAttempt` 用 `Effect.exit` 区分成功/失败/中断：

```ts
const stream = yield* restore(providerStream).pipe(Effect.exit)
const failure = stream._tag === "Failure" ? Option.getOrUndefined(Cause.findErrorOption(stream.cause)) : undefined
if (recoverOverflow && !publisher.hasAssistantStarted() && isContextOverflowFailure(overflowFailure ?? failure) && ...)
  return yield* Effect.die(continueAfterOverflowCompaction)
if (overflowFailure) yield* publish(overflowFailure)
const llmFailure = failure instanceof LLMError ? failure : undefined
if (llmFailure && !publisher.hasProviderError()) { ... fail unsettled tools ... }
if (stream._tag === "Failure" && Cause.hasInterrupts(stream.cause)) yield* FiberSet.clear(toolFibers)
...
```

**7 阶段错误处理**（基于 `Cause` 的精确分类）：
1. `Cause.hasInterrupts(cause)` → 中断
2. `isContextOverflowFailure(...)` → 触发 compaction
3. `instanceof LLMError` → 上报 LLM 错误
4. `isQuestionRejected(cause)` → question 被拒，停机
5. `stream._tag === "Failure" && !hasInterrupts` → 工具执行错误
6. `publisher.hasProviderError()` → 工具执行错误
7. `stream._tag === "Success" && !hasProviderError()` → 兜底 “provider did not return a tool result”

**对 synthia 的借鉴**：
- Rust 侧用 `enum ToolError { Interrupted, Overflow, QuestionRejected, Llm(LlmError), Execution(...), ProviderMissing }` + 分类 match。
- `Cause` 的语义是**“一个失败可能由多个并行原因组成”**（如 `Cause.parallel(...)`）——Rust 的 `JoinError` 在多 task join 时也有类似语义。

#### 7.3.5 同步机制

`session.ts:176-186` 的 `enqueueWake`：

```ts
const enqueueWake = (admitted: SessionInput.Admitted) =>
  execution.wake(admitted.sessionID, admitted.admittedSeq).pipe(
    Effect.tapCause((cause) =>
      Cause.hasInterruptsOnly(cause)
        ? Effect.void
        : logFailure("Failed to wake Session", admitted.sessionID, cause),
    ),
    Effect.ignore,
    Effect.forkIn(scope, { startImmediately: true }),
    Effect.asVoid,
  )
```

**`forkIn(scope, ...)`** 拿父 scope 跑 fiber，但 `startImmediately: true` 立即调度。`Effect.ignore` + `Effect.forkIn` = fire-and-forget。Rust 等价：

```rust
let _ = tokio::spawn(async move { execution.wake(session_id, admitted_seq).await; });
```

### 7.4 总结：Effect 对 synthia 最有价值的部分

不是 syntax 本身（Rust 没用 Effect 范式），而是这些**结构性**思想：

1. **每个服务 = `Context.Service<Tag, Interface> + Layer.effect(...)`**——Rust 用 `trait + Arc<Impl>`。
2. **`yield*` 显式声明依赖**——Rust 函数参数列就是依赖。
3. **`Effect.uninterruptibleMask` 保护关键路径**——Rust 用 `tokio::select!` 保护。
4. **`Scope.addFinalizer` 注册清理**——Rust 用 `Drop` + 显式 `close()` 包装。
5. **`Layer` 替换 = DI 容器**——Rust 用 `wireup` / `figment` 风格的 builder。
6. **`Effect.catchTag` 精确错误处理**——Rust 用 `Result::map_err` + `thiserror`。
7. **`FiberSet` 限并发**——Rust 用 `JoinSet` + `Semaphore`。
8. **`Effect.withSpan` OTel trace**——Rust 用 `tracing::instrument` + OTLP。
9. **`Schema` 派生类型**——Rust 用 `serde + schemars`。
10. **`Stream.concat(historical, live)` 拼接**——Rust 用 `futures::stream::iter(historical).chain(live)`。

---

## 8. Permission / Safety 模型

opencode V2 把 Permission 设计为**基于规则的静态评估 + 异步 deferred ask**。`packages/core/src/permission.ts` + `permission/schema.ts` + `permission/saved.ts`。

### 8.1 数据结构

`packages/core/src/permission/schema.ts:5-15`：

```ts
export const Effect = Schema.Literals(["allow", "deny", "ask"])
export const Rule = Schema.Struct({
  action: Schema.String,    // glob pattern: "bash", "edit", "*"
  resource: Schema.String,  // glob pattern: "/abs/path", "*.txt", "*"
  effect: Effect,           // allow | deny | ask
})
export const Ruleset = Schema.Array(Rule)
```

**三态 effect**（`allow` / `deny` / `ask`）—— 关键：**默认 effect 是 `ask`，不是 `deny`**。

### 8.2 评估算法

`packages/core/src/permission.ts:102-112`：

```ts
export function evaluate(action: string, resource: string, ...rulesets: Ruleset[]): Rule {
  return (
    rulesets
      .flat()
      .findLast((rule) => Wildcard.match(action, rule.action) && Wildcard.match(resource, rule.resource))
    ?? { action, resource: "*", effect: "ask" }   // ← 默认 ask
  )
}
```

**`findLast`**（不是 `find`）——**最新规则赢**，符合“最后写的规则覆盖前面的”直觉。

`Wildcard.match`（`packages/core/src/util/wildcard.ts:3-13`）实现 POSIX glob：

```ts
export function match(input: string, pattern: string) {
  const normalized = input.replaceAll("\\", "/")
  let escaped = pattern
    .replaceAll("\\", "/")
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".")
  if (escaped.endsWith(" .*")) escaped = escaped.slice(0, -3) + "( .*)?"
  return new RegExp("^" + escaped + "$", process.platform === "win32" ? "si" : "s").test(normalized)
}
```

**对 synthia 的借鉴**：glob 转 regex 是个一行式。Rust 侧可以用 `globset` crate 或自己写。

### 8.3 三类规则源

`packages/core/src/permission.ts:181-188`：

```ts
const evaluateInput = EffectRuntime.fnUntraced(function* (input: AssertInput) {
  const rules = yield* configured(input.sessionID, input.agent)         // ← 1. agent 内置 rules
  if (denied(input, rules)) return { effect: "deny" as const, rules }
  const all = [...rules, ...(yield* savedRules())]                       // ← 2. user 持久化的 saved rules
  const effects = input.resources.map((resource) => evaluate(input.action, resource, all).effect)
  const effect: Effect = effects.includes("deny") ? "deny"
                       : effects.includes("ask") ? "ask"
                       : "allow"
  return { effect, rules: all }
})
```

**两层 rule 合并**：
1. **agent 内置**——`agent.permissions`（`packages/core/src/agent.ts:30` 看到 `permissions: PermissionSchema.Ruleset` 字段）
2. **user 持久化**——`PermissionSaved` 服务（`packages/core/src/permission/saved.ts`），存到 `permission_saved` 表，跨 session 累积。

**多 resource 时合并策略**（`effects.includes` 优先级）：
- 任何 `deny` → 整体 `deny`
- 任何 `ask` → 整体 `ask`
- 全部 `allow` → 整体 `allow`

**对 synthia 的借鉴**：Rust 侧：

```rust
fn evaluate(input: &AssertInput, agent_rules: &Ruleset, saved_rules: &Ruleset) -> Effect {
    let all: Ruleset = agent_rules.iter().chain(saved_rules.iter()).cloned().collect();
    let effects: Vec<Effect> = input.resources.iter()
        .map(|r| find_last_match(&all, &input.action, r).effect)
        .collect();
    if effects.contains(&Effect::Deny) { Effect::Deny }
    else if effects.contains(&Effect::Ask) { Effect::Ask }
    else { Effect::Allow }
}
```

### 8.4 Fail-open vs Fail-closed

`packages/core/src/permission.ts:19, 173-188`：

```ts
const missingAgentPermissions: Ruleset = [{ action: "*", resource: "*", effect: "deny" }]
// ↑ 如果 agent 没有配置 rules，default = deny

function denied(input, rules) {
  return input.resources.some((resource) => evaluate(input.action, resource, rules).effect === "deny")
}

function evaluateInput(input) {
  const rules = yield* configured(input.sessionID, input.agent)
  if (denied(input, rules)) return { effect: "deny", rules }    // ← 第一关：deny 立即 return
  const all = [...rules, ...savedRules()]
  ...
  // 第二关：合并 saved rules，最严格的赢
  const effect: Effect = effects.includes("deny") ? "deny" : effects.includes("ask") ? "ask" : "allow"
  return { effect, rules: all }
}
```

**`configured` 的 fallback**（`permission.ts:163-171`）：

```ts
const configured = EffectRuntime.fn("PermissionV2.configured")(function* (sessionID, agentID?) {
  const session = yield* sessions.get(sessionID)
  if (!session) return yield* new SessionV2.NotFoundError({ sessionID })
  const agent = yield* agents.resolve(agentID ?? session.agent)
  return agent?.permissions ?? missingAgentPermissions
})
```

**对 synthia 的借鉴**：
- **Default = deny**——synthia phase 0 也可以这样（最严格）。
- Phase 1 可以加 `--dangerously-skip-permissions` CLI flag + `permissions: { default: "allow" }` config，**让用户主动选择**。
- **多 source 合并时“deny 优先”**——这种 fail-closed 行为符合安全直觉。

### 8.5 异步 Permission UX

`packages/core/src/permission.ts:202-243` 是 async ask 流程：

```ts
const create = (request: Request, agent?: AgentV2.ID) =>
  EffectRuntime.uninterruptible(
    EffectRuntime.gen(function* () {
      const deferred = yield* Deferred.make<void, RejectedError | CorrectedError>()
      const item = { request, agent, deferred }
      if (pending.has(request.id)) return yield* EffectRuntime.die(`Duplicate pending permission ID: ${request.id}`)
      pending.set(request.id, item)
      yield* events.publish(Event.Asked, request).pipe(
        EffectRuntime.onError(() => EffectRuntime.sync(() => pending.delete(request.id))),
      )
      return item
    }),
  )

const ask = EffectRuntime.fn("PermissionV2.ask")(function* (input) {
  const result = yield* evaluateInput(input)
  const value = request(input)
  if (result.effect === "ask") yield* create(value, input.agent)    // ← 创建一个待决项
  return { id: value.id, effect: result.effect }
})

const assert = EffectRuntime.fn("PermissionV2.assert")((input) =>
  EffectRuntime.uninterruptibleMask((restore) =>
    EffectRuntime.gen(function* () {
      const result = yield* evaluateInput(input)
      if (result.effect === "deny") return yield* new DeniedError({ rules: relevant(input, result.rules) })
      if (result.effect === "allow") return
      const item = yield* create(request(input), input.agent)      // ← 创建 deferred，挂起
      return yield* restore(Deferred.await(item.deferred)).pipe(   // ← 等待用户回复
        EffectRuntime.ensuring(
          EffectRuntime.sync(() => { pending.delete(item.request.id) }),
        ),
      )
    }),
  ),
)
```

**`Deferred<void, RejectedError | CorrectedError>`** = 单次信号量。`await(deferred)` 阻塞直到 `succeed` 或 `fail`。

`reply` 路径（`permission.ts:245-311`）：

```ts
const reply = EffectRuntime.fn("PermissionV2.reply")((input: ReplyInput) =>
  EffectRuntime.uninterruptible(
    EffectRuntime.gen(function* () {
      const existing = pending.get(input.requestID)
      if (!existing) return yield* new NotFoundError({ requestID: input.requestID })

      yield* events.publish(Event.Replied, { sessionID, requestID, reply })

      if (input.reply === "reject") {
        yield* Deferred.fail(existing.deferred, ...)
        pending.delete(input.requestID)
        for (const [id, item] of pending) {
          if (item.request.sessionID !== existing.request.sessionID) continue
          yield* events.publish(Event.Replied, { sessionID, requestID, reply: "reject" })
          yield* Deferred.fail(item.deferred, new RejectedError())    // ← 一次 reject = 整个 session 全部 reject
          pending.delete(id)
        }
        return
      }

      if (input.reply === "always" && existing.request.save?.length) {
        yield* saved.add({ projectID, action, resources })            // ← "always" 持久化
      }
      yield* Deferred.succeed(existing.deferred, undefined)
      pending.delete(input.requestID)
      
      // ← 关键：always 之后，**遍历 pending**，看其它请求是否能用新 saved rules 自动放行
      if (input.reply !== "always" || !existing.request.save?.length) return
      const rememberedRules = yield* savedRules()
      for (const [id, item] of pending) {
        ...
        if (!item.request.resources.every(...)) continue
        yield* events.publish(Event.Replied, { sessionID, requestID, reply: "always" })
        yield* Deferred.succeed(item.deferred, undefined)             // ← 自动放行 pending
        pending.delete(id)
      }
    }),
  ),
)
```

**三件事**值得借鉴：
1. **一次 reject = 整个 session 全部 reject**——降低用户决策成本（`for (const [id, item] of pending) if same session → fail`）。
2. **`always` 自动 re-evaluate pending**——保存一条新 rule 后，重新评估所有挂起的请求，**无需用户逐个确认**。
3. **`uninterruptibleMask` 包裹 reply**——避免 reply 处理被打断留下 orphan pending。

**对 synthia 的借鉴**：
- Rust 侧用 `tokio::sync::oneshot` 实现 `Deferred`。
- "always 自动 re-evaluate pending" 是**最聪明的设计**——synthia 应当 1:1 复用。
- `onError` cleanup + `Effect.ensuring` 清理 map entry——Rust 用 RAII + `try { ... } finally { map.remove(id) }`。

### 8.6 Permission Saved (跨 session)

`packages/core/src/permission/saved.ts`（未读，但 `permission.ts:275-280` 调用）：

```ts
if (input.reply === "always" && existing.request.save?.length) {
  yield* saved.add({
    projectID: location.project.id,
    action: existing.request.action,
    resources: existing.request.save,
  })
}
```

`save` 字段是 request 里**用户想要持久化的 resource 模式**——比 `resources`（本次操作的）更窄（避免一次性 save 整个 `*`）。

**对 synthia 的借鉴**：
- synthia 持久化 rule 时要区分**操作 resource** 和**save 模式**——避免一次写 "allow bash *" 后所有 bash 都被允许。
- `projectID` 隔离不同项目——synthia 用 `project_root` 作为 key。

### 8.7 Permission 与 Policy 的关系

opencode V2 有两个**不同**的 service：

| Service | 范围 | 默认 |
|---|---|---|
| `PermissionV2` | 用户/agent 交互（ask + always/once/reject） | deny |
| `Policy` | 静态规则（无交互） | fallback 由调用方提供 |

`packages/core/src/policy.ts:7, 18-19`：

```ts
export const Effect = Schema.Literals(["allow", "deny"])  // ← 只有 allow/deny，无 ask
...
readonly evaluate: (action: string, resource: string, fallback: Effect) => EffectRuntime.Effect<Effect>
```

**Policy 是"无 UI 反馈的纯规则"**——比如 `config.policy` 里的硬编码规则（CI 模式）。Permission 是"有 UI 反馈的规则"——比如用户态的"always allow bash"。**两者**通常一起用：先 Policy（无成本快速过滤），再 Permission（必要时才 ask）。

**对 synthia 的借鉴**：
- phase 0 只用 `Permission`（带 UI），policy 可以后加。
- 当 Permission 出现 "ask" 决策时，**先看 Policy**（可能 Policy 给了 deny → 直接 fail），再 query 用户。

---

## 9. 可扩展性总结：什么让添加新功能“廉价”

跨 7 大组件分析后，**opencode 让扩展便宜的设计模式**可归纳为 6 条：

### 9.1 Schema 派生的强类型契约

几乎每个扩展点（Tool、Hook、Event、Permission、Agent）都用 Effect `Schema` 描述 shape，**TypeScript 类型由 Schema 派生**。
→ 加新事件只需 1 行 `EventV2.define({ type, schema })`，下游所有 subscriber 自动类型对齐。
→ synthia 应当**全程用 `serde + schemars` + custom derive**（如 `derive(Schema)`）。

### 9.2 WeakMap / Stack-based override

Tool 注册是 `Map<name, Vec<{ token, registration }>>`（LIFO），加 plugin 只需 push 到 stack，scope 退出自动 pop。
→ synthia 应当把"registerable 资源"统一设计成 `Arc<Registry<T>>` + `RegistrationToken: Drop` 模式。

### 9.3 Layered DI

`Layer.effect(Service, ...).pipe(Layer.provide(...))` 让每个服务独立测试、独立替换。
→ synthia 用 `trait` + `Arc<dyn Service>` + builder pattern 即可。

### 9.4 Event Sourcing = "添加新 read-model 不改 write path"

想加“token 用量统计”？新加一个 projector 订阅 `step.ended` event，写到 `token_stats` 表。**原有 write path 一行不动**。
→ synthia 即使 phase 0 不用 ES，也要预留 `Event` 抽象 + subscriber。

### 9.5 Lazy `_definition` 缓存

`tool.ts:68-79` 的 `runtimes.definition(name)` 第一次调用构造 JSON schema + name，后续直接返回缓存。**多协议适配延迟到首次使用**。
→ synthia 用 `OnceCell<JsonSchema>` 即可。

### 9.6 Effect Scope = 隐式生命周期

每个 plugin 一个 `Scope.fork`，每个 tool registration 一个 finalizer——**不需要写 cleanup 代码**，scope 退出时全跑。
→ synthia 用 `Drop` + `tokio_util::CancellationToken` 模拟 scope 层级。

---

## 10. synthia Rust 借鉴清单（按优先级）

按 **"影响大 + 实施难度低"** 排序：

### P0 — 必须做（核心骨架）

| 借鉴 | opencode 来源 | Rust 实现建议 |
|---|---|---|
| **强类型 Tool trait** | `packages/llm/src/tool.ts:48-69` | `trait Tool { type Input: Deserialize; type Output: Serialize; fn description(&self) -> &str; fn to_json_schema(&self) -> JsonSchema; fn execute(&self, input: Self::Input, ctx: &ToolContext) -> impl Future<Output=Result<Self::Output, ToolError>>; fn to_model_output(&self, output: &Self::Output) -> String; fn to_structured_output(&self, output: &Self::Output) -> serde_json::Value; }` |
| **ToolContext 完整字段** | `packages/plugin/src/tool.ts:3-27` | `pub struct ToolContext { session_id, message_id, agent, directory, worktree, abort: CancellationToken, ask: Box<dyn Ask>, report_progress: ... }` |
| **stack-based tool registry** | `core/src/tool/registry.ts:47-104` | `pub struct ToolRegistry { inner: Arc<RwLock<HashMap<String, Vec<Registration>>>> }`，`register(tools) -> RegistrationToken { Drop { unregister; } }` |
| **Permission rule + 3-state effect** | `permission/schema.ts:5-15` | `enum Effect { Allow, Deny, Ask }` + `struct Rule { action: String, resource: String, effect: Effect }` + `glob` crate |
| **Session event log (append-only)** | `core/src/event.ts:96-133, 255-379` | `pub struct EventLog { path: PathBuf }` + `pub async fn append(&self, event: &Event) -> Result<u64>` + `pub async fn read_after(&self, aggregate_id, after: u64) -> Result<Vec<Event>>` |
| **Effect-类 service DI** | `core/src/agent.ts:74-141`, `core/src/event.ts:175` | `trait AgentService: Send + Sync { ... }` + `Arc<dyn AgentService>` + `let agent: Arc<dyn AgentService> = ctx.get();` |

### P1 — 重要（半年内）

| 借鉴 | opencode 来源 | Rust 实现建议 |
|---|---|---|
| **Cache policy: auto / none / object** | `llm/src/cache-policy.ts` + `schema/options.ts:190-220` | `enum CachePolicy { Auto, None, Object { tools, system, messages, ttl_seconds } }` + `fn apply_cache_policy(req: &mut LlmRequest)` |
| **3 breakpoints (tool tail / system tail / latest user)** | `cache-policy.ts:47-97` | 直接 1:1 翻译 |
| **Layered Permission (Policy → Permission)** | `core/src/policy.ts` + `core/src/permission.ts` | 拆 `PolicyService` (无 UI) + `PermissionService` (有 UI) |
| **`always` 自动 re-evaluate pending** | `core/src/permission.ts:284-308` | `for pending: if (evaluate(pending, all_rules) == Allow) { oneshot.send(Ok(())) }` |
| **一次性 reject = 整个 session 全部 reject** | `core/src/permission.ts:262-271` | `for pending in same session: fail all oneshots` |
| **Event Sourcing projector pattern** | `core/src/event.ts:653-658` | `trait Projector { fn on_event(&self, event: &Event) -> impl Future<Output=Result<()>>; }`，订阅器在事务内调用 |
| **Plugin Scope = Drop** | `core/src/plugin.ts:92-181` | `trait Plugin { fn register(self: Box<Self>, registry: &mut PluginRegistry) -> Registration; }`，`Drop` 里 unregister |
| **Hook input/output 模式** | `core/src/plugin.ts:23-65`, `plugin/src/index.ts:222-335` | `trait Hook<Input, Output> { fn invoke(&self, input: &Input, output: &mut Output); }`，用 `&mut Output` 让 plugin 改 output |
| **`dispose` lifecycle** | `plugin/src/index.ts:223` | `trait Plugin { async fn dispose(&self) {} }` 默认 no-op |

### P2 — 增值（一年内）

| 借鉴 | opencode 来源 | Rust 实现建议 |
|---|---|---|
| **Session `parentID` 支持 fork** | `session/schema.ts:25-49` | `Session { id, parent_id: Option<SessionId>, ... }` |
| **Ephemeral vs Durable event 分类** | `core/src/event.ts:29-35, 500` | `enum EventKind { Durable { aggregate, version }, Ephemeral }` |
| **`versionedType` 字段** | `core/src/event.ts:81-83` | `format!("{}.{}", type, version)`，DB 存 `${type}.${version}` |
| **Delta vs Ended 双事件** | `session/event.ts:236-244, 247-257` | `text.delta` (ephemeral) + `text.ended` (durable) |
| **Tool `toModelOutput` + `toStructuredOutput` 双输出** | `llm/src/tool.ts:48-54` | 同上 |
| **Provider-executed tool** (e.g. OpenAI code-interpreter) | `runner/llm.ts:256` | `enum ExecutionMode { Local, Provider }` |
| **Prompt `delivery: "steer" \| "queue"`** | `core/src/session.ts:357` | enum + 队列实现 |
| **`promptCacheKey` 注入** | `runner/llm.ts:218-222` | `req.provider_options.openai.prompt_cache_key = session.id.to_string();` |
| **OTel `withSpan` 模式** | `core/src/plugin.ts:113-117, 153-159` | `#[tracing::instrument(skip_all, fields(plugin_id, hook))]` |
| **`Effect.catchTag` 精确错误** | `runner/llm.ts:174-176` | `ToolError::Interrupted` / `ToolError::Stale` / `ToolError::SchemaMismatch` 等 |
| **TestClock + TestConsole** | `core/test/lib/effect.ts:44-50` | `trait Clock { fn now() -> Instant; }` + `MockClock` 实现 |

### P3 — 远期（需要时再做）

| 借鉴 | opencode 来源 | 备注 |
|---|---|---|
| **Sync (跨进程事件推送)** | `core/src/event.ts:639-646` | 需要 gRPC / WebSocket bridge |
| **`strictOwner` 多节点抢占** | `core/src/event.ts:271-278, 529-536` | 分布式部署时才需要 |
| **`Effect.uninterruptibleMask`** | `runner/llm.ts:259-280` | 已有 `tokio::select!` 替代，但需明确语义 |
| **TUI plugin 系统** | `packages/plugin/src/tui.ts` | synthia 不一定需要 TUI，先做 server plugin |
| **`Effect.forkIn(scope, ...)`** | `session.ts:184` | 已有 `tokio::spawn` |
| **FiberSet** | `runner/llm.ts:191` | 已有 `tokio::task::JoinSet` |
| **Vignette post-process** | `tui-smoke.tsx:991-995` | 纯 UI 概念，不影响 core |
| **Route + Slot + Command + Keymap plugin** | `plugin/src/tui.ts:455-486, 519-521` | 纯 UI 概念 |

---

## 附录 A：核心文件:行号 速查表

| 主题 | 文件 | 行号 |
|---|---|---|
| Plugin interface (server) | `packages/plugin/src/index.ts` | 56-335 |
| Plugin tool context | `packages/plugin/src/tool.ts` | 3-27 |
| Plugin TUI interface | `packages/plugin/src/tui.ts` | 581-634 |
| Plugin example | `packages/plugin/src/example.ts` | 1-17 |
| BunShell type | `packages/plugin/src/shell.ts` | 1-136 |
| Tool core (V2) | `packages/core/src/tool/tool.ts` | 20-138 |
| Tool registry | `packages/core/src/tool/registry.ts` | 41-130 |
| Built-in tools (Location layer) | `packages/core/src/tool/builtins.ts` | 31-44 |
| Bash tool example | `packages/core/src/tool/bash.ts` | 107-205 |
| Tools.Service (narrow interface) | `packages/core/src/tool/tools.ts` | 6-13 |
| LLM Tool (protocol-agnostic) | `packages/llm/src/tool.ts` | 48-69, 133-165, 221-249 |
| Tool runtime (dispatch) | `packages/llm/src/tool-runtime.ts` | 23-77 |
| Cache policy resolver | `packages/llm/src/cache-policy.ts` | 18-110 |
| Cache policy schema | `packages/llm/src/schema/options.ts` | 190-220 |
| LLM system / message parts | `packages/llm/src/schema/messages.ts` | 6-100 |
| Event bus (V2) | `packages/core/src/event.ts` | 96-678 |
| Session events | `packages/core/src/session/event.ts` | 50-510 |
| Session service | `packages/core/src/session.ts` | 105-422 |
| Session schema | `packages/core/src/session/schema.ts` | 12-49 |
| Session runner (LLM loop) | `packages/core/src/session/runner/llm.ts` | 90-401 |
| Permission service | `packages/core/src/permission.ts` | 45-327 |
| Permission schema | `packages/core/src/permission/schema.ts` | 5-15 |
| Policy (static) | `packages/core/src/policy.ts` | 7-45 |
| Plugin core (V2) | `packages/core/src/plugin.ts` | 14-181 |
| Agent (V2) | `packages/core/src/agent.ts` | 20-141 |
| State (immer + scope) | `packages/core/src/state.ts` | 55-111 |
| Layer-node (cycle detection) | `packages/core/src/effect/layer-node.ts` | 25-100 |
| Test effect helper | `packages/core/test/lib/effect.ts` | 1-53 |
| Wildcard matcher | `packages/core/src/util/wildcard.ts` | 3-13 |
| Real plugin (TUI smoke) | `.opencode/plugins/tui-smoke.tsx` | 1-1017 |

## 附录 B：5 条最值得记住的 opencode 设计

1. **`Schema 派生类型 + Schema.Class` 让 event/permission/tool 都自动获得 JSON 编码 + 类型安全 + OTel trace。** Rust 端用 `serde + schemars` 1:1 复刻。
2. **每个 plugin 一个 Effect `Scope`，`Effect.addFinalizer` 跑 cleanup。** Rust 用 `Drop` 实现 RAII。
3. **Tool registry 是 stack-based override + scope lifecycle。** 加新 tool 不改原有代码。
4. **Event Sourcing 拆分 durable（写 DB）vs ephemeral（仅 PubSub）vs delta（仅增量推送）。** 持久化与实时订阅完全解耦。
5. **Cache policy 默认 "auto" 三断点（tool tail / system tail / latest user），但不覆盖手动 `CacheHint`。** 自动+手动混合模式既减少用户决策，又给专家留控制口。
