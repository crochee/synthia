# GoalListPanel 事件聚合点 — 设计稿 v2

> 状态：v2 终稿（多专家对抗性审查完成）
> 适用范围：`/home/crochee/workspace/synthia`
> 上游约束：`mvp-agent-design.md` §3.1 / §6.4 / §6.5 / §6.6 / §10 M2 / §11.2
> 上游文档：`docs/design/synthia-a2a-ui-fusion-design.md` §4.5

## 0. 现状与缺口

仓库内事实（2026-08-15）：

| 层 | 现状 | 缺口 |
|---|---|---|
| `crates/synthia-agent/src/events/system_event.rs` | `SystemEvent` 9 变体 | 无 GoalUpdate / Classifier / Planner / Evaluator |
| `crates/synthia-agent/src/events/event_enum.rs::is_durable` | 所有 `System(_)` 一律 false | GoalUpdate 需标 true |
| `crates/synthia-server/src/a2a/executor.rs::record_event_into_history` | GoalUpdate 走兜底（不写） | 需扩 arm 写入 history |
| `synthia-web/src/api/a2a-stream.ts::classifyPartPayload` | 识别 tool_call / tool_result 两种自然形状 | 无 goal_update 形状 |
| `synthia-web/src/pages/ChatPage.tsx` | 维护 messages + 5s tick + tool_block 超时 | 无独立 goal 状态 |

## 1. 选型（A1）

- **协议层**：新增 `SystemEvent::GoalUpdate` 变体 + executor.rs 走 A2A `StatusUpdate` + `Part.data`
- **客户端聚合**：`a2a-stream.ts::classifyPart` 自然形状识别 + `state/goals.ts` 纯函数 reducer + `state/useGoals.ts` hook
- **不引入 `kind` 判别字段**，靠 `{goals:[{id,description,status},...]}` 键集合识别

## 2. 设计原则

1. 协议层是单一真相源
2. 不发明判别字段（与 `classifyPartPayload` 检测 `{id, name, input}` 同源）
3. 协议 / aggregator / reducer / view 四层解耦
4. Reducer 纯函数，便于单测
5. **MVP 最小增强的边界规则**（v2 新增）：新加的最小增强必须满足 **(1) 单文件可改完 + (2) 不引入新依赖 + (3) 与现有 Radix 体系不冲突**。三项不满足其一就退到 V2。

## 3. 协议层

### 3.1 `Goal` 与 `GoalStatus`（与 `mvp-agent-design.md §3.1` 对齐，无重复定义）

字段最小化（安全审查 D8）：仅 `id` / `description` / `status` 三字段。**不携带 prompt / 内部评估文本 / token 计数**（这些属于 §13.3 V2 监控指标）。

`GoalStatus` 4 态 snake_case：`pending` / `in_progress` / `completed` / `blocked`。

### 3.2 `SystemEvent::GoalUpdate`

```rust
pub enum SystemEvent {
    SessionStarted { /* ... */ },
    SessionEnded { /* ... */ },
    // ... 现有 8 变体 ...
    /// ★ Goal 状态更新（classifier / planner / evaluator 产出）
    ///
    /// `goals` 是当前 session 的**完整**目标列表（不是增量）。
    /// 每次 emit 都是"现在这 N 个 goal 各自处于 X 状态"的全量快照。
    /// reducer 端用 goal.id 做 key 合并。
    ///
    /// `iteration` 为可选（v2 降级 Option<usize>）；MVP 仅用于
    /// 前端"按时间分组"的视觉排序，不参与任何自动化。
    GoalUpdate {
        goals: Vec<Goal>,
        iteration: Option<usize>,
    },
}
```

- `kind()` 返回 `"GoalUpdate"`
- spec 表新增：`GoalUpdate` **durable = true**

### 3.3 `is_durable` 方案 α（D2 裁定）

```rust
// crates/synthia-agent/src/events/event_enum.rs::is_durable
Self::System(sys) => matches!(sys, SystemEvent::GoalUpdate { .. }),
```

