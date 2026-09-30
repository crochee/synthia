## Why

Synthia 的 agent runtime 与生产级 AI agent（opencode / codex / pi-mono）相比有结构性差距：Tool 抽象薄弱（12 方法挤在一个 trait）、AgentMessage 缺少 `llm_visible()` 视图抽象、react_loop 之外的能力被迫嵌入主循环而非作为 Tool 暴露。这些差距在每个新能力（subagent / permission gating / tool search）增加时都会放大。现在处理是因为：(1) Phase 1 是 10 个 PR × ≤ 3 天的渐进交付，风险可控；(2) 用户已选择全面对标 + Tool 化方向；(3) 综合报告已锁定范围。预期收益：Tool 抽象现代化、AgentMessage 解耦、核心 loop 不变前提下的可扩展性。

## What Changes

### Tool Trait 拆分

- From: `Tool` trait 有 12 个方法（definition / execution / lifecycle 混杂）
- To: 拆为 `ToolDefinition` + `ToolExecution` + `ToolLifecycle` 三个 sub-trait，每个 ≤ 5 方法
- Reason: 责任清晰；实现者可按需选择 sub-trait
- Impact: **Non-breaking**（保留 `ToolV1` alias 2 minor 版本）；5 个 Tool 实现需小幅适配

### AgentMessage 视图抽象

- From: `AgentMessage` / `MessageRole` 直接 enum 分支，内部状态与 LLM 视野耦合
- To: 加 `fn llm_visible(&self) -> bool` method + `MessageKind` enum 区分（system / user / assistant / tool_call / tool_result）
- Reason: 借鉴 pi-mono 最强烈推荐；让 LLM 视野 vs 内部状态解耦
- Impact: **Non-breaking**（新增 method + enum）

### Tool Registry 双索引

- From: `HashMap<String, Arc<dyn Tool>>` 单索引
- To: HashMap + `Vec<ToolMetadata>` 双索引
- Reason: lookup 加速 O(1)；Vec 顺序保留 LLM 视野稳定 sequence
- Impact: **Non-breaking**（内部重构）

### Provider 三层架构

- From: Agent 直接持有 LLM client
- To: Provider trait 抽象，Agent 通过 Provider 调用 LLM
- Reason: opencode 验证；跨 Provider 共用 reaction loop
- Impact: **Non-breaking**（最小可用 Provider trait）

### CompactionTool 抽象

- From: `compress` 内置于 react_loop，不可替换
- To: `CompressionTool` trait + `DefaultCompactionTool` 默认实现 + 注入点
- Reason: 符合 Progressive Toolification 原则
- Impact: **Non-breaking**（默认实现保留）

### AgentTool Factory Wiring

- From: `AgentTool` 已实现 + 已注册，但 factory 未串
- To: factory 调用 AgentTool 注册到主流程
- Reason: 修复既有 gap（实现已存在）
- Impact: **Non-breaking**（仅 wiring）

### `_xxx` 字段清理

- From: `AgentRunConfig` 11 个 `_xxx` 字段被静默丢弃
- To: 删除 / 重命名 / 启用全部字段
- Reason: silent-drop 是代码 smell，违反 observability 原则
- Impact: **可能 breaking**（取决于哪些字段启用 vs 删除）

### Tool Permission Trait

- From: 无 permission 抽象
- To: `ToolPermission` trait（借鉴 codex C 模式）
- Reason: 为未来 sandbox / approval 留接口
- Impact: **Non-breaking**（默认 PermissionAlwaysAllow）

## Capabilities

### New Capabilities

- `tool-trait-decomposition`: Tool trait 拆分为 Definition / Execution / Lifecycle 三个 sub-trait；提供 `ToolV1` alias 保持向后兼容
- `agent-message-view`: AgentMessage 加 `llm_visible()` 方法 + `MessageKind` enum 区分消息种类
- `tool-registry-dual-index`: ToolRegistry 双索引（HashMap + Vec<ToolMetadata>），O(1) lookup 与顺序遍历
- `provider-trait`: Provider trait 抽象层，让 Agent 不直接持有 LLM client
- `compression-tool`: CompactionTool trait + DefaultCompactionTool 默认实现 + 注入点
- `tool-permission`: ToolPermission trait 定义 + 默认 PermissionAlwaysAllow 实现
- `agent-tool-wiring`: AgentTool factory wiring（修复既有 gap）
- `config-field-cleanup`: `_xxx` 字段清理（11 个 AgentRunConfig 静默丢弃字段处理）

### Modified Capabilities

- (none — 本 change 不修改既有 spec，所有改动通过 new capability 表达)

## Impact

### Affected Code

- `crates/synthia-agent/src/`: `Tool` trait、`AgentMessage`、`AgentRunConfig`、`react_loop`
- `crates/synthia-tools/src/`: `ToolRegistry`、`AgentTool`
- `crates/synthia-llm/src/`: 新增 `Provider` trait
- `crates/synthia-session/src/`: 可能微调以适配 Provider trait
- 共 ~5 个 Tool 实现需适配 sub-trait

### APIs

- 新增：3 个 sub-trait、`MessageKind` enum、`Provider` trait、`CompressionTool` trait、`ToolPermission` trait
- 修改：`Tool` trait（保留 alias）、`AgentMessage`（加 method）、`AgentRunConfig`（清理字段）
- 不破坏 semver（minor 版本内）

### Dependencies

- 无新增外部依赖
- 内部 crate 间依赖：synthia-agent → synthia-llm（Provider trait）

### Systems

- 不涉及部署变更（纯库内重构）
- 不涉及 DB schema 变更
- 不涉及 endpoint 变更
- 不涉及 telemetry schema 变更（synthia-telemetry 已做）

### Migration

- Phase 1: 10 个 PR × ≤ 3 天 = 6 周（详见 design.md Migration Plan）
- 每个 PR 独立分支、独立可回滚
- Feature flag：`ToolV1` alias 保留 2 minor 版本