## Context

synthia-agent 的 `StepSample::execute` 走 `ModelProvider::stream()` 路徑,該路徑在代碼考古中發現 3 個真 bug、4 處冗餘/死代碼、1 處協議錯位:

| 問題 | 位置 | 性質 |
|------|------|------|
| Bug 1 | `streaming/openai.rs:151` `ends_with("")` 永遠 true | 真 bug,thinking-tag 解析壞掉 |
| Bug 2 | provider 每 delta 發 `Content(ToolUse{input: 全量String})` | 真 bug,O(n²) 傳輸 |
| Bug 3 | `collect_stream_response` 靜默 drop 5 個變體 | 真 bug,信息黑洞 |
| 冗餘 1/2 | `stream.rs` 與 `streaming/anthropic.rs` 100% 相同 | 死代碼 |
| 死代碼 1/2 | `StreamChunk::ToolCallStart/Delta/End` 定義了但 provider 從不產生 | 死代碼 |
| 協議錯位 | 流所有權在 provider,agent 無 abort/fallback/增量 usage | 架構缺陷 |

單獨修 bug 不夠,架構本身要改。opencode 走 AI SDK 風格的統一 `streamText` + 回調式 `onChunk` 模型。本設計對齊到該模型,但**保留所有現有結構體命名**(用戶硬性要求)。

設計源頭是 `docs/superpowers/specs/2026-06-07-streaming-2part-truncate-design.md`(LOCKED 狀態)。本檔是 OpenSpec 流程中的 design 產物,負責把決策的「為什麼」結構化記錄。

---

## Goals / Non-Goals

**Goals:**

- ✅ 修復 3 個真 bug (Bug 1 thinking-tag / Bug 2 全量累積 / Bug 3 信息黑洞)
- ✅ 引入 `complete_with_stream` 回調式流接口,默認實現走 `complete()` (0 行為變化)
- ✅ 引入 `TwoPartPrompt` 2-part prompt 設計,優化 prefix cache 命中率
- ✅ 引入 `Truncate` 服務,head/tail 模式 + 落盤,大輸出可控
- ✅ 統一可觀測性: 9 個 metric + 1 個 context trace schema
- ✅ 統一可中斷性: CancellationToken 全鏈路傳遞
- ✅ 結構體命名 0 churn: 複用 `StreamChunk` / `SamplingResult` / `ToolUse` / `ContentPart` / `Usage`
- ✅ 5 個老 bug 修完,3 個冗餘文件刪除
- ✅ 12 輪 session benchmark `prefix_stability_ratio` ≥ 85%

**Non-Goals:**

- ❌ 第三個改進 (Memory 接入) 改為 Memory 工具族 → 單獨 spec
- ❌ L4 Auto-compact (Stage 1/2/3 三階段壓縮) → 留 P2
- ❌ Session Reset (重建 session) → 留 P2
- ❌ Steering 中斷 (用戶中途改方向) → 留 P2
- ❌ Provider 第三方實現 (registry) 強制重寫 → 默認實現走 `complete()` 兼容
- ❌ 刪 `truncate_result` 老 truncate → 留 release 週期後
- ❌ 引入 `SamplingDelta` 新 enum → 複用 `StreamChunk`

---

## Decisions

### D1: 集成策略 = 輕量叠加 (Lightweight Overlay)

- **選擇**: 新增 `complete_with_stream` 方法,默認實現走 `complete()`;舊 `ModelProvider::stream()` 標 `#[deprecated]`,保留 1 release 週期
- **理由**:
  - 風險最低: 0 行為變化
  - 第三方 provider 不用動 (默認實現兼容)
  - 1 release 週期內逐步切換
- **已考慮 alternative**:
  - B. 一次性重寫 → 風險高,第三方 provider 全 break
  - C. 完全獨立服務 → 增加複雜度,得不償失

### D2: 流式 API 風格 = 回調式 (Callback) 而非 Stream 異步迭代

- **選擇**: `complete_with_stream(req, on_delta: FnMut(StreamChunk))`,on_delta 是閉包回調
- **理由**:
  - 流所有權在 agent loop,而不是 provider → abort/fallback 天然支持
  - 與 opencode / AI SDK 對齊
  - 第三方 provider 不寫 streaming 代碼也能用 (默認實現)
- **已考慮 alternative**:
  - A. 保留 `stream() -> Box<Stream>` → 問題依舊 (流所有權錯位)
  - B. 雙 API 並存 → 維護成本高

### D3: 結構體命名 = 不引入新 enum,`StreamChunk` +1 變體

