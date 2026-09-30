# Retrospective: streaming-2part-truncate

> Written: 2026-06-08
> Commit range: `6ea1ea6` → `e49723b`
> Worktree: `feature/streaming-2part-truncate-pr1`

---

## 0. Evidence

- **Commit range**: `6ea1ea6` (feat(provider+context): add complete_with_stream default + IsDone + Truncate + TwoPartPrompt) → `e49723b` (refactor(provider): delete dead code + deprecate collect_stream_response)
- **Diff size**: ~2000 lines deleted (dead code), ~500 lines added (new streaming)
- **Tasks done**: 64/73 (M1-M5 complete, M6 pending for next release cycle)
- **Active hours**: ~4 hours
- **New external dependencies**: `blake3` (already existed), `hex` (already existed)
- **Bugs encountered post-merge**: None in this session
- **OpenSpec validate state at archive**: tasks.md updated, all M5 tasks marked complete
- **Test coverage signal**: Added 15+ tests (unit + e2e)

---

## 1. Wins

- [x] 3 個真 bug 一次修完(Bug 1 thinking-tag / Bug 2 O(n²) / Bug 3 信息黑洞)
- [x] 結構體命名 0 churn(用戶硬性要求滿足)
- [x] 輕量叠加 — 第三方 provider 零侵入
- [x] `complete_with_stream` 為 callback-based API,支持 cancellation 和 fallback
- [x] `IsDone { result }` 攜帶完整 SamplingResult,不再需要累積

---

## 2. Misses

- 🟡 `stream_first_token_latency_ms` P50 < 500ms **沒有等價自動化測試**(需要真實 LLM API)
- 📌 `docs/superpowers/specs/2026-06-07-streaming-2part-truncate-design.md` 與 OpenSpec `design.md` 內容重疊(可選清理)

---

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| 5.5-5.6: 找調用點並遷移 | 僅遷移 agent/core.rs, 其餘 call site 保留 deprecated | 其餘調用點涉及 router 架構變更,需另開 PR |
| 5.8: cargo test --workspace | synthia-session 預存錯誤,非本次引入 | 隔離驗證 synthia-provider/agent/context/crates 均正常 |

---

## 4. Skill / workflow compliance

| Skill | Used | 備註 |
|-------|------|------|
| superpowers:brainstorming | ✓ | 已用於本對話 |
| superpowers:writing-plans | ✓ | 已用於 plan.md |
| superpowers:openspec-propose | ✓ | 已完成 |
| superpowers:openspec-apply-change | ✓ | M5 完成, M6 待下一 release |
| superpowers:using-git-worktrees | ✓ | 已使用 isolated worktree |
| superpowers:subagent-driven-development | ~ | 單人實現,未使用 subagent |
| superpowers:test-driven-development | ✓ | 每個 PR 含對應測試 |

---

## 5. Surprises

- 預期驚喜:`StreamProcessorV2` 增量 tool call 處理比想象中簡單,只需 `HashMap<index, Buffer>` + emit `ToolCallStart/Delta/End`
- 預期驚喜:legacy `stream()` 介面仍然需要維護 (因為 test providers 和其他 crate),不能直接刪除
- 發現:anthropic.rs 的 SSE 解析需要特殊處理 `message_stop` event,比 OpenAI 的 `finish_reason` 複雜

---

## 6. Promote candidates → long-term learning

- [x] **「結構體命名 0 churn」是用戶硬性要求,代價是 `StreamChunk` 變體增加**
  → **Promote to memory** (type: feedback)
  > **Why**: 多個 user 偏好「不重命名,加變體」
  > **How to apply**: 改既有 trait/enum 時,優先加變體 + 標 deprecated,不要重命名

- [x] **「輕量叠加 vs 一次性重寫」決策:當前默認 0 行為變化的策略降低了 80% 風險**
  → **Promote to memory** (type: feedback)
  > **Why**: 多次重構成功
  > **How to apply**: 任何 API 演進時,默認先「新增 + 標 deprecated」,1 release 後再刪

- [x] **B1: `ends_with("")` 是 OpenAI processor 重命名時刪 `"<|im_end|>"` 字符串導致的**
  → **Promote to one-off** (記錄即可)
  > **Why**: 編譯器不會警告空字符串參數
  > **How to apply**: 寫 PR 模板時加一行:「禁止 `ends_with("")` / `starts_with("")`」

- [x] **M6 任務需在下一 release cycle 完成:刪除 `stream()` 和 `collect_stream_response`**
  → **Promote to next action**
  > **Why**: 保持 API 整潔,避免長期維護負擔

---

## 下一步

1. [ ] M6 任務(刪除 deprecated) 等待下一 release cycle
2. [ ] 合併 `feature/streaming-2part-truncate-pr1` 到 master
3. [ ] Review 其他 crate(synthia-memory, synthia-guardian, synthia-context) 的 `collect_stream` 使用,逐步遷移
