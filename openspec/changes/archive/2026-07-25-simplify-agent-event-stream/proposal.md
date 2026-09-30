## Why

當前 `AgentEvent` 有 32 個 variant，其中 `LlmReasoningDelta`、`Finish`、`Status(AgentStatus)` 是 dead code——mapping 不發或被 catch-all 丟棄。Anthropic `signature_delta` 在 streaming 解析中被默默丟棄，跨 turn reasoning continuity 已斷；`StreamAccumulator.text_deltas: Vec<String>` 漏推 Reasoning 內容。Wire mapping (`mapping.rs`) 用 ~280 行 `match` 通過 `metadata.segment_type` 字符串嗅探分發事件，Memory 系統維護 ~22 個 variant 的手寫 `is_durable()` 白名單。重構為 5 頂層 enum + 子 enum 後預期變體數 -84%、mapping -71%、reasoning 真正透傳、wire 改用 A2A `Part::data` 標準 typed JSON。

## What Changes

**AgentEvent 結構**
- From: 32 個 variant（`LlmStreamDelta`, `LlmReasoningDelta`, `LlmResponseComplete`, `LlmError`, `Thinking`, `ToolCallStarted/Completed/Skipped/Error`, `IterationStarted/Completed`, `Checkpoint`, `StateChange`, `ContextCompacted`, `RecoveryApplied`, `Finish`, `GuardianWarning`, `LoopWarning`, `TokenBudgetWarning/Notice`, `SteeringReceived`, `HookError`, `GuardianConfirmationRequest`, `EditConflict`, `SelfReflection`, `Subagent*×5`, `Status`, `Warning`, `Progress`, `Custom`）
- To: 5 個頂層 variant（`Model(ContentPart)`, `ModelDone(SamplingResult)`, `System(SystemEvent)`, `Agent(AgentMeta, Box<AgentEvent>)`, `Hook(HookEvent)`）+ 4 個子 enum
- Reason: 復用 Provider 結構 + 子 enum 各自獨立擴展點
- Impact: breaking (AgentEvent 是 wire-protocol 的一部分)

**Reasoning 區分**
- From: Reasoning 與 Text 都走 `text_deltas` 漏推，wire 端 `LlmReasoningDelta` 是 dead code
- To: Provider 維持結構化區分（`ContentPart::Reasoning` vs `ContentPart::Text`），agent 透傳不重組
- Reason: 對齊業界（Anthropic 原生 channel + OpenAI responses API 結構化 reasoning）
- Impact: reasoning 內容現在真正進 wire

**Signature 處理**
- From: Anthropic `signature_delta` 在 streaming 解析中被丟棄，跨 turn reasoning continuity 斷
- To: Provider `ReasoningContent { text, signature: Option<String> }` 替代 `TextContent`，Anthropic v2 解析後 attach
- Reason: 修復已存在的 silent bug
- Impact: breaking (ContentPart::Reasoning 內部類型變更)

**Wire 形態**
- From: `Part::text("") + metadata.segment_type = "response_complete"` 等字符串嗅探（`mapping.rs:102-106` hack）
- To: 統一 `Part::data(json!({kind, ...payload}))`，前端通過 `JSON.parse(part.data).kind` 分發
- Reason: 對齊 A2A 標準 typed Part，移除字符串嗅探 hack
- Impact: breaking (wire schema 變更)

**Task status**
- From: `AgentEvent::Status(AgentStatus)` variant + mapping 文檔註釋把 `SessionInterrupted` 映射為 `Canceled` (bug)
- To: TaskState 完全從 `SystemEvent::Session*` 推導，mapping 文檔修正為 `InputRequired`
- Reason: task ≡ session 同一概念，沒有獨立 AgentStatus
- Impact: breaking (Status variant 刪除)

**is_durable 持久化規則**
- From: 手寫白名單 ~22 variant，未知 variant 默認 durable (spec `event-durability-classification` 第二條)
- To: 顯式 match 表達式：
  - durable: `Model(Text/ToolUse/ToolResult/Resource)`
  - NOT durable: `Model(Reasoning/Image/Audio)`, `ModelDone`, `System(*)`, `Agent(_, _)`, `Hook(*)`
- Reason: 靜態類型保護 + Spec 第二條修訂
- Impact: memory 行為變更（reasoning / attachment 不寫）

