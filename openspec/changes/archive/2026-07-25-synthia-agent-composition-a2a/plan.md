# Synthia Agent-Composition-A2A Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** 以 agent_as_tool 为唯一原语重构 Synthia Agent 架构，集成 A2A 协议实现跨 agent 标准通信，补全 Multi-Agent 模式层。

**Architecture:** AgentHandle（无状态句柄）与 AgentSession（私有状态）正交分离，agent_as_tool() 纯函数将 agent 包成 Tool，A2A 协议（a2a-lf）实现本地+远程 agent 互操作，Generator-Verifier / Workflow / Transfer 从 agent_as_tool() 自然组合，Interceptor Chain 统一横切关注点。

**Tech Stack:** Rust, a2a-lf / a2a-client-lf / a2a-server-lf, tokio, async-trait, dashmap

---

## Task 1: AgentHandle / AgentSession 分离

**Files:** `crates/synthia-agent/src/handle.rs`, `crates/synthia-agent/src/session.rs` (new), `crates/synthia-agent/src/agent_instance.rs` (modify)

- 定义 AgentHandle struct（从 Agent struct + AgentInstance 提取能力字段）
- 定义 AgentSession struct（从 AgentInstance.session + AgentRunConfig 提取状态字段）
- 定义 LoopState struct
- AgentInstance = type alias 过渡
- AgentRunConfig 精简（移除重叠字段）
- 迁移所有使用点
- **Verify:** cargo test -p synthia-agent

## Task 2: agent_as_tool() 纯函数

**Files:** `crates/synthia-agent/src/a2t.rs` (new), `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs` (modify)

- 实现 agent_as_tool() 纯函数
- 实现 AgentTool struct + Tool trait
- AgentTool::call() 创建新 Session → handle.run() → ToolOutput
- 旧 AgentTool 内部委托新实现
- **Verify:** cargo test -p synthia-agent — agent_as_tool

## Task 3: synthia-a2a crate

**Files:** `crates/synthia-a2a/` (new crate), `Cargo.toml` (workspace add)

- 创建 crate，添加 a2a-lf 依赖
- 实现 A2aTransport, SynthiaA2aHandler, AgentCard 构建
- A2aTransport::serve() / discover()
- agent_output_to_a2a_stream() 转换
- **Verify:** cargo test -p synthia-a2a

## Task 4: SendMessage / SendMessageStream Tool

**Files:** `crates/synthia-a2a/src/tools.rs` (new)

- SendMessageTool + Tool trait（A2A 同步通信）
- SendMessageStreamTool + Tool trait（A2A 流式通信）
- A2A tool 自动注册
- **Verify:** cargo test -p synthia-a2a — send_message

## Task 5: Multi-Agent 模式层

**Files:** `crates/synthia-agent/src/patterns/` (new directory)

- orchestrate() / orchestrate_remote()
- GeneratorVerifier struct + run()
- Workflow struct + run()
- transfer_bidirectional()
- **Verify:** cargo test -p synthia-agent — patterns

## Task 6: AgentExecutor trait 统一

**Files:** `crates/synthia-agent/src/executor.rs` (new)

- AgentExecutor trait（run + resume）
- AgentStreamExecutor: AgentExecutor trait
- AgentHandle impl
- 删除 run_stream_with_state
- RunConfig 精简
- **Verify:** cargo test -p synthia-agent — executor

## Task 7: Interceptor Chain 统一

**Files:** `crates/synthia-agent/src/interceptor.rs` (new), `crates/synthia-agent/src/interceptors/` (new directory)

- Interceptor trait + InterceptorEvent enum
- InterceptorChain struct + dispatch()
- TraceInterceptor, ApprovalInterceptor, RetryInterceptor, CompactInterceptor, LoopDetectInterceptor
- HookBuilder deprecated 迁移
- **Verify:** cargo test -p synthia-agent — interceptor

## Task 8: 清理与删除

**Files:** Multiple deletions across crates/synthia-agent/

- 删除 SubagentManager, SlotGuard, InMemoryMessageBus, MessageBus
- 删除 TeamCreateTool, TeamDeleteTool, HandoffTool, SendMessageTool(旧)
- 删除 AgentCoordinator(旧), HookBuilder deprecated, EnhancedToolDispatcher
- 删除 AgentInstance type alias, run_stream_with_state
- cargo clippy + cargo fmt
- **Verify:** cargo test --workspace && cargo clippy --all-targets --all-features --tests --all

---

## Execution Order

Tasks 1-2 must be sequential (agent_as_tool depends on AgentHandle).
Task 3-4 can run in parallel after Task 1 (synthia-a2a depends on AgentHandle but not agent_as_tool).
Task 5 depends on Tasks 2 + 4 (patterns use agent_as_tool + SendMessage).
Task 6-7 can run in parallel after Task 1 (trait + interceptor don't depend on each other).
Task 8 is final cleanup after all other tasks.

```
T1 → T2 → T5
T1 → T3 → T4 → T5
T1 → T6
T1 → T7
T8 (after all)
```
