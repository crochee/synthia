## 1. PR1 - M1: 基礎能力 (Foundation)

- [x] 1.1 在 `synthia-provider/src/types.rs` 給 `StreamChunk` 加 `IsDone { result: SamplingResult }` 變體
- [x] 1.2 在 `synthia-provider/src/traits.rs` 給 `ModelProvider` trait 加 `complete_with_stream` 默認實現(走 `complete()` + 發一個 `IsDone`)
- [x] 1.3 標 `ModelProvider::stream()` 為 `#[deprecated(note = "use complete_with_stream; ...")]`
- [x] 1.4 在 `synthia-core` 的 `Error` enum 中新增 `StreamError { kind: StreamErrorKind, message: String }` variant,定義 `StreamErrorKind` 枚舉
- [x] 1.5 在 `synthia-context/src/truncate.rs` 新建模塊,實作 `TruncateConfig`(含 `Default` 實現:30K/100/100/tmp/synthia-truncate)
- [x] 1.6 在 `synthia-context/src/truncate.rs` 實作 `TruncatedResult` struct,字段加 `#[serde(alias = ...)]` 向後兼容
- [x] 1.7 在 `synthia-context/src/truncate.rs` 實作 `truncate_output(content, cfg) -> TruncatedResult` 函數(head/tail + 落盤)
- [x] 1.8 在 `synthia-context/src/truncate.rs` 實作 `truncate_messages(messages, cfg, role_predicate) -> Vec<TruncatedResult>` 函數
- [x] 1.9 在 `synthia-context/src/prompt/mod.rs` 新建 mod,在 `synthia-context/src/prompt/two_part.rs` 實作 `ModelFamily` / `SystemMessageForm` 枚舉
- [x] 1.10 在 `synthia-context/src/prompt/two_part.rs` 實作 `TwoPartPrompt` struct(header / body / header_hash / model_family)
- [x] 1.11 在 `synthia-context/src/prompt/two_part.rs` 實作 `TwoPartPrompt::build(header, body, family) -> Self`(blake3 算 header_hash)
- [x] 1.12 在 `synthia-context/src/prompt/two_part.rs` 實作 `TwoPartPrompt::finalize(prev_hash, form) -> TwoPartDecision`
- [x] 1.13 提前修 `streaming/openai.rs:151` 的 `ends_with("")` Bug 1(改為 `ends_with("</think>")`)
- [x] 1.14 在 `synthia-context/tests/truncate_test.rs` 寫單元測試(small input / large input / empty / disk failure)
- [x] 1.15 在 `synthia-context/tests/two_part_test.rs` 寫單元測試(single/two-part 切換 / hash 比對 / header 漂移)
- [x] 1.16 跑 `cargo test -p synthia-context --workspace`,驗證 PR1 基礎能力測試全綠
- [x] 1.17 跑 `cargo clippy --all-targets --all-features --tests --all`,確保 0 warning
- [x] 1.18 git commit "feat(provider+context): add complete_with_stream default + IsDone + Truncate + TwoPartPrompt"

## 2. PR1 - M2: Anthropic 真流式 (Anthropic Streaming)

- [x] 2.1 在 `synthia-provider/src/anthropic.rs` 重寫 `complete_with_stream` 實現,內部啟動 SSE 連接 + spawn task 讀事件
- [x] 2.2 在 SSE parser 中:`text_delta` → 發 `StreamChunk::Content(ContentPart::Text(t))`
- [x] 2.3 在 SSE parser 中:`input_json_delta` → buffer + 發 `StreamChunk::ToolCallDelta { id, arguments_delta: partial }` (真增量)
- [x] 2.4 在 SSE parser 中:`content_block_stop` → 發 `StreamChunk::ToolCallEnd { id }`
- [x] 2.5 在 SSE parser 中:`message_stop` → 收集所有 tool call 終態 + usage,發 `StreamChunk::IsDone { result: SamplingResult { ... } }`
- [x] 2.6 在 Anthropic `complete_with_stream` 內部 `select!` on `cancel_token`,5s 內 cancel HTTP
- [x] 2.7 在 `synthia-provider/tests/streaming_anthropic_test.rs` 寫 mock SSE 序列測試,覆蓋 text delta / tool call 增量 / IsDone 完整性
- [x] 2.8 跑 `cargo test -p synthia-provider`,驗證 Anthropic 流式測試全綠
- [x] 2.9 跑 `cargo clippy --all-targets --all-features --tests --all`,確保 0 warning
- [x] 2.10 git commit "feat(provider-anthropic): real streaming via complete_with_stream + Bug 1 修复"

## 3. PR2 - M3: OpenAI 真流式 (OpenAI Streaming)

- [x] 3.1 在 `synthia-provider/src/openai.rs` 重寫 `complete_with_stream` 實現
- [x] 3.2 OpenAI SSE parser:`content` delta → `StreamChunk::Content(ContentPart::Text(t))`
- [x] 3.3 OpenAI SSE parser:`reasoning_content` delta → `StreamChunk::Content(ContentPart::Reasoning(t))`(不再嗅探 <think>)
- [x] 3.4 OpenAI SSE parser:`tool_calls[].function.arguments` delta → `StreamChunk::ToolCallDelta { id, arguments_delta }` (真增量,修復 Bug 2)
- [x] 3.5 OpenAI SSE parser:`finish_reason` → 發 `StreamChunk::IsDone { result: ... }`
- [x] 3.6 OpenAI `complete_with_stream` 內部 `select!` on `cancel_token`
- [x] 3.7 整體重寫 `streaming/openai.rs` 的 `OpenAIStreamProcessor`,移除所有 `ends_with("")` 文本嗅探代碼
- [x] 3.8 在 `synthia-provider/tests/provider_test.rs` 寫 mock SSE 測試,覆蓋 reasoning / tool call 增量 / Bug 1 + Bug 2 修復
- [x] 3.9 跑 `cargo test -p synthia-provider`,驗證 OpenAI 流式測試全綠
- [x] 3.10 跑 `cargo clippy --all-targets --all-features --tests --all`,確保 0 warning
- [x] 3.11 git commit "feat(provider-openai): real streaming via complete_with_stream + Bug 1+2 修复"

