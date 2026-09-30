## Why

Synthia 当前 Agent 系统存在三个核心差距：(1) Agent 定义硬编码无法用户扩展，(2) 权限系统缺乏层级覆盖机制，(3) 缺乏多 Agent 控制平面。这导致用户无法自定义 Agent、无法细粒度控制权限、无法构建多 Agent 协作流程。相比 OpenCode（Markdown 文件定义 Agent）和 Codex（层级式多 Agent 控制），Synthia 在生产级功能上存在明显差距。现在实施这些增强，可以将 Synthia 提升到与 OpenCode/Codex 同等的生产级水平。

## What Changes

**文件式 Agent 定义**
- From: Agent 定义硬编码在 Rust 模块中
- To: 支持 `.agents/agents/<id>.md` Markdown 文件 + YAML frontmatter 定义 Agent
- Reason: 用户可自定义 Agent，无需修改代码
- Impact: 非破坏性变更，既有大发开式 API 全部保持

**多层权限合并**
- From: 单层权限配置（默认或查表），无覆盖机制
- To: 三层合并（Default → Agent → User），支持 `allow/deny/ask` 三种动作
- Reason: 安全默认 + Agent 可定制 + 用户可覆盖
- Impact: 向后兼容，旧 TOML 配置继续工作

**多 Agent 控制平面**
- From: 单 Agent 架构，无层级结构
- To: AgentPath 寻址、Mailbox 通信、SpawnReservation RAII、CompletionWatcher
- Reason: 支持子 Agent 协作、层级管理、资源限制
- Impact: 新增控制平面 API，不影响现有单 Agent 路径

## Capabilities

### New Capabilities
- `file-based-agent`: 支持 Markdown 文件定义 Agent，含 frontmatter 配置、extends继承、热重载
- `permission-merge`: 三层权限合并引擎，含 pattern 匹配、Ask 流程复用 Guardian
- `multi-agent-control`: 多 Agent 控制平面，含 AgentPath、AgentRegistry、Mailbox、CompletionWatcher
- `fork-policy`: 子 Agent Fork 策略，支持消息历史和权限的5+4 种组合

### Modified Capabilities
- `agent-config`: 增量扩展字段（permission_rules、permission_default、tools、denied_tools、extends、mode 等）
- `stream-builder`: 新增 StepSpawn 步骤、AgentRunConfig 新增可选 agent_control 字段

## Impact

**新增模块**:
- `synthia-agent/src/agent_file/` — AgentFileLoader、frontmatter 解析、loader
- `synthia-permission/src/rule.rs` — PermissionRule、MergedPolicy
- `synthia-agent/src/control/` — AgentControl、AgentRegistry、Mailbox、SpawnReservation

**修改模块**:
- `synthia-agent/src/lib.rs` — 导出新模块
- `synthia-agent/src/agent.rs` — AgentRunConfig 扩展
- `synthia-agent/src/stream_builder/` — StepSpawn 集成

**工作量**:
- P0 文件式 Agent 定义: 5.5 人天
- P0 多层权限合并: 5.5 人天
- P1 多 Agent 控制平面: 14-19 人天（4 phases）