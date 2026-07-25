## Context

當前 `AgentEvent` enum (`crates/synthia-agent/src/events/event_enum.rs`) 有 32 個 variant，分散在 agent loop、tool execution、recovery、subagent、hook 等多個關注點。`crates/synthia-a2a/src/mapping.rs` 用 ~280 行 `match` 把 32 個 variant 翻譯成 A2A `StreamResponse`，主要通過 `metadata.segment_type` 字符串嗅探分發。Memory 系統 (`crates/synthia-event-v2`) 維護手寫 `is_durable_event_type()` 白名單覆蓋 ~22 個 variant 名。

**已發現的具體問題（代碼層證據）**：

| 問題 | 位置 | 影響 |
|------|------|------|
| `LlmReasoningDelta` 是 dead variant | `event_enum.rs` + `mapping.rs:97-112` 用 `Part::text("")` 當 marker | Reasoning 內容**靜默丟失** |
| `Finish { output }` 是 dead variant | `mapping.rs:175` + `let _ = content` 註解 | `subagent_factory.rs:184` 仍構造它 |
| `Status(AgentStatus)` 在 mapping 被 catch-all 丟棄 | `mapping.rs:79` 列舉但無分支 | 無 wire 表達 |
| `signature_delta` 在 streaming 解析中被丟棄 | `crates/synthia-provider/src/streaming/anthropic/v2.rs:75` 直接忽略 | 跨 turn reasoning continuity 斷 |
| `text_deltas: Vec<String>` 只 push Text | `crates/synthia-agent/src/events/stream.rs:18` | Reasoning delta 漏推 |
| `mapping.rs:30` 文檔註釋有 bug | `SessionInterrupted → Canceled` 應為 `InputRequired` | A2A wire 行為錯 |

**業界對齊**（2026-07）：
- Anthropic SSE：`thinking_delta` / `text_delta` 必須匹配獨立 content_block type；`signature_delta` 必須保留原樣回傳
- OpenAI o-series responses API：結構化 `type: "reasoning"` 與 `type: "message"` 並列
- OpenAI inline `<think>` 標籤是 deprecated 模式，agent 層不該 sniff

**約束**：
- AgentEvent 是 wire-protocol 的一部分，wire 改變需要前後端同步
- `synthia-event-v2` 的 spec 規定 "Process unknown events as durable" (safe default)，新決策需要修改 spec
- Provider 類型跨 crate 共享（`ContentPart` 在 synthia-provider 定義，agent 透傳）

## Goals / Non-Goals

**Goals**:
- AgentEvent variant 數從 32 → 5（-84%）
- mapping.rs 從 ~280 行 → ~80 行（-71%）
- 修復 `signature_delta` 解析（跨 turn reasoning continuity）
- 修復 `text_deltas` 漏推 Reasoning 的 bug
- Reasoning 內容真正進 wire，前端可見
- Wire 用 A2A 標準 `Part::data` typed JSON，移除 `metadata.segment_type` 字符串嗅探
- is_durable 決策改為顯式 match 表達式（靜態類型保護）
- 子 enum 各自獨立擴展點（SystemEvent/WarningKind/HookEvent/AgentMeta）
- 前端 dispatch 簡化為 `JSON.parse(part.data).kind`

**Non-Goals**:
- Prompt-cache key generation（獨立 change）
- Memory compaction strategy（獨立 change）
- Multi-turn reasoning block replay（需要 Provider message history signature passthrough，獨立 change）
- 改 A2A protocol 本身
- 改 Provider 的 wire format（如 OpenAI/Anthropic API contract）

## Decisions

### D1：頂層 enum 5 個 variant（Q21）

- **選擇**：`AgentEvent { Model(ContentPart), ModelDone(SamplingResult), System(SystemEvent), Agent(AgentMeta, Box<AgentEvent>), Hook(HookEvent) }`
- **理由**：
  - 復用 Provider `ContentPart` 作為 Model 載體，agent 不重組 Text/Reasoning/Image/ToolUse
  - 子 enum 獨立擴展（System/Hook 各自演進不影響頂層）
  - `Agent(AgentMeta, Box<AgentEvent>)` 一對 tuple 比 `AgentTrace { Started/Event/Completed/Failed }` 子 enum 更精簡
- **已考慮 alternative**：
  - 8-12 個頂層 variant（更細分類）— 拒絕，子 enum 已經給擴展點
  - 完全扁平 32 個 variant（不變）— 拒絕，當前狀態正是要解決的問題

