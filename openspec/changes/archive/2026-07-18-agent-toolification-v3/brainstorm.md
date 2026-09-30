<!--
Raw capture of brainstorming output for openspec change agent-toolification-v3.

Brainstorming 已通过 5 个对抗性专家 agent + 6+ 轮澄清问题完成。
本檔原樣捕捉決策鏈、設計取捨、未決問題。

design.md 將從本檔萃取並重新整理為結構化設計文件。
不要將本檔的內容複製到 design.md — 兩者互補但不重疊。
-->

# Brainstorm — agent-toolification-v3

**Date**: 2026-07-17
**Context**: Synthia 是一个 Rust AI Agent 框架（21 crate，~7k LOC），用户希望让 agent 逻辑与生产级 agent（opencode / codex / pi-mono）对齐，并按"tool-centric"重构。
**Trigger question**: "我想跟市面上 AI agent 对齐设计...除了主逻辑 react loop 和 session 之外，其他功能尽量抽象为 tool 实现"

---

## §1 Background（已知状态）

### 1.1 当前架构（Synthia）

- **Crate 结构**（21 个）：synthia / synthia-agent / synthia-llm / synthia-tools / synthia-session / synthia-telemetry / synthia-config / ...
- **核心实体**：
  - `AgentRunConfig`：丢弃 11 个 `_xxx` 字段（baseline 漏报 2 个：`_prompt_template_name`, `_model_id`）
  - `AgentMessage` / `MessageRole`：无 `llm_visible()` 抽象
  - `Tool` trait：实际 12 方法（非 7）
  - `AgentTool`：**已实现 + 已注册**，但 factory 未串（gap 是 wiring，不是实现）
- **核心机制**：
  - `react_loop`：每步 `should_continue()` + `act()` + `observe()` + `compress()`
  - `Session`（OrphanRule 已修）
  - `Tool` registry（HashMap-based）
  - `compress`：核心能力之一（PI 历史中提到）
  - MCP client 完整实现（348 行审查）

### 1.2 对标项目

| Project | 特点 | 可借鉴 | 不借鉴 |
|---------|------|--------|--------|
| opencode | 控制面清晰、tool 化激进、并发强 | Tool/Session/Provider 三层架构、llm_visible 抽象、Event 流 | 30+ overloads、ts 全栈包袱 |
| codex | Rust 血统、安全模型深、AppServer 架构 | VFS / sandbox / approval 模型、SubscriptionGate | V8 引擎直搬、Session 格式二进制 |
| pi-mono | 极简（30 overloads）、TS 单一栈 | AgentMessage + llm_visible()（最强烈推荐） | 30 overloads、`wrap_output_with_otel` task-local |

---

## §2 Decision Chain（5 轮澄清问题）

### Q1 — 范围

**问**：你想做哪类对齐？

- A. 全面对标 + Tool 化
- B. 仅借鉴特定模式（控制在 3-5 个）
- C. 只做核心机制对齐（react_loop + Session）
- D. 评估型（先出报告）

**用户选择**：**A. 全面对标 + Tool 化**

### Q2 — 深度

**问**：每个模式要给到多深？

- A. 仅概念 + 借鉴点
- B. 概念 + Trait 草案
- C. 概念 + Trait + 测试骨架
- D. 完整实现

**用户选择**：**B. 概念 + Trait 草案**（避免过度工程化）

### Q3 — 对抗维度

**问**：想听哪种反方意见？

- A. 仅"为什么不这样做"
- B. A + 项目间冲突
- C. B + 性能影响
- D. C + 类型契约
- E. 全部 5 项（最彻底）

**用户选择**：**E. 全部 5 项**

→ 决定派出 5 个对抗性专家 agent。

### Q4 — 输出格式

**问**：报告以什么形式交付？

- A. Markdown 综合报告
- B. 文档站（report.html）
- C. OpenSpec change proposal
- D. 仅 PRD + Epic 分解

**用户选择**：**A. Markdown 综合报告**

### Q5 — 后续动作

**问**：综合报告后做什么？

- A. 进入 OpenSpec change proposal
- B. 直接动手实现 Phase 1
- C. 暂时存档
- D. 退回到 explore 模式

**用户选择**：**A. 进入 OpenSpec change proposal**（当前正在执行）

---

## §3 五专家对抗结论

### 3.1 Opencode 借鉴（opencode-control-plane-patterns.md, 292 行）