理由：不递归到 `SystemEvent::is_durable`，避免"durable 性"概念扩散到 System 层；MVP 集中一行可见。

### 3.4 Server SSE 映射（D5 裁定）

```rust
// crates/synthia-server/src/a2a/executor.rs —— 主事件循环
AgentEvent::System(SystemEvent::GoalUpdate { goals, iteration }) => {
    // A2A v1.0 wire 形状（D5 已确认）：
    //   StreamResponse::StatusUpdate {
    //     taskId, contextId,
    //     status: { state: TaskState::Working,
    //               message: Some(Message {
    //                 parts: vec![Part::data(json!({"goals": goals, "iteration": iteration}))],
    //                 ... }),
    //               timestamp: Some(now) },
    //     final: false,            // ★ SDK 字段名待核对（v2 待办 §13）
    //   }
    // 注：实施时第一步是核对 @a2a-js/sdk v1.0 的 StatusUpdate 字段名
    // （final vs final_status vs is_final）；本节给出期望形状，代码以
    // 实际 SDK 类型为准。

    // 历史写入（D7 裁定：消除歧义）
    record_event_into_history(&mut history_builder, &event);

    // SSE 发射
    yield StreamResponse::StatusUpdate(/* 上述形状 */);
}
```

**节流约束（D6 裁定）**：
- Planner 每轮 ReAct 只发一次 GoalUpdate（拆解完成后立即发）
- Evaluator 触发 `ProgressEval::AdjustPlan` 时再发一次（覆盖旧 goals）
- Classifier 不直接发 GoalUpdate（classifier 输出体现在第一次 GoalUpdate 的 goals 列表是否非空）
- 频率上限：每轮 ReAct ≤ 2 次 GoalUpdate

### 3.5 历史写入（D7 裁定）

```rust
// crates/synthia-server/src/a2a/executor.rs::record_event_into_history
AgentEvent::System(SystemEvent::GoalUpdate { goals, iteration }) => {
    // Part.data 形状：{ "goals": goals, "iteration": iteration }
    // Part.text 为空（结构化载荷不混文本）
    HistoryPart::data_only(json!({ "goals": goals, "iteration": iteration }))
}
// 兜底分支 AgentEvent::System(_) => {} 改为：
// AgentEvent::System(_) => {} // 仅处理非 GoalUpdate 的 System 变体
// （保留原语义，避免扩散）
```

### 3.6 Server 测试矩阵（7 项，D10）

| # | 场景 | 期望 |
|---|---|---|
| 1 | GoalUpdate 序列化回环 | 字段值不变；status snake_case |
| 2 | `is_durable` 对 GoalUpdate = true，其他 System = false | spec 表一致 |
| 3 | GoalUpdate → history 写入 | `Task.history` 末条 message 含 `Part.data = {goals, iteration}`，text 为空 |
| 4 | GoalUpdate → SSE 形状 | `StatusUpdate{state=working, final=false, message.parts=[Part.data{...}]}` |
| 5 | 多次 GoalUpdate 间隔不交叉污染 history | 每条 GoalUpdate message 独立 |
| 6 | GoalUpdate + SessionEnded 同帧到达 | SessionEnded 仍走 `StreamResponse::Task` 终态分支（`is_terminal_event` 维持） |
| 7 | **回归**：SessionStarted / SessionEnded / ToolUse / ToolResult 写入 history 路径不被 GoalUpdate arm 改动破坏 | 既有测试全通过 |

## 4. 客户端聚合层

### 4.1 识别：`looksLikeGoalPayload`（D11 加强白名单）

```ts
// synthia-web/src/api/a2a-stream.ts
function looksLikeGoalPayload(
  data: Record<string, unknown>,
): data is { goals: Array<{ id: string; description: string; status: string }>; iteration?: number | null } {
  if (!Array.isArray(data.goals)) return false;
  if (data.goals.length === 0) return false; // 空数组不识别
  return data.goals.every((g) =>
    g && typeof g === 'object'
    && typeof (g as Record<string, unknown>).id === 'string'
    && typeof (g as Record<string, unknown>).description === 'string'
    && typeof (g as Record<string, unknown>).status === 'string',
  );
}
```