### D2：Reasoning 與 Text 維持結構化區分（業界對齊）

- **選擇**：`ContentPart::Text(TextContent)` 與 `ContentPart::Reasoning(ReasoningContent)` 各自獨立 variant，agent 透傳
- **理由**：
  - Anthropic 原生 channel 獨立 content_block；OpenAI responses API 結構化 `type: reasoning`
  - agent 層 sniff `<think>` 標籤是 deprecated 模式
  - Reasoning 不持久化但不丟失（StreamAccumulator 累積，wire 透傳）
- **已考慮 alternative**：
  - 合併到 `ContentPart::Text` 加 `is_reasoning: bool` flag — 拒絕，破壞 Provider 結構
  - 提取 Reasoning 為頂層 `AgentEvent::Reasoning` — 拒絕，與 Model 同級反而割裂

### D3：ReasoningContent 新增 signature 字段（Q8）

- **選擇**：
  ```rust
  pub struct ReasoningContent {
      pub text: String,
      #[serde(skip_serializing_if = "Option::is_none", default)]
      pub signature: Option<String>,
  }
  pub enum ContentPart { Reasoning(ReasoningContent), ... }
  ```
- **理由**：
  - 修復已存在的 silent bug（`signature_delta` 被丟）
  - Anthropic signature 是 thinking block 末尾必發，跨 tool turn reasoning continuity 必須保留
  - OpenAI 不發 signature，Option 處理三態
- **已考慮 alternative**：
  - 在 `SamplingResult` 加 `reasoning_signature: Option<String>` — 部分採用，agent 層聚合用，但不替代 Provider 內 ContentPart 攜帶
  - 完全刪掉 reasoning — 拒絕，破壞 reasoning continuity

### D4：AgentMeta 結構化（Q9）

- **選擇**：
  ```rust
  pub struct AgentMeta {
      pub parent_session_id: String,
      pub child_session_id: String,
      pub parent_depth: usize,
  }
  ```
- **理由**：
  - parent 與 child 兩個 session id 都保留（child 知道自己是誰，parent 想知道事件從哪來）
  - `parent_depth` 用於限制嵌套深度，現有 `SubagentSessionFactory::run_child(parent_depth)` 已有傳入
  - 未來可加 `agent_role: Option<String>`, `task_summary: Option<String>` 不破壞兼容
- **已考慮 alternative**：
  - 只保留 `child_session_id` — 拒絕，subagent bridge spec 顯式需要 parent
  - tuple `(String, String)` — 拒絕，無字段名調用點不清晰

### D5：Fatal variant 完全刪除（Q10）

- **選擇**：`AgentEvent::System(SystemEvent::Fatal)` 變體不存在；終止錯誤走 `System(SystemEvent::SessionEnded(reason: Error(msg)))`
- **理由**：
  - SessionEndReason::Error 已能承載錯誤信息
  - 兩個變體表達同一概念是冗餘
- **已考慮 alternative**：
  - Fatal 與 SessionEnded(Failed) 並存 — 拒絕，記憶層與 UI 都要兩處判斷

### D6：A2A Part::data 替代 metadata.segment_type（Q11/Q12）

- **選擇**：所有 AgentEvent payload 用 `Part::data(json!({kind, ...payload}))`；`Part::text("")` hack 刪除
- **理由**：
  - A2A `Part::data` 是 typed JSON（`serde_json::Value`），前端可 `JSON.parse(part.data)` 拿到 typed 結構
  - 移除 `mapping.rs:102-106` 空 text marker hack
  - 前端 dispatch 從 `metadata.segment_type` 字符串嗅探 10+ case 變成單一 `data.kind` 字段 switch
- **已考慮 alternative**：
  - 保留 `Part::text(content) + metadata.segment_type` — 拒絕，當前問題的根因
  - 自定義 A2A extension — 拒絕，破壞 A2A 標準

### D7：Part::text 用法約束（Q33 修訂）

- **選擇**：AgentEvent payload 一律 `Part::data` 或 `Part::file`；`Part::text` 僅用於 A2A `TaskStatusUpdate.message` 字段（人類可讀狀態說明）
- **理由**：
  - 模型輸出是結構化數據，不該走人類可讀 text channel
  - StatusUpdate.message 是給用戶看的錯誤/中斷說明
- **已考慮 alternative**：
  - 保留 Part::text 給 model final message — 拒絕，無此事件

### D8：TaskState 完全從 SessionEvent 推導（Q34 修訂）

