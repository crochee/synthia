## Context

Synthia 是一个 Rust workspace 形式的 AI Agent 运行时，核心设计原则记录在 `.trae/rules/agent_rule.md`（P1-P10），强调 KV Cache 前缀一致性、Append-Only 上下文、渐进降级、系统级不信任、可观测性优先。近期完成的 `p0-subagent-execution-session-persistence` 已让子 Agent 会话具备持久化能力，但执行底盘（tool execution、approval、sandbox）仍处于半完成状态。

通过对比 OpenCode（TypeScript/Bun，Schema/Effect-TS 驱动）与 Codex（Rust/120+ crates，生产级沙箱与 Turn 状态机），发现 Synthia 在以下方面存在真实差距：
- `Permission::RequireConfirm` 没有异步确认路径，实际等效于 deny。
- `BashTool` 裸执行，无 OS 级沙箱。
- 工具执行层职责分散在 `ToolRegistry`/`EnhancedToolDispatcher`/`ToolExecutor`。
- 核心文件/编辑/搜索工具仍是 stub。
- ReAct 主循环深陷 `async_stream!` 宏，难以测试与中断。

本次变更聚焦把执行底盘做扎实，而不是一次性重写整个 Agent 架构。

## Goals / Non-Goals

**Goals:**
1. 建立单一 `ToolOrchestrator` 入口，统一工具调度的审批、沙箱、执行、超时、重试、取消、恢复。
2. 让 `Permission::RequireConfirm` 具备真正的异步审批生命周期，并支持 session 级缓存。
3. 在 Linux 上实现基于 bubblewrap + landlock/seccomp 的沙箱执行层，平台不可用时 fail-closed。
4. 补齐核心文件/编辑/搜索工具，支持结构化 patch 与流式进度事件。
5. 保持 P1-P10 原则：system prompt 不变、append-only、渐进降级、系统级不信任。

**Non-Goals:**
1. 不一次性把 ReAct 主循环重写为 Codex 式 Turn 状态机（本次只要求 `ToolOrchestrator` 接口能支撑未来迁移）。
2. 不实现 macOS/Windows 沙箱后端（仅定义抽象，Linux 后端优先）。
3. 不实现网络访问审批与托管代理（保留在后续变更）。
4. 不改写 `synthia-memory` 的语义检索与 learning 机制。

## Decisions

### D1：采用单一 `ToolOrchestrator` 入口，而非继续修补三层分发器
- **选择**：新建 `synthia-tool-orchestrator` crate，定义 `ToolOrchestrator` trait 与默认实现。所有工具调用（内置、MCP、子 Agent）最终都通过它。
- **理由**：Codex 的 `ToolOrchestrator` 已被验证能把 approval → sandbox → exec → escalation 统一为一条可观测管线；Synthia 当前三层重复实现是导致取消/重试/沙箱策略不一致的根因。
- **已考虑 alternative**：
  - 仅合并 `EnhancedToolDispatcher` 与 `ToolExecutor`：会保留 `ToolRegistry` 中的权限检查，审批与执行仍分离。
  - 在 `ToolRegistry` 中直接加沙箱逻辑：会让注册表继续膨胀，违反单一职责。

### D2：异步审批通过 `ApprovalService` trait 注入，而非在 `ToolRegistry` 中硬编码
- **选择**：`ToolOrchestrator` 接收 `Arc<dyn ApprovalService>`，在 `Permission::RequireConfirm` 时调用 `request_approval(tool, args, policy)` 异步等待，结果写入 `ApprovalStore`。
- **理由**：解耦审批 UI（CLI/Server 实现不同）与核心执行逻辑；保持 fail-closed（超时/取消/服务不可用 = deny）。
- **已考虑 alternative**：
  - 在 `synthia-permission` 中直接阻塞等待用户输入：会引入 IO 依赖，难以在 server 场景使用。
  - 使用全局 channel：破坏可测试性与多 session 隔离。

### D3：沙箱作为 `ToolOrchestrator` 的执行后端，而非 `BashTool` 内部逻辑
- **选择**：新增 `synthia-sandbox` crate，提供 `SandboxManager::select(policy) -> SandboxAttempt`，`ToolOrchestrator` 调用 `SandboxAttempt::wrap(command)` 生成受控执行命令。
- **理由**：沙箱策略应与工具类型解耦；未来 file read/write、MCP、子 Agent 都可能需要沙箱。
- **已考虑 alternative**：
  - 仅在 `synthia-tool-bash` 中内嵌 bwrap 调用：无法复用到其他工具，也难以支持 macOS/Windows 后端。

