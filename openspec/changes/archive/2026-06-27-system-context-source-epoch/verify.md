# Verification Report

> 此檔案由 `openspec-verify-change` skill 在 apply 完成後產生，用以確認實作
> 與 specs / design / tasks 的一致性。失敗的檢查須返回對應 artifact 修正後
> 再重跑 verify。

**Change**: `system-context-source-epoch`
**Verified at**: `2026-06-26 22:30`
**Verifier**: `apply-change agent (GLM-5.2)`

---

## 1. Structural Validation (`openspec validate --all --json`)

- [x] 全數 items `\"valid\": true`（change 自身 + 與本 change 相關的 main specs）

**結果**：

```text
Total: 89, Passed: 87, Failed: 2
  FAIL: subagent-listing (spec) - pre-existing, missing ## Purpose (unrelated to this change)
  FAIL: v2-session-api (spec) - pre-existing, missing ## Purpose (unrelated to this change)
```

**Change 自身驗證**：`system-context-source-epoch` (change) → `valid: true`

**本次修復**：`cache-policy-injection` main spec 原本缺少 `## Purpose` 段（pre-existing），因本 change 正在修改該 capability，已外科式補上 `# cache-policy-injection Specification` 標題與 `## Purpose` 段，現在 valid。

**剩餘 2 個失敗為 pre-existing 且與本 change 無關**（`subagent-listing`、`v2-session-api` 屬於其他 capability），依 CLAUDE.md「Surgical Changes」原則不在本 change 範圍內修復，記錄為 follow-up。

| Item | Type | Issues |
|---|---|---|
| subagent-listing | spec | pre-existing: missing `## Purpose` (unrelated) |
| v2-session-api | spec | pre-existing: missing `## Purpose` (unrelated) |

---

## 2. Task Completion (`tasks.md`)

- [x] 所有 `- [ ]` 已變為 `- [x]`

**未完成任務**：無（66/66 全數完成）

| Task | 未完成原因 | 是否阻塞 archive |
|---|---|---|
| — | — | — |

---

## 3. Delta Spec Sync State

對每個 `openspec/changes/system-context-source-epoch/specs/` 下的 capability 目錄，與
`openspec/specs/<capability>/spec.md` 比對：

| Capability | Sync 狀態 | 備註 |
|---|---|---|
| cache-control-mark | ✗ 待 sync | MODIFIED requirements；main spec 存在，archive 時合併 |
| cache-policy-injection | ✗ 待 sync | ADDED requirements；main spec 存在（已補 Purpose），archive 時合併 |
| prefix-source-trait | ✗ 待 sync | NEW capability；main spec 不存在，archive 時創建 |
| system-context | ✗ 待 sync | REMOVED requirements；main spec 存在，archive 時移除 3 條 git 相關 requirement |

> 全部 4 個 delta spec 均待 sync，這是 archive 前的預期狀態。`openspec archive -y` 會自動完成 sync。

---

## 4. Design / Specs Coherence Spot Check

抽樣比對 `design.md` 的決策是否反映在 `specs/*.md` 的 Requirements 與 Scenarios 中：

| 抽樣項 | design 描述 | specs 對應 | 差距 |
|---|---|---|---|
| D1 寬方案範圍 | 端到端修復 cache prefix 一致性鏈 | 4 個 spec 覆蓋 mark/injection/source-trait/system-context | 無 |
| D2 刪除 SystemContext | 刪除死代碼而非復活 | system-context spec REMOVED 3 條 git requirement | 無 |
| D3 統一 CacheControlMark | unify 到獨立 crate 而非 bridge | cache-control-mark spec MODIFIED（hash 獨立性、scope 命名空間） | 無 |
| D4 applyCachePolicy 接入 | assembler 層無條件注入 + provider 層守衛 | cache-policy-injection spec ADDED 4 條生產路徑注入 requirement | 無 |
| D5 Source trait | opencode baseline/update/removed 生命週期 | prefix-source-trait spec ADDED 7 條 requirement（Source/SourceDelta/SourceEpoch/3 個 Source 實作/CacheBreakDetector 重寫） | 無 |
| D6 確定性 hash | 全量替換 DefaultHasher → ahash | cache-control-mark spec MODIFIED（hash 獨立於 system content） | 無 |
| D7 CacheBreakDetector 重寫 | 隨 D5 SourceEpoch 自動修復 | prefix-source-trait spec（CacheBreakDetector SHALL use SourceEpoch HashMap） | 無 |

**漂移警告**（非阻塞）：無

---

## 5. Implementation Signal

- [ ] Worktree 內無未 staged 的檔案
- [ ] 所有相關 commit 已推送

**狀態**：Worktree `/home/crochee/workspace/synthia/.worktrees/system-context-source-epoch` 內有 22 個未 staged 檔案（2 個新增目錄 `crates/synthia-cache-mark/`、`crates/synthia-context/src/source/`，1 個刪除 `system_context.rs`，其餘為修改）。

**阻塞原因**：依專案記憶硬約束「Do not automatically commit changes; commit only after explicit user instruction」，本 agent 不得自動 commit。實作已完成（cargo check / fmt / clippy / test --workspace 全綠，553+ 測試通過），但 commit 動作待用戶明確指示。

**Commit 範圍**：待用戶指示後產生（建議分 5 個語意 commit 或單一 squash commit）。

---

## 6. Front-Door Routing Leak Detector（warning, 非阻塞）

```bash
ls docs/superpowers/specs/*.md 2>/dev/null
```

- [x] 無檔案（`(none)`）

**洩漏清單**：無

---

## 7. Deferred Manual Dogfood vs Automated Test Equivalence

`plan.md` 中無任何 `[~]` 標記的 deferred task（`grep -c '\[~\]'` = 0），本節無需填寫。

> plan.md 完全沒有 `[~]` row，本節空白即 PASS。

---

## Overall Decision

- [ ] ✅ PASS — 可進入 finishing-a-development-branch 與 archive
- [x] ⚠️ PASS WITH WARNINGS — 可進入後續步驟但需注意：實作已完成且全綠，但 commit 動作待用戶明確指示（專案記憶硬約束）。另 有 2 個 pre-existing main spec 失敗（`subagent-listing`、`v2-session-api`）與本 change 無關，建議作為獨立 follow-up。
- [ ] FAIL — 返回失敗的 artifact 修正後重跑 verify

**下一步**：

1. 等待用戶明確 commit 指示（依專案記憶硬約束，不得自動 commit）。
2. Commit 完成後重跑 verify §5 確認 Implementation Signal 通過。
3. 進入 `retrospective` artifact 產出。
4. 執行 `openspec archive -y` 同步 delta specs 並歸檔。
5. 使用 `superpowers:finishing-a-development-branch` 完成。