- **選擇**：`AgentEvent::Status(AgentStatus)` 變體刪除；mapping 從 `System(SystemEvent::Session*)` 直接推導 A2A TaskState
- **理由**：
  - task 與 session 是同一概念
  - mapping 已如此實現（`mapping.rs:42-90`）
  - 文檔註釋 `mapping.rs:30` 有 bug（`SessionInterrupted → Canceled` 應為 `InputRequired`），本次順手修
- **已考慮 alternative**：
  - 保留 `Status(AgentStatus)` 並 map — 拒絕，是 mapping catch-all 丟棄的 dead variant

### D9：is_durable 顯式 match 表（持久化規則）

- **選擇**：
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
- **理由**：
  - 靜態類型保護，新變體必須顯式 match
  - Reasoning / Image/Audio 不持久化：思考是過程、attachment base64 太重
  - ModelDone 不持久化：usage 是 billing metadata，內容已被 stream delta 覆蓋
  - System/Hook 不持久化：不影響會話決策/意圖
  - Resource 是 durable：URL 引用持久
- **已考慮 alternative**：
  - 復用 spec 默認 "unknown = durable" — 拒絕，新設計無 unknown variant，不需要 safe default
  - 全 durable — 拒絕，存儲爆炸（reasoning/attachment）

### D10：ToolResult is_error 保留 Provider 三態（Q1）

- **選擇**：透傳 `Provider::ToolResult { is_error: Option<bool] }`；agent 不自定義 ToolStatus enum
- **理由**：
  - 復用 Provider 結構原則
  - None/Some(true)/Some(false) 三態語義清晰
  - Skipped 不發 ToolResult，改發 `Hook(HookEvent::ConfirmResponse { approved: false, tool_use_id })`
- **已考慮 alternative**：
  - agent 自定義 `enum ToolStatus { Ok, Skipped, Err }` — 拒絕，破壞 Provider 復用

### D11：StreamAccumulator 改造

- **選擇**：
  ```rust
  pub struct StreamAccumulator {
      text: String,
      reasoning: String,
      reasoning_signature: Option<String>,
      tool_calls: Vec<ToolUse>,
      deltas: Vec<ContentPart>,       // ← Vec<ContentPart> 替代 Vec<String>
      usage: Option<TokenUsage>,
  }
  ```
- **理由**：
  - `deltas: Vec<ContentPart>` 同時 push Text/Reasoning/ToolUse，mapping 根據 ContentPart variant 分發
  - 修復當前只 push Text 漏推 Reasoning 的 bug
- **已考慮 alternative**：
  - `text_deltas: Vec<String>, reasoning_deltas: Vec<String>` 雙 Vec — 拒絕，agent 層不該知道細分

### D12：mapping.rs 簡化（80 行目標）

- **選擇**：5 個頂層 variant → 5 段 match；每個子 enum 內部 match
- **理由**：
  - 從 280 行 → 80 行
  - 每段結構一致：`match event { X(...) => wire_message(...), Y(...) => wire_status_update(...) }`
  - Part::data 統一構造，無 metadata.segment_type 字符串複製

### D13：前端 dispatch 改造

- **選擇**：
  ```typescript
  function getEventKind(parts: Part[]): { kind: string, payload: any } | null {
    const data = parts.find(p => p.type === 'data');
    if (!data) return null;
    return { kind: data.data.kind, payload: data.data };
  }
  ```
- **理由**：
  - 從 10+ case metadata.segment_type 嗅探 → 單一 JSON.parse
  - 字段類型由 serde JSON schema 保證，前端不會因字段名拼錯出錯
- **已考慮 alternative**：
  - 保留舊 dispatch 通過 metadata.segment_type 兼容 — 拒絕，本 change 是 breaking change

### D14：ToolResult content 多模態處理

- **選擇**：mapping 把 `ToolResult.content: Vec<ContentPart>` flatten 成 string（join Text parts）；不傳遞嵌套 ContentPart
- **理由**：
  - UI 不懂 ContentPart 嵌套
  - ToolResult 在模型視角是純文本反饋
- **已考慮 alternative**：
  - wire 上保留 ContentPart 數組 — 拒絕，前端複雜度爆炸

### D15：Agent(AgentMeta, Box<AgentEvent>) 嵌套表達

- **選擇**：所有 subagent 事件（包括 Started）作為 inner event：
  - `Agent(meta, Box::new(AgentEvent::System(SystemEvent::SessionStarted{..})))` 表示子 agent 啟動
  - `Agent(meta, Box::new(AgentEvent::Model(ContentPart::Text(..))))` 表示子 agent 輸出
  - `Agent(meta, Box::new(AgentEvent::System(SystemEvent::SessionEnded(..))))` 表示子 agent 結束