不变量：
- 不引入 `kind` 判别字段
- **只读白名单键**；`Part.metadata` / 额外字段忽略（D11 加强）
- 与 `classifyPartPayload` 检测 `{id, name, input}` 同源

### 4.2 SegmentType 与 SegmentMetadata

```ts
export type SegmentType = /* ... */ | 'goal_update'; // ★ 新增

export interface SegmentMetadata {
  // ... 现有字段 ...
  goals?: Array<{ id: string; description: string; status: GoalStatusWire }>;
  iteration?: number | null;
}

export type GoalStatusWire = 'pending' | 'in_progress' | 'completed' | 'blocked';
```

### 4.3 Reducer：`state/goals.ts`（D13 合并）

**纯函数 + 类型 + 单测**，无 React / 无 SSE 引用。

```ts
export interface GoalUpdatePayload {
  goals: ReadonlyArray<Goal>;
  iteration: number | null;
}

export interface GoalsState {
  byId: Record<string, Goal>;
  iteration: number | null;
  activeGoalId: string | null;
}

export const initialGoalsState: GoalsState = { byId: {}, iteration: null, activeGoalId: null };

/**
 * 全量快照语义：
 *   每次 GoalUpdate 覆盖整个 byId（mvp-agent-design.md §6.5
 *   ProgressEval::AdjustPlan { goals: Vec<Goal> } 的设计）。
 *   reducer 用 goal.id 做 key 合并：
 *     - 老 goal.id 不再出现 → 自动清掉
 *     - 同 goal.id → status 字段被最新值覆盖
 *     - 顺序由 localeCompare 稳定化
 */
export function applyGoalUpdate(state: GoalsState, payload: GoalUpdatePayload): GoalsState { /* ... */ }

export function resetGoals(): GoalsState { /* = initialGoalsState */ }

function pickActiveGoal(byId: Record<string, Goal>): string | null {
  // InProgress 优先 → Pending 次之 → null
  // 字典序用 localeCompare（V2 国际化时切 Intl.Collator）
}
```

### 4.4 Hook：`state/useGoals.ts`（D4 + D13 合并）

**两入口合一**：参数化 `source`。

```ts
export function useGoals(opts: {
  source: 'stream';
  sessionId: string;
  stream: AsyncGenerator<A2AStreamEvent>;
}): { state: GoalsState; activeGoalId: string | null; orderedGoals: Goal[] };

export function useGoals(opts: {
  source: 'history';
  task: TaskDetail;
}): { state: GoalsState; activeGoalId: string | null; orderedGoals: Goal[] };
```

关键约束（D4 + D7）：
- **不重复订阅**：stream 分支复用 ChatPage 的 `sendMessageStream`，只分叉 dispatch
- **sessionId / task 变化时 reset**：避免跨 session 残留
- **history 分支一次性回放**：遍历 `task.history` 把 GoalUpdate message 喂给 reducer（**不调 stream**，纯函数式喂入）

### 4.5 ChatPage 与 TaskDetailPage 接入

```ts
// ChatPage.tsx —— 改动点
const { state: goalsState } = useGoals({ source: 'stream', sessionId, stream: sendMessageStream(text, sessionId) });

// TaskDetailPage.tsx —— 改动点
const { state: goalsState } = useGoals({ source: 'history', task });
```

两者共享同一 reducer；UI `GoalListPanel` 接 `state` 即可。

### 4.6 ChatMessageView 兼容性

```ts
// ChatMessageView.tsx —— dispatchPartPayload 现有 ignore 分支新增：
if (meta.type === 'goal_update') return; // 已被 goalsReducer 消费
```

### 4.7 客户端测试矩阵（13 项，D11 加强）