8 个可借鉴模式：
1. **Tool/Session/Provider 三层分离** — Agent 不直接持有 LLM
2. **llm_visible() 抽象** — `AgentMessage::llm_visible()` 让内部状态与 LLM 视野解耦
3. **Event 流（Stream + subscriber）** — 控制面/数据面分离
4. **ToolRegistry 的 HashMap + Vec 索引** — 双索引加速 lookup
5. **AgentTool factory 模式** — 缺 wiring
6. **SubscriptionGate** — 控制面开关（codex 强项）
7. **Provider trait 抽象** — 跨 Provider 共用 reaction loop
8. **CancellationToken 贯穿 Session → Tool** — 取消可观察

### 3.2 Codex 差异化（codex-vs-opencode-design.md, 458 行）

7 个 A-G 模式：
- **A**：Subscription 模型（OAuth 优先）— **不学**（我们靠 API key）
- **B**：VFS sandbox — **可选**（看部署需求）
- **C**：Approval 模型 — **可借鉴为 ToolPermission trait**
- **D**：AppServer 进程模型 — **不学**（我们是库不是服务）
- **E**：Message history compaction — **可借鉴为 CompressionTool**
- **F**：安全模型（landlock/bubblewrap/seatbelt）— **可选**
- **G**：Telemetry + OTLP — **已做**（synthia-telemetry 已实现）

### 3.3 Pi-mono 极简（pi-mono 117 行报告）

**核心推荐（最强烈）**：
- `AgentMessage + llm_visible()` — 单一抽象让一切变简单
- **不要学**：
  - 30 overloads
  - `wrap_output_with_otel`（task-local）
  - Subscription/OAuth（依赖 TS 生态）
- **要学**：
  - AgentMessage 的 5 个 method trait（system / user / assistant / tool_call / tool_result）
  - CompactionTool 抽象（独立于 react_loop）

### 3.4 反方挑战（inline in §4）

**反方主张**：全部 tool 化是过度抽象陷阱。

**反方让步条件（Progressive Toolification 原则）**：

> 一个候选功能**可以** tool 化的充要条件：
> 1. 外部可达（用户/LLM 可触发）
> 2. 副作用显式（产出一个可观察 artifact）
> 3. 上下文相关（需要 LLM 上下文决定何时调用）
> 4. 可降级（Tool 不可用时能 fallback 到核心 loop）

> 4 个条件中至少 3 个满足才 tool 化，否则保持核心机制。

**反方判定**：
- ✅ Tool 化：compression / subagent / file ops / permission gating / tool search
- ❌ 不 Tool 化：react_loop / session lifecycle / event emission / telemetry span / cancel token / provider routing

### 3.5 Synthia Gap 重审（被 cancel，替代以人工校准）

人工校准结果（与 baseline 2026-07-12 对比）：
- AgentRunConfig **多 2 个** `_xxx` 字段被丢弃（`_prompt_template_name`, `_model_id`）
- Tool trait **多 5 个方法**（12 vs 7）
- AgentTool **已实现 + 已注册**（baseline 误判）
- Compress **已在 react_loop 调用**（baseline 漏报）

---

## §4 设计取捨

### 4.1 范围 vs 抽象

| 选项 | 优点 | 缺点 |
|------|------|------|
| 全部 Tool 化 | 用户原话最强解释 | 抽象成本（每 Tool 一个 trait）、性能（每次调用穿过 registry） |
| 仅外部可达 Tool | 与 opencode 一致 | session lifecycle / cancel 仍内置（边界模糊） |
| Progressive Toolification | 渐进、可证伪 | 需要评审标准（§3.4 4 条件） |

**取捨**：**采用 Progressive Toolification**，4 条件 ≥3 才 tool 化。

### 4.2 AgentMessage 抽象方式

| 选项 | 优点 | 缺点 |
|------|------|------|
| 加 `llm_visible()` method | 最小改动、向后兼容 | 不强制 message 序列一致 |
| 改为 `Message` enum | 类型安全、序列一致 | 破坏性变更（breaking） |
| 加 `MessageView` newtype | 零侵入 | 多一层间接 |

**取捨**：**加 `llm_visible() -> bool`**，再加 `MessageKind` enum 区分（这是渐进路线）。

### 4.3 Tool trait 是否扩展

