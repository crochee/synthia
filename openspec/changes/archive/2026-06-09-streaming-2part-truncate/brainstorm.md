<!--
Raw capture of brainstorming session for "streaming-2part-truncate" change.

本檔原樣捕捉 brainstorming 對話的決策鏈。Skill 的自然產出：背景 → 決策鏈 Q1-Qn → 設計取捨。

設計本身在 `docs/superpowers/specs/2026-06-07-streaming-2part-truncate-design.md` 完整呈現。
本檔作為它的決策源頭,記錄「為什麼選 A 不選 B」的推理鏈,供日後追溯。

不要把本檔內容複製到 design.md — design.md 是結構化重組後的設計文件。
-->

# Brainstorming: streaming-2part-truncate

**日期**: 2026-06-07 → 2026-06-08
**主持人**: User
**產出**: 設計 spec + 本決策日誌
**skill**: brainstorming + 多專家對抗性分析

---

## 背景 (Background)

User 要求對 synthia-agent 與生產級 AI agent (opencode 和 codex) 進行差距分析,
重點是 opencode,目標是「取長補短」: 借鑑優秀實現/架構/邏輯。

產出物要求: **分析 + 首個改進的完整 spec**(用戶選擇)。

首個改進選擇: **Streaming + 2-part prompt + Truncate 服務**。

---

## 決策鏈 (Decision Chain)

### Q1. 首個改進選哪個?

候選: 3 個改進(差距分析中排序)
1. Streaming + 2-part prompt + Truncate 服務
2. (其他兩個略)
3. (其他兩個略)

**決議**: 選 1 (用戶拍板) — 因為它能一次解決:
- 3 個真 bug (Bug 1 thinking-tag 嗅探壞掉 / Bug 2 O(n²) 全量累積 / Bug 3 信息黑洞)
- 4 處冗餘/死代碼
- 1 處協議錯位 (流所有權在 provider)
- 同時引入 prefix cache 優化 (2-part prompt) 和可預測成本 (Truncate 落盤)

---

### Q2. 集成策略選哪個?

候選:
- A. **輕量叠加** (Lightweight Overlay): 新方法 + 舊方法並存,標 deprecated
- B. 一次性重寫: 刪舊 `stream()`,只留 `complete_with_stream()`
- C. 完全獨立服務: 旁路 `streaming-v2` crate

**決議**: 選 A (用戶拍板 + 我推薦) — 理由:
- 風險最低: 舊路徑 0 變化,新路徑 opt-in
- 第三方 provider 不用動 (默認實現走 `complete()`)
- 1 release 週期內逐步切換,切完刪 deprecated

---

### Q3. 「結構體的命名不要變」是怎麼回事?

User 決策: **重寫的同時,結構體的命名不要變**。

含義: 新的 `complete_with_stream` 回調事件類型,不能引入 `SamplingDelta` 這個新 enum,
必須複用現有的 `StreamChunk`。

**推理**:
- `StreamChunk` 已有的變體: `Content` / `Usage` / `Stop` / `ToolCallStart` / `ToolCallDelta` / `ToolCallEnd`
- 唯一缺的是「流結束 + 攜帶完整 result」的信號 → 加一個 `IsDone { result: SamplingResult }` 變體即可
- `ToolCallDelta { arguments_delta }` 已經存在,只是 provider 從來不發,改 provider 即可
- `SamplingResult` 是 agent 已在用的最終類型,直接用

**淨增**:
- `StreamChunk::IsDone` 1 個變體
- `ModelFamily` / `SystemMessageForm` / `StreamError` 3 個小枚舉
- `TwoPartPrompt` / `TruncateConfig` 2 個 struct
- `complete_with_stream` 1 個 trait 方法

**淨刪老名字**: 0

---

### Q4. stream() 現狀有什麼問題?

User 追問: 「stream_completion 目前 provider 的實現有什麼問題和差異嗎?為啥要重寫?」

對 `synthia-provider` 進行代碼考古,發現 3 個真 bug + 4 處冗餘/死代碼 + 1 處協議錯位:

#### Bug 1: `streaming/openai.rs:151` `ends_with("")` 永遠 true

```rust
let ends_thinking = content.ends_with("");   // ← 永遠 true
```

