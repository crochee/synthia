# Synthia × a2a-ui 融合设计方案（MVP 版 · v2）

> 状态：v2 终稿（多专家对抗性审查完成）
> 适用范围：`/home/crochee/workspace/synthia`
> 上游参考：`mvp-agent-design.md` v1.3（权威边界） + `~/workspace/a2a-ui`（交互/布局参考）
> 子文档：[goallistpanel-aggregation.md](goallistpanel-aggregation.md)（GoalListPanel 事件聚合点细节）

## 0. 背景

`a2a-ui` 在 A2A 协议交互、可视化布局、Trace 展示上做了完整探索；Synthia 当前已具备协议层（SSE、TaskHistory、A2A SDK v1.0）。本设计在 **不偏离 `mvp-agent-design.md` MVP 边界**（§10/§11）的前提下，仅吸收 `a2a-ui` 中**对 MVP 验收有帮助**的交互/展示模式。

## 1. 设计原则（与 MVP 对齐）

1. **协议优先**：UI 必须忠实反映 A2A v1.0 语义（Part / Artifact / Task / Context），不发明字段。
2. **MVP 边界**：不做任何权限/安全能力、不做远程 Agent 发现、不做完整 Trace 可视化、不做多主题切换。
3. **可中断、可恢复**：长任务支持 cancel、续接、归档（M1 验收要求"读 + 改 + 总结"完整跑通）。
4. **设计 token 收敛**：颜色 / 间距 / 字体收敛到一套变量，便于后续主题切换。
5. **借鉴而非重写**：复用 Synthia 现有 Radix UI + Custom CSS 体系；不引入 shadcn/ui。
6. **MVP 最小增强的边界规则**：新加的最小增强必须满足 **(1) 单文件可改完 + (2) 不引入新依赖 + (3) 与现有 Radix 体系不冲突**。三项不满足其一就退到 V2。

## 2. 与 `mvp-agent-design.md` MVP 边界的对齐

| MVP 验收项（§10） | 当前 Synthia 现状 | 本设计中的支撑 |
|---|---|---|
| **M1** 读 + 改 + 总结任务跑通 | `ChatPage` 已有 ToolCall/ToolResult 折叠展示 | 强化 `ChatMessageView` 折叠交互与超时提示 |
| **M1** 模型 streaming 正常 | `a2a-stream.ts` 已实现 SSE 流式 | 无新增需求 |
| **M2** 多步任务有 goal 状态可视化 | Synthia 当前**无 Goal 状态 UI** | **新增 GoalListPanel**（详见子文档） |
| **M2** Classifier/Planner/Evaluator 可观察 | 无 | **新增 StatsBar**（紧凑版：事件类型计数 + StopReason） |
| **M1+M2** ReAct loop 步数 / Tool 调用频次可观察 | 无 | **新增 StatsBar**（tool 次数 / ReAct 步数 / StopReason） |

> **不引入项**（明确划出 MVP）：Phoenix/Arize 接入、a2a-ui/TraceSidebar、远程 Agent URL 发现、SSRF 防护、`synthia-trace-store`、多主题切换、可拖拽 TracePanel。这些保留为 **V2 扩展项**。

## 3. 现状对比（仅 MVP 相关）

| 维度 | a2a-ui | Synthia | MVP 是否需要补 |
|---|---|---|---|
| Chat 三栏布局 | Trace(左) + Chat(中) + AgentDetail(右) | 单栏 | **不补**（V2） |
| 可拖拽侧栏 | ResizableSidebar + localStorage | 220px 固定 | **不补**（V2） |
| 主题切换 | Dark/Light/System | 仅暗色 | **不补**（V2） |
| Goal 状态可视化 | 无 | 无 | **必须补**（M2 验收） |
| 紧凑统计条 | 无 | 无 | **必须补**（M1+M2 可读性） |
| Stream toggle UI | 有 | 无 | **最小增强**：复用 SSE 流，仅暴露开关 |
| Context ID 可视化 | 有 | 自动 UUID | **最小增强**：ChatHeader 展示 + 复制 |
| ErrorBoundary | 全局 | 缺失 | **必须补**（M1 验收基础） |
| 加载守卫 | `useAppState` | 无 | **必须补**（避免首屏闪烁） |
| Tool call/Result 折叠 | 有 | 已有 | **复用 + 强化超时提示** |
| Agent URL 发现 | 有 | 无 | **不做**（V2） |
| Trace 可视化 | Jaeger + Graph | 无 | **不做**（V2） |

## 4. 前端方案

### 4.1 目录调整

