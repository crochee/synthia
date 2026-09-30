<!--
Raw capture of superpowers:brainstorming output.
本檔原樣捕捉 brainstorming 對話產出 — 決策鏈 + 設計取捨。
-->

# 背景

當前 `AgentEvent` enum 有 32 個 variant，分布在 `crates/synthia-agent/src/events/event_enum.rs`。Wire mapping (`crates/synthia-a2a/src/mapping.rs`) 用 ~280 行 `match` 把它们翻译成 A2A `StreamResponse`，主要通过 `metadata.segment_type` 字符串嗅探。Memory 系統維護 ~22 個 variant 的手寫 `is_durable()` 白名單。

## 已發現的問題（代碼層證據）

1. **`LlmReasoningDelta` 是 dead variant** — 構造點存在但 wire 不發，導致 reasoning 內容**靜默丟失** (`crates/synthia-a2a/src/mapping.rs:102-106` 用空 `Part::text("")` + `segment_type=response_complete` 當 marker)
2. **`Finish { output }` 是 dead variant** — 宣告 + `let _ = content` 註解 (`mapping.rs:175` 仍然走它)
3. **`Status(AgentStatus)` 在 mapping 被 catch-all 丟棄** — 沒有專門分支
4. **Anthropic `signature_delta` 在 streaming 解析中被丟棄** — `crates/synthia-provider/src/streaming/anthropic/v2.rs:75` 直接忽略，跨 turn reasoning continuity 已斷
5. **`text_deltas: Vec<String>` 只 push Text 不 push Reasoning** — `StreamAccumulator` (`crates/synthia-agent/src/events/stream.rs:18`) 漏掉了 reasoning channel

## 業界證據（2026-07）

- **Anthropic 原生 channel**：獨立 content_block + `thinking_delta` / `text_delta` 嚴格匹配 block type。`signature_delta` 是 thinking block 末尾必發，**必須原樣回傳** 否則跨 tool turn 的 reasoning continuity 會斷
- **OpenAI o-series responses API**：`response.output = [{type: "reasoning"}, {type: "message"}]`，結構化區分
- **OpenAI inline `<think>` 標籤**：deprecated 模式，agent 層不該 sniff。Provider 透明透傳

# 決策鏈

## Q1: ToolResult is_error 三態 vs ToolStatus enum
**決策**：保留 `Option<bool>` 三態，復用 Provider `ToolResult { is_error: Option<bool> }`
- `None` = stream 沒完成
- `Some(true)` = 錯誤
- `Some(false)` = 成功

## Q2: ToolUse id 保留
**決策**：保留 Provider `ToolUse { id }`，用於配對 ToolResult

## Q3: 持久化（Resource vs Image/Audio）
**決策**：
- `ContentPart::Resource` **durable**（URL 引用持久）
- `ContentPart::Image/Audio` **transient**（base64 太重，前端獨立緩存 attachment）

## Q4: ToolUse/ToolResult 原子事件
**決策**：Provider streaming 累積成完整 `ContentPart::ToolUse(ToolUse { id, name, input })` 後 agent emit，agent 不處理 stream delta

## Q5: Model 流式 chunk
**決策**：保留流式 delta，`Model(ContentPart::Text("..."))` 是 chunk 級別，前端打字機消費

## Q6/Q7: Custom event 位置
**決策**：歸 `HookEvent::Custom`，保持頂層 5 variant

## Q8: signature 字段
**決策**：本次做。引入 `ReasoningContent { text, signature: Option<String> }` 替代 `TextContent` 作為 `ContentPart::Reasoning` 內部類型。`Anthropic v2` streaming 解析 `signature_delta` 並 attach

## Q9: AgentTraceMeta 結構化
**決策**：
```rust
pub struct AgentMeta {
    pub parent_session_id: String,
    pub child_session_id: String,
    pub parent_depth: usize,
}
```
兩個 session id 都保留（child 知道自己 parent 是誰，parent 想知道 child 是誰）

## Q10: Fatal 刪除
**決策**：`Fatal` variant 刪除，所有終止走 `SessionEnded(Failed(msg))`（Fatal 信息塞進 `SessionEndReason::Error`）

## Q11/Q12: A2A Part::data 替代 segment_type
**決策**：用 `Part::data(json!({kind, ...payload}))` 替代 `Part::text("") + metadata.segment_type`。`mapping.rs:102-106` 的 hack 完全替換掉

## Q13: ModelDone + memory 流式合併
**決策**：
- 保留 `ModelDone(SamplingResult)` 作為 wire event（攜帶 usage）
- **Memory 不持久化 ModelDone**
- Query 時按"事件類型變化"判斷流段邊界

## Q15: Reasoning 持久化
**決策**：**不寫 memory**（思考是過程，正文才是事實）

## Q21: AgentTrace 簡化
**決策**：`Agent(AgentMeta, Box<AgentEvent>)` 一對 tuple，刪 `AgentTrace` 子 enum。Started/Event/Completed/Failed 全靠 `Box<AgentEvent>` 表達

## Q23: SessionEndReason
**決策**：保留現有全部 variant，Fatal 信息塞 `Error(msg)`

## Q24: HookError 歸 System::Warning
**決策**：`HookError` → `System(SystemEvent::Warning { kind: Hook })`

## Q25: SelfReflection 作為工具
**決策**：`AgentEvent::SelfReflection` variant 完全刪除，走 `Model(ContentPart::ToolUse { name: "self_reflection" })` 路徑