### D4：Linux 后端优先使用 bubblewrap，landlock/seccomp 作为可选加固
- **选择**：Linux `SandboxType::Bubblewrap` 使用 `bwrap` 二进制限制文件系统命名空间；`SandboxType::Landlock`/`Seccomp` 在 bwrap 基础上通过 feature flag 叠加。
- **理由**：bubblewrap 无需内核补丁，部署简单，是 Codex 也采用的路径；landlock/seccomp 提供更细粒度控制但实现复杂度更高。
- **已考虑 alternative**：
  - 直接用 landlock 作为唯一后端：Linux 5.13+ 才稳定支持，且 Rust landlock crate 成熟度有限。
  - 用容器（Docker/Podman）：引入 daemon 依赖，与 Synthia 的轻量设计冲突。

### D5：核心文件工具采用"临时副本 → hunk 应用 → 校验 → 原子替换"四步模型
- **选择**：`write_file`/`apply_patch` 先写到 `.synthia/tmp/`，校验成功后再 `rename` 原子覆盖原文件。
- **理由**：避免模型 patch 失败直接污染工作区；与 Codex `apply_patch` 的流式进度事件对齐。
- **已考虑 alternative**：
  - 直接覆盖原文件：实现简单但风险高，一旦 LLM 输出错误难以恢复。

### D6：保持现有 `AgentEvent` 不变，新增内部 `ToolOrchestratorEvent` 用于调试
- **选择**：`ToolOrchestrator` 内部事件不直接暴露为外部协议，只通过新增字段/变体补充必要的 `AgentEvent`。
- **理由**：避免一次性重构事件系统；本次聚焦执行底盘。
- **已考虑 alternative**：
  - 同时分层 `AgentEvent`：范围过大，会拖延执行底盘改造。

## Risks / Trade-offs

[Risk] 沙箱后端不可用时可能意外导致大量命令被拒绝，影响可用性。
→ Mitigation: 平台不可用时默认 deny；提供配置项 `sandbox.on_unavailable = deny|prompt`，prompt 时走 `ApprovalService` 让用户显式选择无沙箱执行。

[Risk] 异步审批路径引入死锁或长时间阻塞，导致 Agent 会话卡死。
→ Mitigation: `ApprovalService` 必须带超时（默认 5 分钟）；超时按 deny 处理；CLI/Server 必须支持取消等待。

[Risk] 统一 `ToolOrchestrator` 后，若接口设计不当会成为所有工具扩展的瓶颈。
→ Mitigation: trait 设计预留 `extension` 字段；MCP 工具、子 Agent 工具以 plugin 形式接入，不修改 `ToolOrchestrator` 核心。

[Risk] 文件工具原子替换与现有 truncate/spill 到磁盘机制交互复杂。
→ Mitigation: 原子替换只针对工作区内文件；大文件 spill 逻辑保留在 `synthia-context`，不进入工具层。

[Trade-off] 综合变更范围较大，评审与实现周期更长。
→ 接受理由：Orchestrator/审批/沙箱/文件工具在实现上高度耦合，拆分会制造接口版本碎片；内部以 Phase 0/1/2 分阶段落地保持可控。

[Trade-off] macOS/Windows 沙箱本次不实现。
→ 接受理由：Synthia 当前主要运行环境为 Linux；抽象层先定义，后端可后续填充。

## Migration Plan

1. **Phase 0：接口与基线**
   - 创建 `synthia-tool-orchestrator` 与 `synthia-sandbox` crate。
   - 定义 `ToolOrchestrator`、`ApprovalService`、`SandboxManager` trait。
   - 保留旧 `ToolRegistry` 入口作为垫片，确保现有测试通过。

2. **Phase 1：核心能力落地**
   - 实现 `ApprovalService` 默认内存实现 + `ApprovalStore`。
   - 实现 Linux bubblewrap 后端。
   - 补齐 `core-file-editing-tools`。
   - 将 `BashTool`、`read_file`、`write_file`、`apply_patch` 迁移到 `ToolOrchestrator`。

3. **Phase 2：CLI/Server 集成与清理**
   - CLI 实现基于终端的 `ApprovalService`。
   - Server 实现基于 WebSocket/HTTP 的 `ApprovalService`。
   - 移除 `EnhancedToolDispatcher` 与 `ToolExecutor` 的重复逻辑。
   - 更新文档与示例。

**Rollback 策略**：每个 phase 结束后保留可回退的 commit；`ToolOrchestrator` 通过 feature flag 启用，旧路径作为 fallback。

**验收条件**：
- `cargo clippy --all-targets --all-features --tests --all` 全绿。
- 危险命令首次执行触发审批 UI。
- Linux 沙箱内无法读取工作区外文件。
- 文件 patch 失败不污染原文件。

## Open Questions

1. `ApprovalService` 的默认实现是否需要在无 UI 环境下自动拒绝（headless mode）？
2. `synthia-sandbox` 的 landlock/seccomp 后端是否在本次范围内实现，还是仅预留接口？
3. `ToolOrchestrator` 是否需要支持工具级并发策略（并行/串行），还是沿用全局 semaphore？
4. 是否需要在本次变更中同步移除 `EnhancedToolDispatcher` 与 `ToolExecutor`，还是保留为垫片到下一变更？