| # | 输入 | 期望 |
|---|---|---|
| 1 | `applyGoalUpdate({}, {goals:[], iteration:0})` | `byId={}, active=null` |
| 2 | `applyGoalUpdate({}, {goals:[g1=pending,g2=in_progress,g3=completed], iteration:1})` | `active=g2` |
| 3 | `applyGoalUpdate(state, {goals:[g1=completed,g2=pending], iteration:2})` | `g1.status` 更新；`g2` 新增；`active=g2` |
| 4 | `applyGoalUpdate(state, {goals:[g1=blocked], iteration:3})` | `active=null` |
| 5 | `looksLikeGoalPayload({goals:[{id,description,status}]})` | true |
| 6 | `looksLikeGoalPayload({goals:[{id,description}]})` | false（status 缺失） |
| 7 | `looksLikeGoalPayload({goals:[]})` | false |
| 8 | `looksLikeGoalPayload({id,name,input})` | false（仍是 tool_call） |
| 9 | `looksLikeGoalPayload({goals:'not array'})` | false |
| 10 | `pickActiveGoal({g1:pending,g2:in_progress,g3:completed})` | `'g2'` |
| 11 | `resetGoals()` | 深度等于 initialGoalsState |
| 12 | ChatMessageView fixture `{type:'goal_update'}` | 渲染次数 = 0 |
| 13 | **D11 加强**：fixture 含 `Part.metadata = {truncated_by:'hook'}` 时识别仍通过 | true（白名单忽略额外字段） |

## 5. UI 层

### 5.1 组件契约

```ts
// components/chat/GoalListPanel.tsx
export interface GoalListPanelProps {
  goals: Goal[];          // reducer 已字典序稳定
  activeGoalId: string | null;
  iteration: number | null;
  defaultCollapsed?: boolean;
}
```

自动展开规则：`iteration` 从 `null` → 非 null 时自动展开；折叠/展开状态写入 `localStorage['synthia.goalPanelCollapsed']`。

### 5.2 徽章映射

| GoalStatusWire | CSS slug | 颜色 token |
|---|---|---|
| `pending` | `nt-pill--pending` | `--accent-cyan` |
| `in_progress` | `nt-pill--in-progress` | `--accent-green` |
| `completed` | `nt-pill--completed` | `--accent-magenta` |
| `blocked` | `nt-pill--blocked` | `--accent-red` |

`nt-pill` 体系已在 `synthia-web/src/styles/page.css` 存在，扩 4 个 slug。

## 6. 状态机时序

```
User input ─► classifier() ─► IntentKind::MultiStepTask
                              │
                              ▼
                    planner() ─► [g1=pending, g2=pending, g3=pending]
                              │
                              ▼
        AgentEvent::System(GoalUpdate { goals, iteration: Some(0) })
                              │
        ┌─────────────────────┼────────────────────────────────┐
        │ server              ▼                                │
        │   record_event_into_history ──► history.parts[].data │
        │   yield StreamResponse::StatusUpdate{state=working} │
        └─────────────────────┼────────────────────────────────┘
                              ▼ SSE
                    client (a2a-stream.ts::classifyPart)
                              ▼
                  A2AStreamEvent{type:'message', parts=[Part{data:{goals,iteration}}]}
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
   ChatPage stream 分叉                TaskDetailPage 回灌
              │                               │
              ▼                               ▼
       goalsDispatch(applyGoalUpdate) ──► GoalsState
                                                  │
                                                  ▼
                                          GoalListPanel 渲染
```

## 7. 不做项

- Goal 编辑 / 手动添加 / 删除（§11.1 未列入 MVP；V2）
- Goal 拖拽排序
- Goal 收敛率统计（§13.3 V2 监控）
- Per-goal Span / Trace 跳转
- `synthia-trace-store` feature
- Goal 完成原因 / 备注字段（§3.1 未列）
- `IntentKind` 字段（议题 2 裁定 D3）
- `chatStreamBus.ts` 中间层（D15 裁定删除）
- Phoenix/Arize 接入
- 多主题切换
- 远程 Agent URL 发现
- `synthia-trace-store`

## 8. 风险与权衡