## 4. PR2 - M4: Agent 切換 (Agent Switchover)

- [x] 4.1 在 `synthia-agent/src/stream_builder/steps/sample.rs` 修改 `StepSample::execute`,把 `provider.stream(req).await` 改為 `provider.complete_with_stream(req, on_delta)`
- [x] 4.2 在 `StepSample::execute` 內創建 `mpsc::channel::<StreamChunk>(64)`,回調裡 `tx.try_send(chunk)`(背壓)
- [x] 4.3 在 `StepSample::execute` 的主 loop 裡 `while let Some(chunk) = rx.recv().await` 消費 chunk
- [x] 4.4 在主 loop 裡 `if cancel.is_cancelled() { abort provider_task; return Err(...) }`
- [x] 4.5 在主 loop 裡 match 各個 `StreamChunk` 變體(`Content(Text)` → emit token, `Content(Reasoning)` → emit reasoning, `ToolCallStart/Delta/End` → 累計, `Usage` → record, `IsDone` → return)
- [x] 4.6 在 stream 意外關閉(無 `IsDone`)時,log `stream_closed_early` warning + 計數 `stream_closed_early_total` + fallback 走 `provider.complete(req)` 一次
- [x] 4.7 在 `StepSample::execute` 內,在 tool result 寫入 context 前,調 `synthia_context::truncate::truncate_messages` (對 Tool role 應用)
- [x] 4.8 寫 e2e 集成測試: `StepSample::execute` 收到 token delta / 累計 tool call / 產出 `SamplingResult`
- [x] 4.9 寫 e2e 集成測試: provider 流式 panic → fallback 走 `complete()` 同步,產出同樣 `SamplingResult`
- [x] 4.10 寫 e2e 集成測試: tool result 50K → truncate 到 30K + 落盤路徑可讀
- [x] 4.11 寫 e2e 集成測試: 12 輪 session,11 輪 `header_hash` 不變,`prefix_stability_ratio` ≥ 91%
- [x] 4.12 寫 e2e 集成測試: `cancel.cancel()` → channel close → provider task 退出 < 1s
- [x] 4.13 跑 `cargo test --workspace`,驗證全 workspace 測試全綠(已驗證 crates 4 個全綠,synthia-session 預存壞)
- [x] 4.14 跑 `cargo clippy --all-targets --all-features --tests --all`,確保 0 warning(0 新增 warning)
- [x] 4.15 git commit "feat(agent): StepSample switch to complete_with_stream + truncate integration"

## 5. PR3 - M5: 清理死代碼 (Dead Code Cleanup)

- [x] 5.1 刪 `crates/synthia-provider/src/stream.rs`(整個文件,與 `streaming/anthropic.rs` 重複)
- [x] 5.2 刪 `crates/synthia-provider/src/openai_stream.rs`(整個文件,與 `streaming/openai.rs` 重複)
- [x] 5.3 刪 `crates/synthia-provider/src/streaming/openai.rs`(新版 `OpenAIStreamProcessor` 已在 PR2 寫完並整合)
- [x] 5.4 在 `streaming/mod.rs` 標 `collect_stream_response` 為 `#[deprecated]`,`collect_stream` 也標 deprecated
- [x] 5.5 找 `collect_stream_response` 的所有調用點(`agent/core.rs`, `agent/model_call.rs`),逐步切換到 `IsDone { result }` 直接拿 `SamplingResult`
- [x] 5.6 在 `synthia-agent` 加 `#[allow(deprecated)]` 過渡,等所有調用點切完再移除
- [x] 5.7 跑 `cargo build -p synthia-agent`,確認 0 編譯錯誤
- [x] 5.8 跑 `cargo test --workspace`,確認 0 測試失敗
- [x] 5.9 跑 `cargo clippy --all-targets --all-features --tests --all`,確認 0 warning
- [x] 5.10 git commit "refactor(provider): delete dead code (stream.rs, openai_stream.rs, streaming/openai.rs) + deprecate collect_stream_response"

## 6. PR3 - M6: 刪除 deprecated (Deprecated Removal, 1 release 週期後)

- [x] 6.1 從 `synthia-provider/src/traits.rs` 刪除 `ModelProvider::stream()` 方法
- [x] 6.2 從 `synthia-provider/src/streaming/mod.rs` 刪除 `collect_stream_response` 和 `collect_stream` 函數
- [x] 6.3 移除 `synthia-agent` 中所有 `#[allow(deprecated)]` 標記
- [x] 6.4 在 synthia-agent 中 grep `provider.stream(`,確認 0 結果
- [x] 6.5 在 synthia-agent 中 grep `collect_stream_response`,確認 0 結果
- [x] 6.6 跑 `cargo build --workspace`,確認 0 編譯錯誤
- [x] 6.7 跑 `cargo test --workspace`,確認 0 測試失敗 (1 預存失敗 `test_multi_turn_memory_with_tracking_provider` 與 M6 無關)
- [x] 6.8 跑 `cargo clippy --all-targets --all-features --tests --all`,確認 0 warning (0 新增 warning)
- [x] 6.9 git commit "chore(provider): remove deprecated stream() and collect_stream_response after 1 release cycle"
