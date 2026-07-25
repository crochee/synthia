<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: Synthia Agent-Composition-A2A 架构融合

## 背景

参考微信文章《AI Agent框架从0-1设计》的核心设计原则：
1. **Less is more** — 核心概念少而正交，新概念先问"能否是已有概念的推论"
2. **能力与状态分离** — Agent 是无状态推理句柄，AgentSession 是私有状态
3. **组合优于继承** — 通过简单粘合层和正交概念组合出各种业务模式

当前 Synthia 架构差距：
- AgentInstance 混合了能力(definition)和状态(session)
- AgentRunConfig 与 Agent 字段重叠（tool_registry, hook_registry, session_store 重复）
- SubagentManager 是独立概念，不是 agent_as_tool 的推论
- 缺少 Generator-Verifier、Workflow、对称转移模式
- HandoffTool 是单向消息，非对称转移
- TeamCreateTool/DeleteTool 只有名义分组，无执行语义
- Run/Resume 有三条入口路径，缺显式 trait 边界
- 审批走 ApprovalService 独立路径，重试走 ToolOrchestrator，不是统一 Interceptor

## 决议链

### Q1: 整体架构方向？

三种方案评估：
- A: 渐进精修 — 改动最小但根因未解
- B: Trait-First 重设计 — 接口清晰但可能过度抽象
- **C: 组合优先架构** — 以 agent_as_tool 为唯一原语，模式层自然涌现

**决议：方案 C**

### Q2: Sub-agent 概念是否保留？

用户明确指示：**以 agent as tool 为唯一原语，消灭 sub-agent 概念**。

- SubagentManager 职责分散到 AgentHandle + ToolOrchestrator
- AgentTool(task) 替换为 agent_as_tool() 纯函数
- 深度限制 → ToolOrchestrator 执行策略
- 并发控制 → ToolOrchestrator 已有
- 子树取消 → CancellationToken 层级

### Q3: Multi-Agent 通信方式？

用户指示：**通过 A2A 协议（a2a-lf 库）实现多 agent 交互**。

三种 Tool 覆盖所有场景：
1. **AgentTool** = agent_as_tool() — 本地 in-process
2. **SendMessage** — A2A 同步通信
3. **SendMessageStream** — A2A 流式通信

A2A 协议优势：
- Agent 可同时作为 A2A Client 和 Server
- 本地 agent: in-process call
- 远程 agent: HTTP/gRPC A2A
- AgentCard 能力发现
- Task 状态管理

### Q4: Interceptor 统一方式？

统一为 Interceptor Chain（中间件模式）：
- TraceInterceptor → 替代 OTel wrapper
- ApprovalInterceptor → 替代独立 ApprovalService 路径
- RetryInterceptor → 替代 ToolOrchestrator 中重试逻辑
- CompactInterceptor → 替代独立 compaction 步骤
- LoopDetectInterceptor → 适配现有 LoopDetector

### Q5: Run/Resume 接口？

两条路径，显式 trait 边界：
- `AgentExecutor::run()` — 无状态单发
- `AgentExecutor::resume()` — 有状态续跑
- `AgentStreamExecutor: AgentExecutor` — 流式扩展

消除 `run_stream_with_state`，统一为 `resume_stream`。

## 删除清单

| 删除 | 替代为 |
|------|--------|
| AgentInstance | AgentHandle + AgentSession |
| SubagentManager | AgentHandle + ToolOrchestrator |
| AgentTool (task, 旧) | agent_as_tool() — AgentTool (新) |
| TeamCreateTool / TeamDeleteTool | 删除 |
| HandoffTool | SendMessage (A2A) |
| SlotGuard | 删除 |
| HookBuilder (deprecated) | Interceptor Chain |
| EnhancedToolDispatcher | RetryInterceptor |
| InMemoryMessageBus | A2A transport (a2a-lf) |
| AgentCoordinator (旧) | ToolRegistry + agent_as_tool() |

## 依赖

a2a-lf crate 族（已在 crates.io 可用）：
- a2a-lf = "0.3" — A2A v1 protocol types
- a2a-client-lf = "0.2" — A2A v1 async client
- a2a-server-lf = "0.4" — A2A v1 async