| 风险 | 缓解 |
|---|---|
| `executor.rs` 改动引入回归 | §3.6 第 7 项回归测试 + 沿用 `record_event_into_history` 既有 arm |
| `is_durable` 改造扩散 | 方案 α 集中一行；不引入 `SystemEvent::is_durable` |
| GoalUpdate 频率淹没 heartbeat | §3.4 节流：每轮 ≤ 2 次 |
| `looksLikeGoalPayload` 与未来 ContentPart 字段冲突 | §4.1 白名单严格；§4.7 第 13 项测试覆盖 |
| `useGoals` 两入口（stream / history）形态分裂 | §4.4 reducer 完全共享；hook 仅 switch dispatch |
| 字典序跨 locale 不一致 | MVP `localeCompare`；V2 切 `Intl.Collator` |
| Goal 超过 3–7 个时 UI 性能 | MVP 不做虚拟滚动；V2 |
| 端到端缺失 | §9.3 增补 1 例端到端测试 |

## 9. 验收标准

### 9.1 协议
- [ ] §3.6 7 项测试全通过
- [ ] `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all` + `cargo test -p synthia-agent` + `cargo test -p synthia-server` 通过

### 9.2 客户端
- [ ] §4.7 13 项测试全通过
- [ ] ChatPage + TaskDetailPage 接入 `useGoals` 后渲染一致
- [ ] ChatMessageView 对 `type:'goal_update'` 渲染 0 次
- [ ] 前端 0 lint、TypeScript 0 error

### 9.3 端到端
- [ ] 1 例 e2e：触发 MultiStepTask → GoalListPanel 自动展开 → 四态徽章可见 → session 切换 reset → 切回 history 回放

### 9.4 UI
- [ ] GoalListPanel 自动展开规则生效
- [ ] 折叠/展开状态持久化到 `localStorage`
- [ ] 四态徽章颜色 token 生效

## 10. 实施顺序（4 步，D14 简化）

1. **协议层**：`crates/synthia-agent/src/goal.rs` + `system_event.rs::GoalUpdate` + `event_enum.rs::is_durable` 方案 α + 单测
2. **Server 映射**：`executor.rs::record_event_into_history` 扩 arm + SSE `StatusUpdate` 分支 + 7 项测试
3. **前端识别**：`a2a-stream.ts::looksLikeGoalPayload` + `classifyPart` 扩 + `SegmentType` 加 `'goal_update'` + `extractFromMessage` 翻译 `metadata.goals` / `metadata.iteration`
4. **前端 reducer + hook + UI**：
   - `state/goals.ts`（reducer + types）+ 单测（13 项）
   - `state/useGoals.ts`（stream / history 两入口）
   - `components/chat/GoalListPanel.tsx`
   - `styles/page.css` 扩 4 个 `nt-pill--*` slug
   - ChatPage + TaskDetailPage 接入

## 11. 安全审查结论（D8 + D9）

- Goal 字段最小化：仅 `id` / `description` / `status`
- `iteration` 降级 `Option<usize>`，不强制显示
- 不携带 prompt / 内部评估文本 / token 计数
- Stream toggle UI 纯客户端，不改 Server 协议
- 零偏离 `mvp-agent-design.md §11.2` 范围

## 12. 与 `mvp-agent-design.md` 边界

- §3.1 Goal / GoalStatus 类型一致
- §6.5 ProgressEval::AdjustPlan `goals: Vec<Goal>` 与 reducer 全量快照语义一致
- §10 M2 验收：Goal 状态可视化 ✅
- §11.1 不做项：✓ 未引入任何 §11.1 砍头功能
- §11.2 安全/权限/Hook 砍头：✓ 零新增

## 13. 业界核对结论（D5 待核对事项 → 已核）

### 13.1 核对点 1：`@a2a-js/sdk` v1.0 `StatusUpdate` 字段

**核对结果**：`TaskStatusUpdateEvent` **没有 `final` 字段**。

**证据链**：