- **選擇**: `StreamChunk` 加 1 個 `IsDone { result: SamplingResult }` 變體,其他全保留
- **理由**:
  - 用戶硬性要求「結構體命名不要變」
  - 已有 6 個變體,差一個收尾信號即可
  - `ToolCallDelta` 已經存在,改 provider 連線即可
  - 0 老 struct / enum 被刪
- **已考慮 alternative**:
  - A. 新增 `SamplingDelta` enum → 違反用戶要求
  - B. 完全重寫 `StreamChunk` → 破壞向後兼容

### D4: Truncate 服務放在 synthia-context 而非 tool_executor

- **選擇**: 新增 `synthia-context::truncate`,保留 `tool_executor::truncate_result` 不動
- **理由**:
  - 新 truncate 是「寫入 LLM 上下文前」的統一截斷,屬於 context 關注點
  - 老的 `truncate_result` 是 tool_executor 內部細節,職責不同
  - 字段擴展用 `#[serde(alias)]` 向後兼容
- **已考慮 alternative**:
  - A. 直接擴展 `truncate_result` → 跨 crate 職責不清
  - B. 合併到 `tool_executor` → 概念錯誤,context 不屬於 tool_executor

### D5: TwoPartPrompt 的 token 估算 = 字符數 / 3.5

- **選擇**: header 長度估算按字符數 / 3.5
- **理由**:
  - Sonnet 4 ~3.0, GPT-5 ~4.0,本地中等模型 ~3.5
  - /3.5 是中庸,留 buffer
  - 不引入 tiktoken-rs 編譯期依賴
- **已考慮 alternative**:
  - A. 按 Sonnet 4 比例 (/3.0) → 偏小,可能 header > 3K
  - B. 用 tiktoken-rs 實時算 → 精準但增加依賴
  - C. 寬鬆估算 (/2.5) → 浪費 cache 空間

### D6: StreamError 在 synthia-core 統一 Error enum 中加

- **選擇**: 在 `synthia-core` 統一 `Error` enum 中新增 `StreamError` variant
- **理由**:
  - 用戶決策:統一管理
  - 跨 crate 共享,符合 synthia-core 已有的職責
- **已考慮 alternative**:
  - A. synthia-provider 獨立定義 → 與 synthia-core 重疊
  - B. 完全獨立 Error crate → 增加複雜度

### D7: PR 拆分 = 3 個 PR

- **選擇**: PR1 = M1+M2, PR2 = M3+M4, PR3 = M5+M6
- **理由**:
  - M1+M2: 新增 `complete_with_stream` + Anthropic 真流式 → 改完 Anthropic 用戶馬上受益
  - M3+M4: OpenAI 真流式 + StepSample 切換 → 改完 OpenAI 用戶受益
  - M5+M6: 清死代碼 + 刪 deprecated → 收尾
  - 3 個 PR 評審成本可接受
- **已考慮 alternative**:
  - A. 1 個大 PR → 評審難
  - B. 6 個小 PR → 集成晚,評審次數多
  - C. 5 個 PR → 不對稱,沒必要

---

## Risks / Trade-offs

[Risk] 現有調用方未切到 `complete_with_stream`,deprecated 警告被忽略
→ Mitigation: 1 release 週期後刪除,期間持續監控 grep `provider.stream()`

[Risk] 回調式流被嵌入到已有的 `async_stream::try_stream!` 不兼容
→ Mitigation: 默認實現走 `complete()`,不強制 provider 重寫;只在需要流式的 provider 寫自定義 `complete_with_stream`

[Risk] `mpsc::channel(64)` 背壓丟棄關鍵 chunk (LLM 主訊息)
→ Mitigation: `IsDone` 攜帶完整 `SamplingResult`,丟了中間 delta 也能恢復(若 `IsDone` 本身丟,fallback 走 `complete()` 同步重試)

[Risk] Provider 第三方實現 (registry) 不重寫 `complete_with_stream`
→ Mitigation: 默認實現走 `complete()`,等同於無流式;主動聯繫 owner 在 1 release 週期內升級

[Risk] 取消時 HTTP 連接不釋放
→ Mitigation: provider task 內 `select!` on `cancel_token`,5s 內 cancel HTTP connection

[Risk] Bug 1 修復後,Qwen/DeepSeek 行為變化導致已有 session 結果不同
→ Mitigation: 默認 `SystemMessageForm::Single` 保留舊行為,`TwoPart` 是 opt-in