- **理由**：
  - Q21 簡化原則
  - SessionStarted/Ended 已經在 SystemEvent，子 agent 不需要獨立 Started/Completed variant
- **已考慮 alternative**：
  - 保留 `AgentTrace { Started, Event, Completed, Failed }` — 拒絕，Q21 明確要求簡化

## Risks / Trade-offs

[Risk] **Wire 變更破壞前後端同步** → Mitigation: 灰度發布；前後端同步 PR；wire schema 變更前先 freeze 兩週觀察舊客戶端

[Risk] **is_durable 決策變更導致 memory 缺失** → Mitigation: replay 測試必備；對比新舊決策下 session replay 結果

[Risk] **signature 處理路徑未測試** → Mitigation: 為 Anthropic v2 streaming 加 e2e 測試（multi-turn 帶 signature）

[Trade-off] **`AgentMeta` 雙 session id 字段冗餘** → 接受：subagent 與 parent 都需要明確知道對方 id，用於 replay tree 構建

[Trade-off] **`Part::data` JSON 字段順序由 serde_json::Value::Object 決定** → 接受：序列化穩定（按 key 字母序），前端不受影響

[Trade-off] **`SystemEvent::Usage` 替代 `ModelDone.usage`** → 接受：usage 是 billing metadata 不會話事實；`ModelDone` 仍攜帶 usage 給 wire（前端 UI 顯示 token 數），但 memory 不寫

[Risk] **多 spec 同時修改** → Mitigation: 拆成單獨 PR 但同一 change；spec review 集中

## Migration Plan

**部署順序**：

1. **Phase 1 — Provider 層先建**（獨立可合併）
   - 添加 `ReasoningContent { text, signature }` 結構
   - Anthropic v2 streaming 解析 `signature_delta`
   - Provider 內部測試覆蓋
   
2. **Phase 2 — AgentEvent 重構**（核心改動）
   - 重寫 `event_enum.rs` 從 32 → 5 variant
   - StreamAccumulator 改造 `Vec<ContentPart>`
   - `is_durable()` 重寫為顯式 match
   - synthia-event-v2 白名單更新
   
3. **Phase 3 — Wire mapping 替換**
   - `mapping.rs` 全部用 `Part::data`
   - 修復 `mapping.rs:30` 文檔註釋 bug
   - mapping 內部測試重寫
   
4. **Phase 4 — 前端同步**
   - `synthia-web` dispatch 改為 `JSON.parse(part.data).kind`
   - 前端 type 定義更新
   
5. **Phase 5 — Spec 修訂**
   - 6 個現有 spec 修改
   - 2 個新 spec（agent-event-bus, provider-anthropic-signature）建立
   - `event-durability-classification` 第二條修訂
   
6. **Phase 6 — Test fixture 更新**
   - `crates/synthia-agent/tests/e2e_llm_test.rs`
   - `crates/synthia-agent/tests/e2e_cli_test.rs`
   - `crates/synthia-server/tests/e2e_registry_pipeline_test.rs`
   - `crates/synthia-cli/src/repl_core/repl/format_event.rs`
   - `crates/synthia-server/src/sse.rs`

**回滾策略**：
- 6 個 spec 修改為獨立 commit，可 revert
- Provider 層 ReasoningContent 與 TextContent 並行期可保留 compat struct（deprecated）
- mapping 變更是 breaking，必須 atomic commit

**驗收條件**：
- `cargo build --workspace` 通過
- `cargo test --workspace` 通過
- 新增 Anthropic signature e2e 測試
- 前端手動驗證 reasoning 內容顯示
- memory replay 對比測試（舊白名單 vs 新 match 表）

## Open Questions

1. **delta 字段名**：wire JSON 用統一 `delta` 字段 vs 各 ContentPart variant 自己字段名（`text_delta.text` / `thinking_delta.thinking`）
   - 提議：統一 `delta`（前端 dispatch 簡單）
   - 待確認

2. **Usage 字段是否拆出來獨立 wire event**：`ModelDone` 攜帶 vs `SystemEvent::Usage` 獨立
   - 提議：`ModelDone` 攜帶（前端一次拿完整信息）
   - 待確認

3. **舊 spec archive 處理**：`event-durability-classification` 修改後是否歸檔舊版本
   - 提議：保持單一文件，git history 保留
   - 待確認