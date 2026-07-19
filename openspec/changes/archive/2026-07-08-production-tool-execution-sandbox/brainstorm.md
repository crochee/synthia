<!--
Raw capture of brainstorming / exploration output.

本檔原樣捕捉 brainstorming / openspec-explore 的產出，不強制結構。
design.md 從本檔萃取並重新整理為結構化設計文件。
-->

# Brainstorm: 生产级工具执行与沙箱改造

## 背景

对 Synthia 与 OpenCode（/home/crochee/workspace/opencode）、Codex（/home/crochee/workspace/codex）进行多专家对抗性代码分析后，发现 Synthia 的 P1-P10 设计原则（KV Cache 前缀一致性、Append-Only、渐进降级、系统级不信任等）在理念层面非常先进，但实现层存在明显缺口，尤其是工具执行、权限审批与沙箱隔离三条关键路径。

## 关键发现摘要

### 上下文维度
- Synthia 的 `PrefixTracker` 只监控部分 system prompt，未覆盖完整 `ContextAssembler::system_snapshot`。
- 缺少 Codex 的 `reference_context_item` 差分上下文注入机制。
- 压缩实现与“三阶段 pruning”设计不对齐。

### 安全/权限维度
- `Permission::RequireConfirm` 被 `ToolRegistry` 直接映射为 error，没有真正的异步确认路径。
- `BashTool` 直接 `bash -c` 启动子进程，无操作系统级沙箱。
- 权限规则只有工具名字符串匹配，无 action/resource 维度。
- 无 `ApprovalStore` 缓存用户决策。
- `synthia-guardian` 未接入工具执行路径。

### 工具/执行维度
- `ToolRegistry` / `EnhancedToolDispatcher` / `ToolExecutor` 职责重叠。
- `cancel_token` 被显式忽略，工具不可取消。
- `file_tools.rs` / `system_tools.rs` / `search_tools.rs` 仍是 TODO stub。
- 无复用 PTY/进程池，每次 bash 新建进程。
- MCP 未深度集成。
- 子 Agent resume 路径未贯通。

### 架构维度
- ReAct 主循环深陷 `async_stream::stream!` 宏，控制流散落。
- `recovery_cascade` L3-L5 职责模糊。
- `Session` 是简单数据容器，非权威状态容器。
- `AgentEvent` 单一巨型枚举，内外事件未分层。

## 决策链

### Q1：本次分析的目标是什么？
**选择：B. 基于分析启动 OpenSpec 变更提案。**

### Q2：是否认同 P0 排序？
**选择：认同。**
P0 排序：
1. 修复 `RequireConfirm` 异步审批路径
2. 补齐 `file_tools` / `system_tools` / `search_tools` stub
3. 把 ReAct 主循环抽取为显式 Turn 状态机
4. 统一工具执行层为 `ToolOrchestrator`

### Q3：沙箱策略倾向？
**选择：A. 优先引入 Codex 式平台沙箱（Linux bwrap/landlock 先）。**

### Q4：是否沉淀为 OpenSpec proposal / 设计文档？
**选择：需要。**

## 变更切分取舍

### 选项 A：综合变更——生产级工具执行与沙箱
- 覆盖统一 ToolOrchestrator + 异步审批 + 跨平台沙箱（Linux 先）+ 补齐核心文件工具。
- 优点：一次把执行底盘做扎实，依赖关系在内部管理。
- 风险：变更范围大，需要清晰子任务拆分。

### 选项 B：两个变更——先 Orchestrator+审批，后沙箱
- 优点：依赖清晰，分阶段评审。
- 缺点：沙箱是用户优先级 A，但需等待前置变更完成。

### 选项 C：三个独立变更
- 缺点：管理开销大，且 Orchestrator/审批/沙箱 三者高度耦合。

### 选项 D：只先做一个最小的 PoC
- 缺点：无法直接解决 P0 中的审批、取消、工具 stub 等阻塞问题。

## 最终决策

**选择选项 A：单个综合变更 `production-tool-execution-sandbox`。**

理由：
1. P0 中的异步审批、统一 Orchestrator、补齐核心工具、平台沙箱在实现上高度耦合——沙箱需要 Orchestrator 的统一入口，Orchestrator 需要审批状态机，审批又需要文件/沙箱等危险工具作为首批接入对象。
2. 单个综合变更便于在 design/spec 阶段统一接口边界，避免三个独立变更之间的接口版本碎片化。
3. 内部拆分为多个阶段（Phase 0/1/2）和子任务，仍保持可控粒度。

## 设计方向（初步）

1. 引入 `ToolOrchestrator` 作为所有工具调用的统一入口，聚合审批、沙箱选择、执行、超时、重试、取消、恢复。
2. 将 `Permission::RequireConfirm` 改为真正的异步等待路径，接入 `ApprovalService` 与 `ApprovalStore`。
3. 新增 `synthia-sandbox` crate，提供跨平台沙箱抽象，Linux 优先实现 bubblewrap + landlock/seccomp 后端。
4. 补齐核心文件/编辑/搜索工具，支持结构化 patch 与流式进度事件。
5. 保持 Synthia 的 P1-P10 原则：system prompt 不变、append-only、渐进降级、系统级不信任。