| 选项 | 优点 | 缺点 |
|------|------|------|
| 维持 12 方法 | 不破坏现有实现 | API surface 大 |
| 拆为 3 个 sub-trait（Definition / Execution / Lifecycle） | 责任清晰、可选实现 | 需改所有实现（5 个） |
| 保留 12 方法 + 加 `category` 字段 | 不破坏 | 分类信息不强制 |

**取捨**：**拆 3 个 sub-trait**，每个 sub-trait ≤5 方法。

### 4.4 Tool Registry 数据结构

| 选项 | 优点 | 缺点 |
|------|------|------|
| HashMap<String, Arc<dyn Tool>> | 简单 | lookup O(n) 在 LLM 视野里 |
| HashMap + Vec<String> 索引 | lookup 加速 | 内存翻倍 |
| HashMap + Vec<ToolMetadata> | 最快 | 需双写同步 |

**取捨**：**HashMap + Vec<ToolMetadata>**（opencode 同款）。

### 4.5 Compaction 处置

| 选项 | 优点 | 缺点 |
|------|------|------|
| 保持内置（在 react_loop） | 简单 | 与 Tool 化目标冲突 |
| 抽象为 CompactionTool | 符合 Progressive 原则 | 需改 react_loop 调用点 |
| 双模式（默认内置，可换 Tool） | 灵活 | 复杂 |

**取捨**：**抽象为 CompactionTool**（默认实现保留，可注入自定义）。

### 4.6 Phase 切分

- **Phase 1（6 周）**：P0 #1-#10 = 10 个 low/mid-cost PR
- **Phase 2（4 周）**：P1 = 5 个架构级 refactor
- **Phase 3（4 周）**：可选 = opencode/codex 高级模式

---

## §5 未決问题

1. **Subscription 模型是否需要？** — 当前 API key 路线够用；如未来有 OAuth 需求，加 `SubscriptionGate` (codex B 模式)
2. **VFS sandbox 是否引入？** — 取决于部署目标；如果是 CLI 工具则需要 (codex F)
3. **AgentMessage 破坏性 vs 非破坏性** — 优先非破坏（加 `llm_visible()` + `MessageKind`），下次 major 时再切 enum
4. **Provider routing** — 单 Provider 内置，多 Provider 可作为 Phase 3
5. **OTel integration 是否激进** — synthia-telemetry 已做，需确认 Provider 端是否需要 trace propagation（pi-mono 反对 task-local）

---

## §6 决策记录

| # | 决策 | 来源 | 风险 |
|---|------|------|------|
| D1 | 范围 = 全栈对标 + Tool 化 | Q1 选 A | 范围失控 |
| D2 | 深度 = 概念 + Trait 草案 | Q2 选 B | 草案不够具体 |
| D3 | 5 项对抗维度全开 | Q3 选 E | 信息冗余 |
| D4 | 输出 = Markdown 综合报告 | Q4 选 A | — |
| D5 | 后续 = OpenSpec change proposal | Q5 选 A | 当前正在执行 |
| D6 | Progressive Toolification 原则 | 反方挑战让步 | 边界模糊 → 通过 4 条件具体化 |
| D7 | 拆 Tool trait 为 3 sub-trait | §4.3 | 改 5 个实现 → PR #P0-#3 范围内 |
| D8 | AgentMessage 加 `llm_visible()` | §4.2 | 非破坏，安全 |
| D9 | ToolRegistry 双索引 | §4.4 | 简单 PR |
| D10 | Compaction 抽象为 Tool | §4.5 | 改 react_loop 调用点 |
| D11 | Phase 1 = 10 PR（6 周） | 综合报告 §5 | — |

---

## §7 Skill 自我评审

### Placeholder scan

- ✅ 无 TBD / TODO
- ✅ 所有 4 条件已具体化（4 条件 ≥3 才 tool 化）

### Internal consistency

- ✅ §3 反方主张与 §4.1 取捨一致（采用 Progressive）
- ✅ §5 未决问题与 §6 决策记录不冲突（每个决策都有 §6 一行）
- ✅ D5 = 当前正在执行

### Scope check

- ✅ Phase 1（10 PR）切得足够小，每 PR 1-3 天
- ✅ 不需分解为多个 spec

### Ambiguity check

- ✅ Progressive 4 条件有具体定义
- ✅ Tool sub-trait 拆法有具体边界（≤5 方法/个）

---

## §8 用户已批准的方向（隐式）

由于用户已选 A（OpenSpec change proposal）= D5 当前正在执行，
且之前 5 轮澄清问题均已回答，
brainstorming 阶段已完成。用户不需要再批准 design section。