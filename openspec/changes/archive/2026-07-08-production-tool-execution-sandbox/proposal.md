## Why

Synthia 的 P1-P10 设计原则在理念层面非常先进，但工具执行、权限审批与沙箱隔离三条关键路径的实现明显落后于生产级 Agent（OpenCode/Codex）。当前 `Permission::RequireConfirm` 被静默拒绝、`BashTool` 裸执行、`ToolRegistry`/`EnhancedToolDispatcher`/`ToolExecutor` 职责重叠、`file_tools` 等核心工具仍是 stub。这些缺口使 Agent 无法安全、可靠、可观测地完成编码任务。本次变更旨在把执行底盘提升到生产级，同时保持 Synthia 的 KV Cache 前缀一致性与 Append-Only 原则。

## What Changes

**工具执行入口**
- From: 工具调用分散在 `ToolRegistry`、`EnhancedToolDispatcher`、`ToolExecutor` 三层，超时/重试/并发逻辑重复。
- To: 引入单一 `ToolOrchestrator` 作为所有工具调用的统一入口，负责注册发现、权限审批、沙箱选择、执行、超时、重试、取消、结果聚合。
- Reason: 消除职责重叠，为审批、沙箱、可观测性提供单一埋点。
- Impact: 非破坏性；旧入口逐步垫片化后废弃。

**异步审批生命周期**
- From: `Permission::RequireConfirm` 在 `ToolRegistry::run_with_context` 中被直接映射为 error，用户从未被询问。
- To: 新增 `ApprovalService` trait 与默认实现，`RequireConfirm` 调用该服务异步等待用户/Guardian 决策，结果缓存到 `ApprovalStore`。
- Reason: fail-closed 原则正确，但当前 UX 为零；需要真正的授权路径。
- Impact: 需要 CLI/Server 层实现审批 UI；超时/取消/服务不可用路径必须回归 deny。

**跨平台沙箱执行**
- From: `BashTool` 直接 `bash -c` 启动子进程，仅依赖字符串黑名单。
- To: 新增 `synthia-sandbox` crate，提供 `SandboxManager` + `SandboxType` 抽象；Linux 优先实现 bubblewrap + landlock/seccomp 后端。
- Reason: 字符串黑名单可被编码绕过，必须引入 OS 级隔离。
- Impact: 平台沙箱不可用时默认 deny，禁止静默降级。

**核心文件/编辑/搜索工具**
- From: `file_tools.rs`、`system_tools.rs`、`search_tools.rs` 为 TODO stub。
- To: 补齐结构化读、写、文本替换、patch 应用、搜索工具，支持流式进度事件与原子替换。
- Reason: 没有这些工具，Agent 无法完成基础编码任务。
- Impact: 新增工具注册；旧 stub 直接替换。

## Capabilities

### New Capabilities
- `tool-orchestrator`: 统一工具执行入口，聚合审批、沙箱选择、执行、超时、重试、取消、恢复。
- `async-approval-service`: 异步审批生命周期与按 session 的审批缓存。
- `cross-platform-sandbox`: 跨平台沙箱抽象，Linux 优先实现 bwrap/landlock/seccomp 后端。
- `core-file-editing-tools`: 核心文件/编辑/搜索工具，支持结构化 patch 与流式进度事件。

### Modified Capabilities
- 无。本次变更不修改现有 spec 能力的行为契约，仅新增能力。

## Impact

- **代码层面**：新增 `synthia-tool-orchestrator`、`synthia-sandbox` crate；改造 `synthia-tool`、`synthia-tool-bash`、`synthia-permission`、`synthia-agent`。
- **API 层面**：工具调用结果格式保持不变；新增 `ApprovalService` 与 `SandboxManager` trait 供 CLI/Server 实现。
- **依赖层面**：Linux 沙箱依赖 `bubblewrap` 二进制与可选的 `liblandlock`/`libseccomp`；通过 feature flag 控制。
- **UX 层面**：CLI/Server 需要接入审批请求 UI；危险命令首次执行会弹出确认。