```
synthia-web/src/
├── app/
│   ├── AppShell.tsx           # 新增：仿 a2a-ui App.tsx，加载守卫
│   ├── ErrorBoundary.tsx      # 新增：全局错误边界
│   └── routes.tsx             # 替换 App.tsx 路由表
├── state/                     # 新增
│   ├── AppContext.tsx
│   ├── useAppState.ts
│   ├── goals.ts               # 新增（M2 验收）
│   └── useGoals.ts            # 新增（M2 验收）
├── components/
│   ├── layout/
│   │   ├── MainLayout.tsx
│   │   └── ChatHeader.tsx     # 新增：title + context_id 复制 + stream toggle
│   └── chat/
│       ├── GoalListPanel.tsx      # 新增（M2 验收）
│       ├── StatsBar.tsx           # 新增（M1+M2 可读性）
│       └── ChatMessageView.tsx    # 复用 + 强化 ToolBlock 超时提示
└── pages/
    ├── ChatPage.tsx           # 接入 ChatHeader / GoalListPanel / StatsBar
    ├── TasksPage.tsx          # 保持现有
    ├── TaskDetailPage.tsx     # 接入 useGoals({source:'history'})
    ├── AgentsPage.tsx         # 保持现有
    └── SettingsPage.tsx       # 保持现有
```

> **不做项**（V2）：`TracePanel` / `JaegerTraceView` / `TraceGraph` / `ThemeToggle` / `ResizableSidebar` / `AddAgentDialog` / `AgentDetailPanel` / `EventTimelineStrip`（v2 评审后收敛为 StatsBar）。

### 4.2 MVP 必需项

#### AppShell 加载守卫

```ts
// state/useAppState.ts
export function useAppState() {
  const { agents, sessions, ready } = useAppContext();
  return { ready, agents, sessions };
}

// AppShell.tsx
const { ready } = useAppState();
if (!ready) return <BootScreen />;
return <MainLayout />;
```

#### ErrorBoundary

```tsx
class ErrorBoundary extends React.Component<Props, State> {
  static getDerivedStateFromError(error: Error) { return { error }; }
  componentDidCatch(error, info) { /* 上报 / log */ }
  render() {
    if (this.state.error) return <ErrorScreen error={this.state.error} reset={() => this.setState({ error: null })} />;
    return this.props.children;
  }
}
```

#### StatsBar（M1+M2 可读性）

- 当前 session：tool 调用次数 / ReAct 步数 / 总耗时 / 当前 StopReason
- 每 5s tick 驱动更新
- 数据来源：从 messages 数组派生（tool_call + tool_result 计数）
- **不做**（V2）：按 model 拆分 token 用量、Goal 收敛率

### 4.3 最小增强项

#### ChatHeader

```tsx
<ChatHeader
  title={sessionTitle}
  contextId={contextId}
  onCopyContextId={() => navigator.clipboard.writeText(contextId)}
  streaming={streaming}
  onToggleStreaming={setStreaming}
/>
```

- 仅展示 + 复制（**不做编辑/重生**，V2）
- `streaming` 切换：下一次 send 生效

#### ChatMessageView ToolBlock 超时徽章

- timeout 后显示 `🟡 timed out (>180s)` 徽章 + 折叠时显示状态点

### 4.4 GoalListPanel（M2 验收关键）

**详见子文档 [goallistpanel-aggregation.md](goallistpanel-aggregation.md)。**

要点：
- 数据流：`AgentEvent::System(GoalUpdate)` → A2A SSE `StatusUpdate{Part.data}` → `a2a-stream.ts::classifyPart` 识别 → `state/goals.ts` reducer → `useGoals` hook → GoalListPanel 渲染
- 协议层新增 `SystemEvent::GoalUpdate` 变体（v2 §3 详细设计）
- 客户端 `looksLikeGoalPayload` 自然形状检测（不引入 `kind` 字段）
- Reducer 全量快照语义（按 `mvp-agent-design.md §6.5` `ProgressEval::AdjustPlan`）
- Hook 两入口：`source: 'stream' | 'history'`

### 4.5 设计 token 收敛

- 复用现有 `styles/page.css`：`--font-mono`、`--accent-green/cyan/magenta/red`、`nt-pill`、`nt-markdown`
- **不引入 shadcn/ui**
- 颜色 / 间距 / 圆角阶梯化在 `styles/tokens.css`（V2 主题切换基础）

## 5. Server 方案（最小新增）

