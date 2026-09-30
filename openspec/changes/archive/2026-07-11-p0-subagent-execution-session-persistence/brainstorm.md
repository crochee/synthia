<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。
不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，兩者互補但不重疊。
-->

# Brainstorming: Synthia vs OpenCode vs Codex 架构差距分析

## 背景

用户要求分析 synthia 与生产级 AI agent（opencode 和 codex）的架构差距，特别是 opencode。采用多专家对抗性分析，确保真实性。

## 分析范围

三个项目代码库：
- **Synthia**: Rust monorepo，29 crates，~136K 行 Rust 代码
- **OpenCode**: TypeScript/Bun monorepo，Effect-TS 架构，SQLite 存储
- **Codex**: Rust monorepo，~120 crates，~998K 行 Rust 代码

## 专家团队组建

| 专家 | 领域 | 偏好 |
|------|------|------|
| Agent Loop Architect | 循环设计、错误恢复、状态管理 | 健壮性与可恢复性 |
| Context Economist | 上下文管理、压缩、token 预算 | 信息密度最大化 |
| Sub-Agent Engineer | 多智能体、任务委派、隔离 | 可扩展性与隔离性 |
| Security Auditor | 权限、沙箱、循环检测 | 纵深防御 |

## 决策链：四个辩论回合

### Q1: 优先分析哪个层面？

用户选择：**架构设计层面**（选项 1），以 opencode 为主要对标对象。

### Q2: 聚焦哪些优先级？

经过四轮专家独立分析 + 对抗性辩论，确定了三个优先级层：

**P0 — 严重差距（关键路径）：**
1. 子智能体执行未实现 — AgentTool 创建实例但不执行
2. 无持久会话状态 — LoopContext 7 个字段仅内存

**P1 — 重要差距：**
3. 无并行工具执行
4. 压缩摘要结构化程度较低
5. 无尾部轮次保护
6. 无压缩后自动继续

**P2 — 优化：**
7. Token 预算无增长范围
8. 无操作系统级沙箱

用户选择：**聚焦 P0**。

### Q3: 子智能体采用哪种模式？

**Option A: OpenCode 的前台/后台模式（推荐）**
- 前台：spawn + await 结果，RPC 风格
- 后台：spawn + 立即返回，结果通过 inject() 注入父会话
- 优点：简单直观，UX 好
- 缺点：需要 BackgroundJob 基础设施

**Option B: Codex 的 actor 模式**
- spawn + 父 agent 主动 poll/wait
- 优点：更灵活，父 agent 可以中途发送新消息
- 缺点：更复杂，父 agent 需要显式管理子 agent 生命周期

**决策：采用 Option A（OpenCode 模式），辅以 Codex 的配置继承。**

理由：
- Synthia 已有 ForkPolicy 设计（6 种上下文分叉策略），与 Codex 的配置继承互补
- OpenCode 的 foreground/background 二分更简单，适合当前阶段
- 可通过 `send_message` 工具后续添加 actor 模式交互

### Q4: 会话持久化采用哪种存储？

**决策：扩展 SessionMetadata（JSON）+ 新增 SessionInput JSONL 队列。**

理由：
- Synthia 已有 `messages.jsonl` + `metadata.json` 持久化基础
- 引入 SQLite 增加复杂度（依赖 sqlx），JSON 已足够
- SessionInput 队列用 append-only JSONL，与现有架构一致
- 新增字段使用 `serde(default)` 保证向后兼容

## 子智能体系统详细设计

### 现状

Synthia 有两套并行的子智能体系统，都未完成：

1. `registry/` 系统：`AgentInstance` 有 `session`、`token_budget`、`state`，但 `AgentToolWrapper::call()` 是存根
2. `tools/agent_tools/` 系统：`AgentTool::call()` 创建实例后立即返回 `"Waiting for result..."`
3. `AgentControl` 在 `AgentRunConfig` 中被忽略（`agent_control: _`）
4. `Mailbox::send_message()` 是显式存根（"Phase 5 才会接线"）

### 目标架构