1. **本地 SDK d.ts**（`node_modules/@a2a-js/sdk/dist/a2a-Ubve0YhO.d.ts:208-219`）：
   ```ts
   interface TaskStatusUpdateEvent {
       taskId: string;
       contextId: string;
       status: TaskStatus | undefined;
       metadata: { [key: string]: any; } | undefined;
   }
   ```
   仅 4 个字段，无 `final` / `final_status` / `is_final`。

2. **A2A 官方 v1.0 变更说明**（[a2a-protocol.org/latest/whats-new-v1](https://a2a-protocol.org/latest/whats-new-v1/)）：
   > ✅ **REMOVED:** `final` boolean field removed from TaskStatusUpdateEvent. Leverage protocol binding specific stream closure mechanism instead.

3. **v0.x 时代确实有 `final: boolean`**（[agent2agent.info A2A 概念文档](https://agent2agent.info/zh-cn/docs/concepts/task/)）：
   ```ts
   interface TaskStatusUpdateEvent {
       id: string;
       status: TaskStatus;
       final: boolean; // v0.x 字段
       metadata?: Record<string, any>;
   }
   ```
   v0.x→v1.0 已彻底移除；本仓库使用 SDK v1.0，必须遵守新协议。

**业界终态判定机制**：靠 **`TaskState` 枚举值**，非 boolean 标记。

| 状态 | 含义 | 终态? |
|---|---|---|
| `TASK_STATE_UNSPECIFIED` | 未知/未指定 | ❌ |
| `TASK_STATE_SUBMITTED` | 已提交 | ❌ |
| `TASK_STATE_WORKING` | 处理中 | ❌ |
| `TASK_STATE_COMPLETED` | 成功 | ✅ 终态 |
| `TASK_STATE_FAILED` | 失败 | ✅ 终态 |
| `TASK_STATE_CANCELED` | 取消 | ✅ 终态 |
| `TASK_STATE_INPUT_REQUIRED` | 需要用户输入 | ❌ 中断 |
| `TASK_STATE_REJECTED` | Agent 拒绝 | ✅ 终态 |
| `TASK_STATE_AUTH_REQUIRED` | 需要认证 | ❌ 中断 |

> 业界实现一律用 `state ∈ {completed, failed, canceled, rejected}` 判断终态（如 mcp-mesh-a2a-stream.ts:300-301、Java A2AStream.java:392-394）。
> Synthia 现有 `executor.rs::is_terminal_event` 已基于 `SessionEnded`（来自 agent 事件），与 SDK 协议侧 `TaskState` 是**两个独立的终态判定层**，不冲突。

### 13.2 核对点 2：`Part.data` 通道与 Synthia `ContentPart`

**核对结果**：`Part.data` 是 A2A v1.0 标准结构化载荷通道，**SDK wire shape 已有 `data` 字段**；**Synthia 服务端 `synthia_provider::ContentPart` 枚举当前没有 `Data` 变体**。

**证据链**：

1. **本地 SDK**：`synthia-web/src/api/a2a-stream.ts:305-313` 已定义 `WirePart { data?: unknown; ... }`，前端识别走 `Part.data` 通道已稳定（tool_call / tool_result 当前即通过 `Part.data = {id,name,input}` / `{tool_use_id,content}` 传递）。

2. **A2A 官方变更**：
   - **v0.x**：`{ kind: "data", data: { key: "value" } }`（带 `kind` 判别字段）
   - **v1.0**：`{ data: { key: "value" } }`（**取消 `kind`**，改用 JSON 成员多态）
   - 语义不变：`Part.data` 携带"应用自定义结构化 JSON"（schema 可通过 `metadata` 或关联 `AgentSkill` 描述）。

3. **Synthia `crates/synthia-provider/src/types/content.rs:144-153`**：
   ```rust
   #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
   #[serde(tag = "type", rename_all = "snake_case")]
   pub enum ContentPart {
       Text(TextContent),
       Image(ImageContent),
       Audio(AudioContent),
       ToolUse(ToolUse),
       ToolResult(ToolResult),
       Reasoning(ReasoningContent),
       Resource(ResourceLink),
   }
   ```
   7 个变体，**无 `Data` 变体** —— 缺口确认。

### 13.3 实施修正（D16 裁定）

基于以上业界核对，本设计的实施修正点如下：

| 修正点 | v2 设计稿表述 | 业界核对后修正 |
|---|---|---|
| **§3.4 SSE 终态字段** | `final: false` | **删除 `final` 字段**。GoalUpdate 终态判定统一由 `TaskState` 判定；不引入 SDK 未定义的字段 |
| **§3.4 `state`** | `TaskState::Working` | 维持不变（业界一致） |
| **§3.5/§4 历史 & wire `Part.data`** | `Part.data = {goals, iteration}` | 维持不变（业界通道正确） |
| **§13 协议层** | 服务端发 `Part::data(json!(...))` | **需要新 `ContentPart::Data` 变体**：服务端 Rust 端要新增 `ContentPart::Data(serde_json::Value)` 变体；serde tag 自动对应 SDK `Part.data` 字段（A2A v1.0 不再带 `kind` 判别，纯字段存在性区分） |

**§3.4 修订文本**：

```rust
// crates/synthia-server/src/a2a/executor.rs —— 主事件循环
AgentEvent::System(SystemEvent::GoalUpdate { goals, iteration }) => {
    // A2A v1.0 wire 形状（D16 业界核对后）：
    //   StreamResponse::StatusUpdate {
    //     taskId, contextId,
    //     status: { state: TaskState::TaskStateWorking,     // SDK 枚举名
    //               message: Some(Message {
    //                 parts: vec![Part { data: {goals, iteration} }],  // ★ 业界通道
    //                 ... }),
    //               timestamp: Some(now) }
    //     // ★ 不带 final 字段 —— v1.0 已移除；终态靠 TaskState
    //   }
    //
    // 注：服务端的 synthia-provider 当前 ContentPart 没有 Data 变体，
    //     需先扩 ContentPart::Data(serde_json::Value) 才能构造 Part::data。
    //     SDK 端（a2a-stream.ts）已经能识别 Part.data 形状，无需改动。
    //
    // 历史写入（ContentPart::Data 同步支持 history）：
    record_event_into_history(&mut history_builder, &event);

    // SSE 发射
    yield StreamResponse::StatusUpdate(/* 上述形状 */);
}
```

### 13.4 风险与权衡更新

| 新风险 | 缓解 |
|---|---|
| `ContentPart::Data` 新增变体破坏现有 serde 兼容 | `#[serde(tag = "type", rename_all = "snake_case")]` 已生效；新增变体类型 tag = `"data"`；现有 7 变体的反序列化不受影响（tagged enum 增加分支安全） |
| 服务端 `Part::data` 在 SDK `toJSON` 形态 | 核对 SDK `Part` 接口的 `toJSON` 输出字段集；新增的 `Data` 变体 JSON 序列化为 `{ "data": <value> }`，与 SDK 期望一致 |
| 历史 `Part.data` 与 v0.x `kind: "data"` 字段歧义 | Synthia 仓库已锁定 SDK v1.0；不存在 v0.x 客户端，无需兼容 |

### 13.5 验收标准更新（§9.1）

新增：
- [ ] `ContentPart::Data(serde_json::Value)` 变体已加入 `crates/synthia-provider/src/types/content.rs`，serde round-trip 测试覆盖
- [ ] `Part::data(json!({"goals": [...], "iteration": 0}))` 构造 → wire JSON = `{"data": {"goals": [...], "iteration": 0}}`（无 `kind` 字段）
- [ ] SDK `Part.toJSON` 反序列化到 `WirePart.data` 后，`looksLikeGoalPayload` 仍能识别

### 13.6 代码导读

| 关键文件 | 角色 |
|---|---|
| `crates/synthia-agent/src/goal.rs` | `Goal` + `GoalStatus` 类型 |
| `crates/synthia-agent/src/events/system_event.rs` | `SystemEvent::GoalUpdate` 变体 + `kind()` arm |
| `crates/synthia-agent/src/events/event_enum.rs` | `is_durable` 方案 α（仅 `GoalUpdate` 例外） |
| `crates/synthia-provider/src/types/content.rs` | **新增** `ContentPart::Data(serde_json::Value)` 变体（D16） |
| `crates/synthia-server/src/a2a/executor.rs` | `record_event_into_history` 扩 arm + SSE `StatusUpdate` 分支（不携带 `final`） |
| `synthia-web/src/api/a2a-stream.ts` | `looksLikeGoalPayload` + `classifyPart` 扩 + `SegmentType` 加 `'goal_update'` |
| `synthia-web/src/state/goals.ts` | reducer + types + 单测 |
| `synthia-web/src/state/useGoals.ts` | hook 两入口（stream / history） |
| `synthia-web/src/components/chat/GoalListPanel.tsx` | 渲染组件 |
| `synthia-web/src/styles/page.css` | 4 个 `nt-pill--*` slug |

**实施入口（D16 调整后）**：
1. `crates/synthia-provider/src/types/content.rs` 加 `ContentPart::Data` + 单测
2. `crates/synthia-agent/src/goal.rs` + `system_event.rs::GoalUpdate` + `event_enum.rs::is_durable` 方案 α
3. `crates/synthia-server/src/a2a/executor.rs` 扩 arm + SSE `StatusUpdate` 分支（不携带 `final`）
4. 客户端识别 + reducer + hook + UI（同 v2 §10）

## 14. 与上游设计稿的差异

| 维度 | A1 稿（v1） | v2 终稿 |
|---|---|---|
| 借鉴幅度规则 | 未明说 | 新增"最小增强的边界规则"（§2） |
| `is_durable` 方案 | β 递归 | α 单 case |
| `iteration` 字段 | 必填 `usize` | 可选 `Option<usize>` |
| `IntentKind` 字段 | 未提 | 明确不做 |
| `useGoals` 文件 | 3 个文件 | 2 个文件 |
| `chatStreamBus.ts` | 列出 | 删除 |
| Server 节流 | 未提 | 明确每轮 ≤ 2 次 |
| 测试矩阵 | 6 + 12 | 7 + 13（+ 1 端到端） |
| 实施顺序 | 5 步 | 4 步（D16 后扩为 5 步：含 `ContentPart::Data` 预备） |
| Goal 字段 | 未限制 | 最小化（id/description/status） |
| SSE `final` 字段 | `final: false` | **删除**（A2A v1.0 官方移除） |
| 终态判定 | 隐含 `SessionEnded` | 显式 `TaskState ∈ {completed, failed, canceled, rejected}`（业界） |
| `ContentPart::Data` 变体 | 未提 | **新增**（服务端 Rust 端必须扩） |

## 15. 业界引用

| 引用 | 来源 |
|---|---|
| A2A v1.0 变更说明（`final` 字段移除、kind 取消、TaskState 命名） | [a2a-protocol.org/latest/whats-new-v1](https://a2a-protocol.org/latest/whats-new-v1/) |
| v0.x `TaskStatusUpdateEvent` 旧定义（含 `final: boolean`） | [agent2agent.info/zh-cn/docs/concepts/task](https://agent2agent.info/zh-cn/docs/concepts/task/) |
| `Part.data` 语义（v0.x 带 `kind`，v1.0 仅字段） | [a2a-protocol.org v0.1.0 specification §6.4](https://a2a-protocol.org/v0.1.0/specification/) |
| mcp-mesh TS/Java 消费者终态严格比较实现 | [mcp-mesh.ai/a2a/surfaces-spec](https://mcp-mesh.ai/a2a/surfaces-spec/) |
| `@a2a-js/sdk` v1.0 本地 d.ts 定义 | `node_modules/@a2a-js/sdk/dist/a2a-Ubve0YhO.d.ts:208-219` |
| Synthia `ContentPart` 7 变体定义 | `crates/synthia-provider/src/types/content.rs:144-153` |