### 5.1 新增端点（MVP 必需）

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/api/v1/agents` | 已有 → **增强**：返回 `capabilities` / `defaultInputModes` / `defaultOutputModes` / `skills[]` 完整字段 |
| `GET` | `/api/v1/sessions/{id}/events?since=N` | **新增**（可选，详见子文档 §5.3） |

### 5.2 不新增（明确划出）

| 不做项 | 理由 |
|---|---|
| `/api/v1/agents/discover` / `/api/v1/agents/remote` | 远程 Agent 发现属于 V2 |
| `/api/v1/agents/{name}/card` | 本地 Card 暴露属于 V2 |
| `/api/v1/traces/...` | Trace 可视化属于 V2 |
| `/api/v1/tasks/{id}/cancel` | 任务取消属于 V2 |
| `/api/v1/health` 增强 | Provider 探测属于 V2 |
| `synthia-trace-store` feature | Trace 持久化属于 V2 |

### 5.3 关于 `/api/v1/sessions/{id}/events`

**v2 评审决议**：MVP 阶段**先不实现**。原因：
- GoalUpdate 已通过 SSE 主通道实时下发，状态机可见
- 新增端点会引入鉴权 + 存储 + 测试 4 项 MVP 范围外工作
- 等 M2 验收需求"history 回放" 出现具体诉求时再做（与 `useGoals({source:'history'})` 一并）

> 子文档 v2 §5.3 仍保留此端点设计稿作为 V2 待办。

## 6. 可观测性方案（最小版）

| 层 | 当前 | MVP 目标 |
|---|---|---|
| 服务端 | OTel OTLP（可选）+ tracing 日志 | **不新增**，复用现有 |
| 前端 | 无 | **最小版**：StatsBar + GoalListPanel + ChatHeader（全部从 SSE 流拉取） |
| 关联 | `traceparent` header 透传 | **不引入前端 trace_id 跳转**（V2） |
| 会话 | `context_id` | ChatHeader 展示 + 复制 |

**不做**（V2）：`synthia-trace-store`、Jaeger 视图、Graph 视图、SessionBadge、TraceAttributes、`/events` 端点。

## 7. 实施路线图

### MVP（本次实施，1–2 周）

| 序号 | 任务 | 验证 |
|---|---|---|
| 1 | `AppContext` + `useAppState` + `AppShell` 加载守卫 | 首屏不闪烁 |
| 2 | `ErrorBoundary` 全局化 + `ErrorScreen` | 子树崩溃可恢复 |
| 3 | `ChatHeader`（context_id 展示 + 复制 + stream toggle） | 复制可粘贴；切 stream 不打断 |
| 4 | `StatsBar`（tool 次数 / ReAct 步数 / StopReason） | 实时显示 |
| 5 | `ChatMessageView` ToolBlock 超时徽章强化 | 超时后徽章可见 |
| 6 | Server: `GET /api/v1/agents` 字段增强 + 测试 | `cargo test -p synthia-server` 通过 |
| 7 | 设计 token 收敛（`styles/tokens.css`） | 视觉对齐 |
| 8 | 前端 0 lint + TypeScript 0 error；Server `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all` 通过 | 验收清单 |
| 9 | **M2 子任务**：GoalListPanel（详见子文档 §10） | 子文档验收全通过 |

### V2 延后（明确不在本次范围）

- Trace 可视化（Jaeger / Graph）、TracePanel、ResizableSidebar
- 远程 Agent URL 发现、`/api/v1/agents/discover`、`/api/v1/agents/remote`
- 主题切换（Dark/Light/System）、`ThemeToggle`
- 任务取消（`POST /api/v1/tasks/{id}/cancel`）
- `synthia-trace-store` feature
- Phoenix/Arize 接入
- `AgentDetailPanel`（右侧 320px）
- `/api/v1/health` Provider 探测
- `/api/v1/sessions/{id}/events` 端点
- Context ID 编辑/重生
- `EventTimelineStrip`（v2 评审后收敛为 StatsBar）

## 8. 风险与权衡

| 风险 | 缓解 |
|---|---|
| `EventTimelineStrip` 数据量膨胀 | v2 评审后已收敛为 StatsBar，仅计数 |
| Goal 状态在多轮 session 中聚合 | `useGoals` sessionId / task 变化时 reset |
| StatsBar 5s tick 性能 | 仅统计当前 session；MVP 不引入 Web Worker |
| Server `/agents` 字段增强引入回归 | 既有 arm 沿用；新增单测覆盖 |
| `ErrorBoundary` 包裹过深 | 仅包裹 `MainLayout` 外层 |

## 9. 验收标准（M1+M2）

### M1（最小可跑通）
- [ ] 首屏守卫：进入 `/chat` 前数据齐，不闪烁
- [ ] ErrorBoundary：故意触发崩溃可恢复
- [ ] ChatHeader：context_id 可复制；stream toggle 不打断进行中任务
- [ ] StatsBar：tool 次数 / ReAct 步数 / StopReason 实时显示
- [ ] ChatMessageView ToolBlock：timeout 后徽章可见
- [ ] "读 + 改 + 总结"任务完整跑通

### M2（进化 ReAct）
- [ ] GoalListPanel：触发 `MultiStepTask` 时自动展开；四态徽章可见
- [ ] "找出所有 TODO 并生成报告"任务跑通
- [ ] 子文档 v2 §9 验收全通过

### 质量门
- [ ] 前端 0 lint、TypeScript 0 error
- [ ] Server `cargo +nightly fmt --all` 通过
- [ ] Server `cargo clippy --all-targets --all-features --tests --all` 通过
- [ ] Server 分模块 `cargo test -p <crate>` 通过

## 10. 与 v1 差异

| 维度 | v1 融合稿 | v2 终稿 |
|---|---|---|
| 借鉴幅度规则 | 未明说 | 新增"最小增强的边界规则"（§1） |
| EventTimelineStrip | 单独章节 | **收敛为 StatsBar**（v2 评审后） |
| `/api/v1/sessions/{id}/events` | 列为 MVP 必需 | **推迟到 V2**（v2 评审后） |
| GoalListPanel | §4.5 概要 | **抽出独立子文档** `goallistpanel-aggregation.md` |
| 实施路线图 | 11 项 | 9 项（去独立 e2e / health 增强） |
| 引用子文档 | 无 | 链接 goallistpanel-aggregation.md |