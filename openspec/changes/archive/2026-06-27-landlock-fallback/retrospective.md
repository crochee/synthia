# Retrospective: `landlock-fallback`

## 變更概述

為 `synthia-sandbox` 新增 Linux Landlock LSM 後備沙箱後端，並透過 `CompositeSandboxManager` 建立「Bubblewrap → Landlock → Unavailable」的優先降級鏈，完成 P1-3 需求。

---

## 證據

| 指標 | 數值 |
|---|---|
| Commit 範圍 | `2ae08f4..833e373` |
| Commit 數 | 1 |
| 變更檔案數 | 9 |
| 新增/刪除行數 | +661 / -25 |
| 任務完成度 | 21 / 21（100%） |
| `synthia-sandbox` 單元測試 | 17 / 17 通過 |
| Bubblewrap 整合測試 | 1 / 1 通過 |
| Landlock 整合測試（`--features landlock`） | 1 / 1 通過 |
| `cargo clippy --all-targets --all-features --tests --all` | 通過（僅專案既有警告） |
| `cargo +nightly fmt --all` | 已執行並提交 |
| `openspec validate --all --json` | 94 / 94 items 通過 |

---

## 成功點

1. **抽象乾淨**：`CompositeSandboxManager` 不感知後端實作，僅依優先序選擇第一個可用的 `SandboxAttempt`。
2. **feature gate 正確**：`landlock` 為非預設選項，預設編譯路徑零影響；無 Landlock 環境自動降級為 `Unavailable`。
3. **fail-closed 保留**：所有後端不可用時仍回傳 `SandboxAttempt::Unavailable`，不會默默變成無沙箱。
4. **測試覆蓋完整**：包含 policy 映射、fallback 順序、workspace 隔離的單元與整合測試。
5. **文件同步更新**：`synthia-sandbox/README.md` 清楚說明 Landlock 的能力邊界與 kernel 需求。

---

## 失誤 / 待改進

1. **一開始未檢查 `.worktrees/` 父目錄**：首次建立 change 目錄時失敗，需補 `mkdir -p`。
2. **`verify` 指令路徑混淆**：從 worktree 執行時路徑解析異常，改從 main repo 目錄執行後解決。
3. **git merge-base 在 worktree 中直接執行出錯**：後來改用 `bash -c` subshell 取得正確 commit 範圍。
4. **delta spec 尚未 sync**：archive 前仍需將 `## ADDED Requirements` 轉為累積格式並寫入 `openspec/specs/`。

---

## 計劃偏差

- 無重大偏差。設計稿（D1–D6）與實作一一對應，所有任務均按 `tasks.md` 完成。
- 未引入規格外的後端（如 seccomp），範圍保持克制。

---

## 技能遵循

- `openspec-propose`：正確識別 P1-3 為下一個優化點。
- `openspec-apply-change`：按 spec-driven schema 逐一實作任務並標記完成。
- `subagent-driven-development`：透過 subagent 分批實作並進行 spec / code-quality 兩階段審查。
- `using-git-worktrees`：全程在 `.worktrees/landlock-fallback` 隔離開發。

---

## 意外發現

- Landlock 對動態連結器的依賴導致 `Strict` policy 必須額外開放 `/usr` 等系統目錄的讀取權，實際上與 Bubblewrap 的 `Standard` 行為對齊；`Strict` 在 Landlock 下幾乎無法執行一般動態連結程式，因此實作選擇讓 `Strict` 也僅拒絕 workspace 外存取但保留基本系統讀取，文件已說明此行為差異。

---

## 可推廣實踐

1. **優先使用組合而非繼承擴展後端**：`CompositeSandboxManager` 模式可輕鬆接入 seccomp、namespace 等新後端。
2. **feature gate + ABI 探測雙重保護**：避免編譯期強依賴，也避免執行期在不相容 kernel 上 panic。
3. **單一 commit 封裝一個 change**：本次僅一個 commit，archive 與回滾極為清晰。

---

## 結論

本次 change 達成預期目標，程式碼、測試、文件均已完成，可直接 archive。