## Q26: EditConflict 歸 System::Warning
**決策**：`EditConflict` → `System(SystemEvent::Warning { kind: EditConflict })`

## Q27: Status variant 刪
**決策**：`AgentEvent::Status(AgentStatus)` 刪。task status 完全從 `SessionEvent` 推導。註：`mapping.rs:30` 文檔註釋有 bug（`SessionInterrupted → Canceled` 應為 `InputRequired`），本次順手修

## Q28: IterationStarted/Completed 刪
**決策**：iteration 是內部概念，不進 wire

## Q29: Checkpoint 刪
**決策**：Checkpoint 是 internal state，不影響會話意圖

## Q30: StateChange 刪
**決策**：`StateChange` 是 noise，SessionStarted/Ended 已表達狀態邊界

## Q33: Part::text 用法約束
**決策**：只有 A2A `TaskStatusUpdate.message` 字段用 `Part::text`（人類可讀的狀態說明）。AgentEvent payload 一律 `Part::data` 或 `Part::file`

## Q34: TaskState ≡ SessionState
**決策**：task 跟 session 是同一回事，task status 完全從 `SystemEvent::Session*` 推導。**沒有** `AgentStatus` 概念在 AgentEvent 層

# 設計取捨

| 維度 | 當前 | 提議 | 收益 |
|------|------|------|------|
| AgentEvent variant 數 | 32 | 5 | -84% |
| mapping.rs 行數 | ~280 | ~80 | -71% |
| is_durable 白名單 | 22 variant | match 表達式 | 靜態類型保護 |
| Reasoning wire | 靜默丟失 | 真正透傳 | 跨 turn reasoning continuity |
| Wire schema | `metadata.segment_type` 字符串 | `Part::data` typed JSON | A2A 標準、JSON 字段類型穩定 |
| 前端 dispatch | 字符串嗅探 10+ case | `JSON.parse(data)` 1 step | -90% |

# 最終 schema

## AgentEvent

```rust
pub enum AgentEvent {
    Model(ContentPart),
    ModelDone(SamplingResult),
    System(SystemEvent),
    Agent(AgentMeta, Box<AgentEvent>),
    Hook(HookEvent),
}
```

## SystemEvent

```rust
pub enum SystemEvent {
    SessionStarted { session_id: String },
    SessionEnded { reason: SessionEndReason },
    SessionInterrupted { reason: String },
    Progress { message: String, step: usize, total: usize },
    Warning { kind: WarningKind, message: String, iteration: Option<usize> },
    Recovery { level_number: u32, tool_name: Option<String>, message: String, iteration: Option<usize> },
    Usage { input_tokens: usize, output_tokens: usize, cache_read_tokens: Option<usize>, cache_creation_tokens: Option<usize> },
}
```

## WarningKind

```rust
pub enum WarningKind {
    Guardian, Loop, TokenBudget, ContextCompaction, Hook, EditConflict,
}
```

## SessionEndReason（保留現有全部）

```rust
pub enum SessionEndReason {
    Completed, Cancelled, Error(String),
    TokenBudgetExceeded, MaxIterationsReached,
    GuardianBlocked, LoopDetected, CircuitBreakerOpen,
}
```

## AgentMeta

```rust
pub struct AgentMeta {
    pub parent_session_id: String,
    pub child_session_id: String,
    pub parent_depth: usize,
}
```

## HookEvent

```rust
pub enum HookEvent {
    Message { priority: i32, message: String },
    ConfirmRequest { tool_use_id: String, tool_name: String, reason: String },
    ConfirmResponse { approved: bool, tool_use_id: String },
    Custom { kind: String, data: serde_json::Value },
}
```

## ReasoningContent（Provider 層新增）

```rust
pub struct ReasoningContent {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
}

pub enum ContentPart {
    Text(TextContent),
    Reasoning(ReasoningContent),
    Image(ImageContent),
    Audio(AudioContent),
    Resource(ResourceLink),
    ToolUse(ToolUse),
    ToolResult(ToolResult),
}
```

## 持久化表

| Path | is_durable |
|------|-----------|
| `Model(Text)` | true |
| `Model(Reasoning)` | false |
| `Model(ToolUse)` | true |
| `Model(ToolResult)` | true |
| `Model(Image/Audio)` | false |
| `Model(Resource)` | true |
| `ModelDone` | false |
| `System(*)` | false |
| `Agent(_, _)` | false |
| `Hook(*)` | false |

# 變更範圍

```
title: simplify-agent-event-stream

scope:
  1. Provider: ReasoningContent { text, signature } 替代 TextContent
  2. Anthropic streaming v2: 解析 signature_delta
  3. AgentEvent: 5 頂層 enum + 子 enum 重構
  4. StreamAccumulator: Vec<ContentPart> 流式
  5. mapping.rs: Part::data 替代 segment_type
  6. is_durable() 重寫
  7. synthia-event-v2: 白名單更新
  8. frontend: dispatch 改 part.kind
  9. spec 修訂（6 個）
  10. test fixture 更新

out of scope:
  - Prompt-cache key generation
  - Memory compaction strategy
  - Multi-turn reasoning block replay
```

# 風險

- Wire 改變 → 前端必須同步改 → 灰度發布
- is_durable 決策變 → memory 行為變 → replay 測試必備
- 32 個 variant 刪除 → 大量 test fixture 更新