```
AgentTool::call()
  ├── 解析参数: description, prompt, subagent_type, background, model, tools, fork_policy
  ├── 构建子智能体配置 (Codex 模式):
  │   ├── 继承父 AgentRunConfig (model, provider, token_budget)
  │   ├── 继承父 permission_policy (降级为 User 层)
  │   ├── 应用 ForkPolicy 过滤历史消息
  │   └── 覆盖 subagent_type 指定的 tools/denied_tools
  ├── 创建子 Session + 子 AgentRunConfig
  ├── 分叉执行:
  │   ├── foreground (默认): spawn + await 结果
  │   └── background: spawn + 立即返回 "running"
  └── 结果回传:
      ├── foreground: 直接返回 ToolOutput
      └── background: 通过 Mailbox 注入父会话
```

### 关键设计决策

1. **统一 AgentInstance 类型** — 合并两套 AgentInstance 为单一类型，包含执行所需全部字段
2. **执行桥接** — 新增 `run_subagent(instance) -> JoinHandle<AgentResult>`，内部调用 `StreamBuilder::run_stream_with_state()`
3. **结果通道** — 每个子智能体实例附带 `oneshot::Sender<AgentResult>`
4. **后台注入** — 结果通过 `Mailbox` 作为合成用户消息注入父会话
5. **深度限制** — `parent_depth + 1 <= max_depth`（默认 1）
6. **并发限制** — 新增 `max_concurrent_subagents` 配置（默认 6）

## 会话持久化详细设计

### 现状

`LoopContext` 有 7 个字段完全不持久化：
- `end_reason` — 进程崩溃后不知道会话是否完成
- `cumulative_tokens` — token 使用量在 `metadata.json` 中有 per-call 记录但无累计值
- `recent_tool_results` — 循环检测用，仅内存
- `needs_compact` — 瞬态标记
- `context_token_limit` — 硬限制
- `current_turn_id` — 当前转向 ID
- `span_ctx` — 追踪上下文

转向 channel 是纯内存 `tokio::mpsc`。

### 目标架构

```
SessionMetadata (扩展)
  ├── 现有字段: version, id, owner_user_id, state, token_usage, created_at, updated_at, config, message_count
  └── 新增字段 (serde(default)):
      ├── end_reason: Option<SessionEndReason>
      ├── iteration: usize
      ├── cumulative_tokens: usize
      └── context_token_limit: Option<usize>

SessionInputQueue (新增)
  └── session_input.jsonl (append-only)
      ├── push(session_id, content, delivery)
      ├── drain_pending(session_id) -> Vec<SessionInput>
      ├── has_pending(session_id) -> bool
      └── promote(id) // 标记为已消费
```

### 关键设计决策

1. **`serde(default)` + `..Default::default()`** — 向后兼容旧 session 文件
2. **最小化写入** — 只持久化恢复执行所需的状态，瞬态字段从历史重建
3. **SessionInput 替代内存 channel** — 转向消息写入 JSONL，`drain_steering()` 改为从文件读取
4. **Agent 恢复路径增强** — `resume()` 从 `SessionMetadata` 恢复 `iteration` 和 `end_reason`

## 对 OpenCode 和 Codex 的借鉴

从 OpenCode 借鉴：
- 前台/后台子智能体执行模式
- `inject()` 后台结果注入模式
- 8 部分压缩摘要模板（P1 阶段）
- SessionInput 持久输入队列模式

从 Codex 借鉴：
- 配置快照继承（model, provider, permission, sandbox, cwd）
- AgentControl 基于 watch channel 的状态订阅
- BodyAfterPrefix token 预算范围（P2 阶段）

## Synthia 特有优势（保留）

这些是 Synthia 优于 OpenCode/Codex 的方面，不在此次变更中修改：
- 五层循环检测（LoopDetectorSet）
- 五级错误恢复（L1-L5 级联）
- 凭证守卫（12 个正则表达式模式）
- 注入扫描器（15 个正则表达式模式）
- PBAC 系统（正式策略引擎）
- 前缀稳定性追踪（PrefixTracker）
- 自我反思（每 5 次迭代）
- ForkPolicy 设计（6 种上下文分叉策略）