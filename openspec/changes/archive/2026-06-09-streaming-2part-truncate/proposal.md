## Why

`synthia-provider` 的 `ModelProvider::stream()` 路徑在代碼考古中發現 3 個真 bug (`ends_with("")` thinking-tag 嗅探壞掉 / O(n²) 全量累積 / `collect_stream_response` 信息黑洞)、4 處冗餘/死代碼、和 1 處架構錯位 (流所有權在 provider,agent 無 abort/fallback/增量 usage)。這些 bug 已在生產環境影響 Qwen/DeepSeek reasoning 解析和大輸出 tool result 的 prefix cache。同時 synthia 缺乏 2-part prompt 設計 (prefix cache 命中率低) 和統一的 Truncate 服務 (tool result 大小不可控)。opencode / AI SDK 已經走回調式 `streamText` 模型解決了同類問題。本次改進對齊該模型,**保留所有現有結構體命名**,通過輕量叠加 (新增 `complete_with_stream`,舊 `stream()` 標 deprecated 保留 1 release 週期) 一次性修復 3 個 bug + 引入 2 個新能力 + 清理 3 處死代碼。預期收益:`prefix_stability_ratio` ≥ 85%,`stream_first_token_latency_ms` P50 < 500ms,Qwen/DeepSeek reasoning 正確解析。

## What Changes

**ModelProvider 流式接口**
- From: `stream() -> Box<Stream<Result<StreamChunk, Error>>>` (流所有權在 provider)
- To: `complete_with_stream(req, FnMut(StreamChunk))` (回調式,流所有權在 agent)
- Reason: 修復 3 個真 bug + 架構錯位;abort/fallback/增量 usage 天然支持
- Impact: 非破壞性。`stream()` 標 `#[deprecated]` 保留 1 release 週期;默認實現走 `complete()` 兼容第三方 provider

**StreamChunk 變體擴展**
- From: 6 個變體 (Content/Usage/Stop/ToolCallStart/ToolCallDelta/ToolCallEnd)
- To: +1 個 `IsDone { result: SamplingResult }` 變體 (其他全保留)
- Reason: 「結構體命名不要變」(用戶硬性要求),差一個收尾信號
- Impact: 非破壞性,新加變體

**ToolCallDelta 真正連線**
- From: provider 從不發 `ToolCallDelta`,每 delta 發全量 `Content(ToolUse{input: String})`
- To: provider 發 `ToolCallDelta { arguments_delta: partial }` 真增量
- Reason: 修復 Bug 2 O(n²) 傳輸
- Impact: 行為變化,輸出含義更精準 (增量 vs 全量)

**新增 Truncate 服務**
- From: 只有 `tool_executor::truncate_result` (內部細節)
- To: 新增 `synthia_context::truncate::truncate_output()` (head/tail 模式 + 落盤)
- Reason: 統一 LLM 上下文寫入前的截斷,大輸出可控
- Impact: 新增,舊 truncate 保留 1 release 週期

**新增 TwoPartPrompt 設計**
- From: 單 system message (prefix cache 命中率低)
- To: `TwoPartPrompt { header (2-3K stable), body (variable) }` + `SystemMessageForm::Single | TwoPart` 切換
- Reason: 優化 prefix cache 命中率 (Anthropic / OpenAI 都支持多 system message)
- Impact: opt-in,`SystemMessageForm::Single` 保留舊行為

**StreamError 統一**
- From: 無統一流錯誤
- To: `synthia_core::Error` enum 新增 `StreamError` variant
- Reason: 跨 crate 統一錯誤管理
- Impact: 純新增

**刪除死代碼**
- 刪 `crates/synthia-provider/src/stream.rs` (與 `streaming/anthropic.rs` 重複)
- 刪 `crates/synthia-provider/src/openai_stream.rs` (與 `streaming/openai.rs` 重複)
- 標 `streaming/mod.rs::collect_stream_response` 為 `#[deprecated]`,1 release 後刪
- 1 release 後刪 `ModelProvider::stream()`

## Capabilities

### New Capabilities
- `model-provider-streaming`: 引入 `ModelProvider::complete_with_stream` 回調式流接口,默認 fallback 到 `complete()`;`StreamChunk` 增 `IsDone` 變體;`ToolCallDelta` 真正連線增量
- `tool-output-truncation`: 引入 `synthia_context::truncate::truncate_output()`,head/tail 模式 + temp_dir 落盤,默認 30K bytes
- `two-part-prompt`: 引入 `TwoPartPrompt` 容器,`header` 字節級穩定 + `body` 可變,`header_hash` 追蹤 prefix 穩定性,`SystemMessageForm` 切換單/雙 system message

### Modified Capabilities
- `prefix-tracker-wiring`: `TwoPartPrompt::header_hash` 替補/擴展 prefix 追蹤邏輯,emit `prefix_stability_ratio` 指標
- `stream-builder-v2`: `StepSample::execute` 從 `provider.stream()` 切到 `provider.complete_with_stream()`,引入 mpsc 背壓 + CancellationToken 全鏈路

## Impact

**Affected crates**:
- `synthia-core`: Error enum +1 variant
- `synthia-context`: 新增 `truncate` 和 `prompt/two_part` 模塊
- `synthia-provider`: trait +1 method,types +1 variant,anthropic.rs / openai.rs 重寫 `complete_with_stream`,刪 2 個死代碼文件,修 1 個真 bug
- `synthia-agent`: `stream_builder/steps/sample.rs` 切換調用點,引入 mpsc + cancel token

**Affected APIs**:
- `ModelProvider::stream()`: 標 `#[deprecated]`,1 release 後刪
- `streaming::collect_stream_response`: 標 `#[deprecated]`,1 release 後刪
- 新增 `ModelProvider::complete_with_stream`
- 新增 `StreamChunk::IsDone`

**Affected tests**:
- 新增 `synthia-context/tests/truncate_test.rs`
- 新增 `synthia-context/tests/two_part_test.rs`
- 新增 `synthia-provider/tests/streaming_anthropic_test.rs`
- 新增 `synthia-provider/tests/streaming_openai_test.rs`

**No breaking changes** for end users (model behavior preserved; only internal streaming API evolves).