舊 `openai_stream.rs:133` 是正確的 `content.ends_with("</think>")`。
重命名 / 複製時把 `</think>` 刪了。

**後果**: Qwen/DeepSeek thinking-tag 解析徹底壞掉。

#### Bug 2: 每個 tool-call delta 都發全量累積字符串

`streaming/anthropic.rs:158-167` 和 `streaming/openai.rs:88-108`:

```rust
chunks.push(StreamChunk::Content(ContentPart::ToolUse(ToolUse {
    input: serde_json::Value::String(buffer.input.clone()),  // 全量,不是 delta
})));
```

**後果**: O(n²) 數據傳輸,下游每收一個 chunk 都要重 parse。

#### Bug 3: `collect_stream_response` 信息黑洞

`streaming/mod.rs:66-83` 靜默 drop 5 個變體:
- `Content(Reasoning)` 丟
- `Content(ToolResult)` 丟
- `Content(Image/Audio/Resource)` 丟
- `Usage(_)` 丟
- `ToolCallStart/Delta/End` 丟

#### 4 處冗餘/死代碼

1. `stream.rs` (舊) 與 `streaming/anthropic.rs` (新) 內容 100% 相同
2. `openai_stream.rs` (舊) 與 `streaming/openai.rs` (新) 內容 100% 相同
3. `StreamChunk::ToolCallStart/Delta/End` 定義了但 provider 從不產生
4. `StepSample::execute` 匹配這 3 個變體的代碼走不到

#### 1 處協議錯位: 流所有權在 provider

`stream() -> StreamResult` (Box<Stream>) 的設計導致:
- agent 拿不到 abort 時機 (CancellationToken 傳了沒用)
- agent 拿不到 fallback 時機 (流斷了只能死)
- agent 拿不到增量 usage (只能在末尾)
- agent 拿不到增量 tool call (拿的是全量)

**結論**: 單獨修 bug 不夠,架構要改。opencode 走 AI SDK 風格的 `streamText` + 回調式 `onChunk` 模型,
本設計對齊到該模型,但**保留所有現有結構體命名**(用戶要求)。

---

### Q5. 為什麼重寫成 `complete_with_stream` 能一次解決所有問題?

| 原問題 | `complete_with_stream` 怎麼修 |
|--------|------------------------------|
| Bug 1 thinking 檢測 | `SamplingDelta::ReasoningDelta` 替代文本嗅探 (在本設計裡用 `StreamChunk::Content(ContentPart::Reasoning)`) |
| Bug 2 全量累積 | `ToolCallDelta { arguments_delta }` 只發增量 |
| Bug 3 丟信息 | `IsDone { result }` 直接給完整 `SamplingResult` |
| 流的所有權 | 回調式,agent 在自己 loop 裡消費 |
| Fallback | 默認實現走 `complete()`,provider 不寫一行 streaming 代碼也能用 |
| Cancel | 回調裡 check `cancel_token.is_cancelled()` |
| Usage 增量 | `StreamChunk::Usage(_)` 中間也能發 |
| 死代碼 1/2 | 舊 `StreamChunk::ToolCallStart/Delta/End` 真正連線,不再 dead |
| 冗餘 1/2 | 舊 `stream.rs` / `openai_stream.rs` 刪掉 |

---

### Q6. 設計 5 個待用戶確認的問題

1. **舊 `ModelProvider::stream()` deprecated 保留期**: 1 個 release (用戶選擇)
2. **`StreamError` 位置**: 在 `synthia-core` 統一 Error enum 中加 (用戶選擇)
3. **`TruncateConfig::max_bytes` 默認值**: 30K (用戶選擇,參考 opencode 默認 50K 但選保守)
4. **`TwoPartPrompt` header 2-3K tokens 估算**: 字符數 / 3.5 (用戶選擇,中庸覆蓋主流模型)
5. **M1-M6 6 個階段拆幾個 PR**: 3 個 PR (M1+M2 / M3+M4 / M5+M6) (用戶選擇)

---

## 設計取捨 (Trade-offs)

### T1. 回調式 vs Stream 異步迭代

**選**: 回調式 `FnMut(StreamChunk)`