**AgentEvent 變體映射（舊 → 新）**
| 舊 variant | 新路徑 |
|------------|--------|
| `LlmStreamDelta` | `Model(ContentPart::Text)` |
| `LlmReasoningDelta` | `Model(ContentPart::Reasoning)` |
| `LlmResponseComplete` | `ModelDone(SamplingResult)` |
| `LlmError` | `System(SystemEvent::SessionEnded(reason=Error(msg)))` |
| `Thinking` | `Model(ContentPart::Reasoning)` |
| `ToolCallStarted` | `Model(ContentPart::ToolUse)` |
| `ToolCallCompleted` | `Model(ContentPart::ToolResult { is_error: Some(false) })` |
| `ToolCallSkipped` | `Hook(HookEvent::ConfirmResponse { approved: false, tool_use_id })` |
| `ToolCallError` | `Model(ContentPart::ToolResult { is_error: Some(true) })` |
| `IterationStarted/Completed` | (deleted, internal) |
| `Checkpoint` | (deleted, internal) |
| `StateChange` | (deleted, redundant) |
| `ContextCompacted` | `System(SystemEvent::Warning { kind: ContextCompaction })` |
| `RecoveryApplied` | `System(SystemEvent::Recovery)` |
| `Finish` | (deleted, dead code) |
| `GuardianWarning` | `System(SystemEvent::Warning { kind: Guardian })` |
| `LoopWarning` | `System(SystemEvent::Warning { kind: Loop })` |
| `TokenBudgetWarning/Notice` | `System(SystemEvent::Warning { kind: TokenBudget })` |
| `SteeringReceived` | `Hook(HookEvent::Message)` |
| `HookError` | `System(SystemEvent::Warning { kind: Hook })` |
| `GuardianConfirmationRequest` | `Hook(HookEvent::ConfirmRequest)` |
| `EditConflict` | `System(SystemEvent::Warning { kind: EditConflict })` |
| `SelfReflection` | `Model(ContentPart::ToolUse { name: "self_reflection" })` |
| `Subagent*×5` | `Agent(AgentMeta, Box<AgentEvent>)` |
| `Status(AgentStatus)` | (deleted, derived from SessionEvent) |
| `Warning` | `System(SystemEvent::Warning)` |
| `Progress` | `System(SystemEvent::Progress)` |
| `Custom` | `Hook(HookEvent::Custom)` |

## Capabilities

### New Capabilities
- `agent-event-bus`: AgentEvent 5 頂層 enum + 子 enum 結構 + wire 形態 + is_durable 持久化規則
- `provider-anthropic-signature`: Anthropic streaming signature_delta 解析 + ReasoningContent.signature 字段

### Modified Capabilities
- `event-durability-classification`: is_durable 白名單替換為新 5 變體 match 表
- `subagent-event-bridge`: `SubagentEvent { child_session_id, event }` 改 `Agent(AgentMeta, Box<AgentEvent>)`
- `subagent-background-mode`: 移除 `AgentEvent::Subagent*` 變體依賴
- `recovery-cascade-wiring`: `AgentEvent::RecoveryApplied` 改 `System(SystemEvent::Recovery)` 獨立變體
- `self-reflection-hotmemory`: `AgentEvent::SelfReflection` 變體刪除，改走 ToolUse
- `custom-event-renderer`: `AgentEvent::Custom` 改 `Hook(HookEvent::Custom)`

## Impact

**代碼層**:
- `crates/synthia-agent/src/events/event_enum.rs` — 完全重寫 (32 → 5 頂層)
- `crates/synthia-agent/src/events/stream.rs` — `text_deltas: Vec<String>` → `deltas: Vec<ContentPart>`
- `crates/synthia-agent/src/events/reasons.rs` — `SessionEndReason` 保留，結構不變
- `crates/synthia-a2a/src/mapping.rs` — 280 行 → ~80 行，全部改用 `Part::data`
- `crates/synthia-provider/src/types/content.rs` — `ContentPart::Reasoning(TextContent)` → `ContentPart::Reasoning(ReasoningContent)`
- `crates/synthia-provider/src/streaming/anthropic/v2.rs` — 解析 `signature_delta` 並 attach
- `crates/synthia-server/src/sse.rs` — variant name 字符串更新
- `synthia-web/src/` 前端 dispatch 改為 `part.kind` JSON.parse
- `crates/synthia-event-v2` — `is_durable_event_type` 白名單更新

**測試 fixture**:
- `crates/synthia-agent/tests/e2e_llm_test.rs`
- `crates/synthia-agent/tests/e2e_cli_test.rs`
- `crates/synthia-server/tests/e2e_registry_pipeline_test.rs`
- `crates/synthia-a2a/src/mapping.rs` 內部測試
- `crates/synthia-cli/src/repl_core/repl/format_event.rs`

**規格影響**:
- 6 個現有 spec 修改
- 2 個新 spec 建立

**風險**:
- Wire 改變 → 前端必須同步改 → 灰度發布
- is_durable 決策變 → memory 行為變 → replay 測試必備
- 32 個 variant 刪除 → 大量 test fixture 更新