[Trade-off] `StreamChunk` 變體從 6 個變 7 個,語義略雜
→ 接受理由: 滿足用戶「命名不要變」硬性要求;變體增加 < 20%,可控

[Trade-off] 兩份 truncate 代碼並存 1 release 週期
→ 接受理由: 職責清晰(老 = tool_executor 內部 / 新 = context 統一服務);字段兼容

[Trade-off] 6 個 stage 拆 3 個 PR,M1-M4 之間可能短期出現雙套接口
→ 接受理由: 雙套接口都是穩定的(default + 真流式),無 breaking change

---

## Migration Plan

### PR1 (M1 + M2): 基礎能力 + Anthropic 真流式

**Step 1.1**: 在 `synthia-provider/src/types.rs` 給 `StreamChunk` 加 `IsDone` 變體 (0 老代碼改動)
**Step 1.2**: 在 `synthia-provider/src/traits.rs` 給 `ModelProvider` 加 `complete_with_stream` 默認實現,標 `stream()` deprecated
**Step 1.3**: 在 `synthia-context/src/truncate.rs` 實作 `TruncateConfig` / `TruncatedResult` / `truncate_output`
**Step 1.4**: 在 `synthia-context/src/prompt/two_part.rs` 實作 `TwoPartPrompt` / `ModelFamily` / `SystemMessageForm`
**Step 1.5**: 修 `streaming/openai.rs:151` 的 `ends_with("")` Bug 1 (提前合入,避免 PR1 還帶 bug)
**Step 1.6**: 在 `synthia-core` 的 `Error` enum 中加 `StreamError` variant
**Step 1.7**: 在 `synthia-provider/src/anthropic.rs` 重寫 `complete_with_stream` (真流式,SSE + 回調)
**Step 1.8**: 測試: `truncate_test.rs` / `two_part_test.rs` / `streaming_anthropic_test.rs`

**驗收**:
- `cargo test --workspace` 全綠
- `cargo clippy --all-targets --all-features --tests --all` 0 warning
- Anthropic 用戶能體驗到首 token < 500ms

**回滾**: 直接 revert PR (舊代碼未改)

---

### PR2 (M3 + M4): OpenAI 真流式 + agent 切換

**Step 2.1**: 在 `synthia-provider/src/openai.rs` 重寫 `complete_with_stream` (真流式,修 Bug 2 全量累積,改用 `ToolCallDelta { arguments_delta }`)
**Step 2.2**: 重寫 `streaming/openai.rs` 的 `OpenAIStreamProcessor`,走 `reasoning_content` 字段不再嗅探 `<think>`
**Step 2.3**: 在 `synthia-agent/src/stream_builder/steps/sample.rs` 把 `StepSample::execute` 從 `provider.stream()` 切到 `provider.complete_with_stream()`,用 mpsc channel + cancel token
**Step 2.4**: 測試: `streaming_openai_test.rs` (mock SSE, 覆蓋 Bug 1 修復 + Bug 2 修復) / e2e streaming 測試 / e2e fallback 測試

**驗收**:
- `cargo test --workspace` 全綠
- OpenAI 用戶能體驗到首 token < 500ms
- 取消 < 1s 生效
- 流意外關閉 fallback 走 `complete()` 成功

**回滾**: 改回 `provider.stream()` 一行 diff

---

### PR3 (M5 + M6): 清理 + 刪 deprecated

**Step 3.1**: 刪 `crates/synthia-provider/src/stream.rs` (與 `streaming/anthropic.rs` 重複)
**Step 3.2**: 刪 `crates/synthia-provider/src/openai_stream.rs` (與 `streaming/openai.rs` 重複)
**Step 3.3**: 標 `streaming/mod.rs::collect_stream_response` 為 `#[deprecated]`,改 3 個調用點(`agent/core.rs`, `agent/model_call.rs`)
**Step 3.4**: 刪 `streaming/openai.rs` 整個文件 (新版 OpenAIStreamProcessor 在 PR2 已合入)
**Step 3.5**: 1 release 週期後: 刪 `ModelProvider::stream()` 默認方法,刪 `collect_stream_response` 函數

**驗收**:
- `cargo test --workspace` 全綠
- `cargo clippy` 0 warning
- 1 release 後 `grep -r "provider.stream(" --include="*.rs"` = 0 結果

**回滾**: 從 git 歷史恢復 `stream.rs` / `openai_stream.rs`

---

## Open Questions

無 (5 個待確認問題已在 Q6 全部 LOCKED,見 `brainstorm.md` 決策鏈)。