**理由**:
- 流所有權在 agent,而不是 provider → abort/fallback 天然支持
- 與 opencode / AI SDK 對齊
- 第三方 provider 默認實現 = 走 `complete()`,零成本兼容

**代價**:
- 不能再用 `async_stream::try_stream!` 包裝,得自己寫回調
- 但我們本來就是 imperative agent loop,不是 stream consumer,影響小

### T2. `StreamChunk` 擴展 1 個變體 vs 新 enum `SamplingDelta`

**選**: `StreamChunk` +1 個 `IsDone` 變體 (用戶硬性要求)

**理由**:
- 滿足「結構體命名不要變」
- 已有 6 個變體,差一個收尾信號,加一個就行
- `ToolCallDelta` 已經存在,改 provider 連線即可

**代價**:
- `StreamChunk` 變體從 6 個變 7 個,語義有點雜 (Content + Usage + Stop + ToolCall* + IsDone 混在一起)
- 但 trait 名仍是 `StreamChunk`,語義不變 (「流的 chunk」)

### T3. Truncate 在 context crate 還是 tool_executor crate

**選**: 新增 `synthia-context::truncate`,保留 `tool_executor::truncate_result` 不動

**理由**:
- 新的 truncate 服務是「寫入 LLM 上下文前」的統一截斷,屬於 context 關注點
- 老的 `truncate_result` 是 tool_executor 內部細節,職責不同
- 字段擴展 (加 `output_path` / `marker`) 用 `#[serde(alias)]` 向後兼容

**代價**:
- 兩份 truncate 代碼並存 1 release 週期
- 但職責清晰,代價可接受

### T4. TwoPartPrompt 默認值估算用字符數/3.5

**選**: 字符數 / 3.5

**理由**:
- Sonnet 4 ~3.0, GPT-5 ~4.0,本地中等模型 ~3.5
- /3.5 是中庸,留 buffer
- 不引入 tiktoken-rs 依賴 (簡單,精準度犧牲 <10%)

**代價**:
- 對 Sonnet 4 略低估 (~15%),可能 header 實際 >3K tokens
- 但 2-3K 是設計目標範圍,不是硬上限,無實質影響

### T5. Memory 接入的方式

**選**: 不作為 loop 中一環,Memory 作為 tool 族使用 (用戶決定)

**理由**:
- 「Memory 自動 recall」太隱式,LLM 不知道何時 recall
- 「Memory 作為 tool」明確,LLM 主動調用 `memory_search` / `memory_save` / `memory_recall` / `memory_list` / `memory_forget`
- 與 user 之前要求的「ReAct 顯式」一致

**代價**:
- LLM 必須學會調 memory tool,prompt 中要明示
- 但顯式 > 隱式,長期更可控

---

## 最終決議 (LOCKED)

1. **首個改進**: Streaming + 2-part prompt + Truncate 服務
2. **集成策略**: 輕量叠加 (Lightweight Overlay)
3. **結構體命名**: 不引入新名,`StreamChunk` +1 變體
4. **stream() 保留期**: 1 release
5. **StreamError 位置**: `synthia-core` 統一 Error enum
6. **TruncateConfig 默認**: 30K bytes, head=100 lines, tail=100 lines
7. **header 估算**: 字符數 / 3.5
8. **PR 拆分**: 3 個 (M1+M2 / M3+M4 / M5+M6)

---

## Out of Scope (本次明確不做)

- 第三個改進 (Memory 接入) 改為 Memory 工具族 → 單獨 spec
- L4 Auto-compact (Stage 1/2/3 三階段壓縮) → 留 P2
- Session Reset (重建 session) → 留 P2
- Steering 中斷 (用戶中途改方向) → 留 P2
- Provider 第三方實現 (registry) → 默認實現兼容
- 刪 `truncate_result` 老 truncate → 留 release 週期後

---

## 引用 (References)

- 差距分析: `docs/superpowers/specs/2026-06-07-agent-gap-analysis.md`
- 設計: `docs/superpowers/specs/2026-06-07-streaming-2part-truncate-design.md`
- 規則: `.trae/rules/agent_rule.md` (P1-P10)
- 規則: `.trae/rules/rust.md` (fmt + clippy)
- 外部: opencode / AI SDK `streamText` 回調模型
- 外部: codex `stream` trait (回調式)
