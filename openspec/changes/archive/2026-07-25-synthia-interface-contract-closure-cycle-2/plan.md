# `synthia-interface-contract-closure-cycle-2` Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development (or
> any suitable implementation driver) to execute this plan task-by-task.

**Goal:** 闭环 cycle #1 推迟的 8 张修复卡片（#002–#009），并补 SSE harness + PR 模板 + §5.3 报告格式化；CI advisory → blocking 升级时机评估留至 cycle #2 末。

**Architecture:** 在 cycle #1 的双层校验骨架（`contract-closure/` + `tests/e2e/integration/contract-closure/`）之上增量：先建 `_helpers/sse-harness.ts` 与 PR 模板；再按 #002 → #003 → #004 → #005 → #006 → #007 → #008 → #009 顺序串闭环；最后扩展 `contract-coverage.ts` 报告。

**Tech Stack:** TypeScript（`contract-closure/`, Playwright helpers, frontend edits）, Rust（`synthia-server` 局部改动：cancel handler, error envelope, SSE fields, usage fields, cadence）, Makefile, GitHub Actions.

---

## Task 0: PR 模板 + SSE harness（最先做）

**Files:**
- `.github/PULL_REQUEST_TEMPLATE.md` (NEW)
- `synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.ts` (NEW)
- `synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.test.ts` (NEW)

- [ ] **Step 1:** 创建 `.github/PULL_REQUEST_TEMPLATE.md`，包含 checklist：`[ ] contract.yaml 同步` / `[ ] contract-closure.*.spec.ts 同步` / `[ ] tasks.md §4.x 行号引用`。
- [ ] **Step 2:** 创建 `synthia-harness.ts`：`subscribeAndCapture(url, options)` 用 `fetch(url).then(r => r.body.getReader())`；按 SSE 协议解析 `event:` / `data:` 字段；返回 `{events: SSEEvent[], close: () => reader.cancel()}`。
- [ ] **Step 3:** 单元测试：(a) 给定 3 个 chunked 事件的 fixture 字符串（用 `TextEncoder`），断言 events.length=3 + 字段正确；(b) 半包解析（手动拆分 chunk 边界）；(c) `event: error` 不抛错但写入 events；(d) `close()` 调用后 reader.cancel 被触发。
- [ ] **Step 4:** `pnpm -C synthia-web test sse-harness` 全绿。Commit: `feat(contract-closure): sse-harness + PR template`。
- [ ] **Step 5:** 在 `tasks.md` §4.9 行内 inline 注释 `@a2a-js/sdk` lock 版本：`grep '"@a2a-js/sdk"' synthia-web/package-lock.json | head -1`。

---

## Task 1: 修复卡片 #002 — `message:send` payload camelCase

**Files:**
- `docs/interface-contract/contract.yaml` (modify — 加 entry status)
- `contract-closure/test/` (加 fixture + dangling 用例)
- `synthia-web/src/api/a2a.ts` (modify — 字段统一 camelCase)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.message-send.spec.ts` (NEW)

- [ ] **Step 1:** 读 `tasks.md` §4.2 + ARBITRATION.md (2) `@a2a-js/sdk` 段；定位 contract.yaml 中 `POST /a2a/message:send` entry，记录当前字段命名。
- [ ] **Step 2:** 加 scanner fixture 用例："修复后 message:send payload 双侧字段 camelCase 一致"。
- [ ] **Step 3:** 跑测试 → 确认红。
- [ ] **Step 4:** 改前端 `synthia-web/src/api/a2a.ts`（或调用 message:send 的 hooks）：payload 字段统一 camelCase（按 SDK 类型 `MessageSendParams`）。
- [ ] **Step 5:** 在 `synthia-web/tests/e2e/integration/contract-closure/contract-closure.message-send.spec.ts` 加：(a) 正向：camelCase payload → server 200；(b) 反向：snake_case payload → server 4xx。
- [ ] **Step 6:** `make contract-check && make test-contract-closure-playwright` 全绿。Commit: `fix(contract): #002 align message:send payload to @a2a-js/sdk camelCase`。勾 `tasks.md` §4.2。

---

## Task 2: 修复卡片 #003 — SSE `status-update` state 枚举

**Files:**
- `docs/interface-contract/contract.yaml` (modify)
- `crates/synthia-server/src/server/` (modify — SSE handler)
- `synthia-web/src/hooks/useAgentReducer.ts` (modify — 迁移表)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.sse-status-update.spec.ts` (NEW)

- [ ] **Step 1:** 定位 `tasks/{id}:subscribe` entry 的 `sse_events[status-update].fields.state`；列出后端发射集合与 reducer 接收集合的差集。
- [ ] **Step 2:** scanner fixture：枚举值集合 = `{Working, Completed, Failed, Canceled}` ∩ SDK 类型。
- [ ] **Step 3:** 后端：在 SSE handler 显式枚举；未知值降级 `Failed` + warn log。
- [ ] **Step 4:** 前端 reducer：迁移表补齐；非法 console.error。
- [ ] **Step 5:** 用 sse-harness 写 spec：驱动一次完整 task 生命周期，断言 events 序列中 status-update 枚举值全部命中允许集合。
- [ ] **Step 6:** 全绿。Commit: `fix(contract): #003 SSE status-update state enum alignment`。勾 `tasks.md` §4.3。

---

## Task 3: 修复卡片 #004 — SSE `artifact-update` 缺 `lastChunk`

**Files:**
- `docs/interface-contract/contract.yaml` (modify)
- `crates/synthia-server/src/server/` (modify)
- `synthia-web/src/hooks/useAgentReducer.ts` (modify)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.sse-artifact-update.spec.ts` (NEW)

- [ ] **Step 1:** 后端 SSE handler：artifact-update payload 加 `lastChunk: bool`。
- [ ] **Step 2:** 前端 reducer：监听 `lastChunk`；false 累积、true flush。
- [ ] **Step 3:** sse-harness spec：(a) 多块序列最后 `lastChunk=true`；(b) 单块 `lastChunk=true`；(c) 缺失字段反向。
- [ ] **Step 4:** 全绿。Commit: `fix(contract): #004 SSE artifact-update lastChunk`。勾 `tasks.md` §4.4。

---

## Task 4: 修复卡片 #005 — `SessionSummary.parent_id` 命名

**Files:**
- `docs/interface-contract/contract.yaml` (modify)
- `synthia-web/src/api/sessions.ts` (modify)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.sessions-list.spec.ts` (modify)

- [ ] **Step 1:** 对比后端响应与前端读取字段名。
- [ ] **Step 2:** scanner 用例：双侧字段命名一致。
- [ ] **Step 3:** 改前端按 ARBITRATION 优先级定向。
- [ ] **Step 4:** 在 cycle #1 已有 `sessions-list.spec.ts` 加字段断言（无需新建 spec）。
- [ ] **Step 5:** 全绿。Commit: `fix(contract): #005 SessionSummary.parent_id field naming`。勾 `tasks.md` §4.5。

---

## Task 5: 修复卡片 #006 — cancel handler

**Files:**
- `crates/synthia-server/src/server/router.rs` (modify — 加 cancel route)
- `crates/synthia-server/src/server/cancel.rs` (NEW)
- `crates/synthia-server/src/server/cancel_test.rs` (NEW — 单元测试)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.cancel-task.spec.ts` (NEW)

- [ ] **Step 1:** ripgrep 确认 server 当前 cancel route 是否注册；如未注册，规划路径。
- [ ] **Step 2:** 实现 `cancel.rs`：handler 持有 task handle，清理资源；返回 200。
- [ ] **Step 3:** Rust 单元测试：(a) cancel 存在 task → 200；(b) cancel 不存在 task → 404；(c) 重复 cancel → idempotent 200。
- [ ] **Step 4:** 前端：调用点确认（`a2a.cancelTask` 或 hooks）。
- [ ] **Step 5:** Playwright spec：正/反向 + 重复 cancel。
- [ ] **Step 6:** `cargo test -p synthia-server` + Playwright 全绿。Commit: `fix(contract): #006 register cancel task handler`。勾 `tasks.md` §4.6。

---

## Task 6: 修复卡片 #007 — 错误响应统一封装

**Files:**
- `crates/synthia-server/src/error.rs` (NEW)
- `crates/synthia-server/src/server/**` (modify — 替换散落 StatusCode)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.error-responses.spec.ts` (NEW)

- [ ] **Step 1:** ripgrep 散落 `StatusCode::`；列出所有非统一路径。
- [ ] **Step 2:** 定义 `AppError` enum（实现 `IntoResponse`），含 `(http_status, a2a_code, message)`；A2A `code` 以 `@a2a-js/sdk` 类型为准。
- [ ] **Step 3:** 替换所有 `StatusCode::*` 直返为 `AppError::*`。
- [ ] **Step 4:** Rust 单元测试：错误码 → HTTP 状态映射表。
- [ ] **Step 5:** Playwright spec：覆盖 4xx/5xx + A2A code 一致性。
- [ ] **Step 6:** 全绿。Commit: `fix(contract): #007 unify error response envelope`。勾 `tasks.md` §4.7。

---

## Task 7: 修复卡片 #008 — SSE 重连/反压

**Files:**
- `docs/interface-contract/contract.yaml` (modify — cadence 字段)
- `crates/synthia-server/src/server/` (modify — heartbeat)
- `synthia-web/src/hooks/useAgentReducer.ts` (modify — Retry-After + 重连)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.sse-reconnect.spec.ts` (NEW)

- [ ] **Step 1:** contract.yaml 加 `sse_events[*].cadence.max_idle_ms: 30000`。
- [ ] **Step 2:** 后端：心跳事件；响应头 `Retry-After: <seconds>`。
- [ ] **Step 3:** 前端：监听 Retry-After；空闲超过 max_idle_ms 触发重连；指数退避。
- [ ] **Step 4:** sse-harness spec：(a) 心跳收到；(b) Retry-After 解析；(c) 模拟断流重连（mock fetch 返回 503）。
- [ ] **Step 5:** 全绿。Commit: `fix(contract): #008 SSE reconnect + backpressure`。勾 `tasks.md` §4.8。

---

## Task 8: 修复卡片 #009 — token usage 字段

**Files:**
- `docs/interface-contract/contract.yaml` (modify)
- `crates/synthia-server/src/server/` (modify — SSE handler 加 usage)
- `synthia-web/src/hooks/useAgentReducer.ts` (modify — usage 累加)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.usage-reporting.spec.ts` (NEW)

- [ ] **Step 1:** diff `@a2a-js/sdk` 类型（基于 §0.4 lock）；记录 usage 字段命名（`prompt_tokens` vs `promptTokenCount`）。
- [ ] **Step 2:** scanner 用例：双侧 usage 字段集合一致。
- [ ] **Step 3:** 后端 SSE handler：final `status-update` payload 加 `usage: {prompt_tokens, completion_tokens, total_tokens}`（按 SDK 类型）。
- [ ] **Step 4:** 前端 reducer：监听 usage 并累加到 session 级计数。
- [ ] **Step 5:** spec：断言最终事件含 usage + 数值 > 0。
- [ ] **Step 6:** 全绿。Commit: `fix(contract): #009 token usage field alignment`。勾 `tasks.md` §4.9。

---

## Task 9: §5.3 contract-coverage 未覆盖路径段落

**Files:**
- `contract-closure/contract-coverage.ts` (modify)
- `contract-closure/test/contract-coverage.test.ts` (NEW)

- [ ] **Step 1:** 输出新增 `Uncovered paths:` 段；分类 endpoint / SSE event 未覆盖。
- [ ] **Step 2:** advisory 模式 exit code = 0；stderr 输出清单。
- [ ] **Step 3:** TODO 注释标记 §6.1 升级路径。
- [ ] **Step 4:** 测试：(a) 0 未覆盖时报告不含段；(b) >0 时报告含段 + stderr 列出。
- [ ] **Step 5:** 全绿。Commit: `feat(contract-coverage): uncovered paths section (advisory)`。

---

## Task 10: 收尾 + promote-candidates 评估

**Files:**
- `openspec/changes/synthia-interface-contract-closure-cycle-2/verify.md` (NEW)
- `openspec/changes/synthia-interface-contract-closure-cycle-2/retrospective.md` (NEW)

- [ ] **Step 1:** 全跑：`make contract-scan && make contract-check && make contract-coverage`。
- [ ] **Step 2:** 全跑：`make test-contract-closure && make test-contract-closure-playwright`。
- [ ] **Step 3:** 写 `verify.md`：metrics (fix cards closed: 8, Playwright specs total, SSE harness coverage, 未覆盖路径数)。
- [ ] **Step 4:** 写 `retrospective.md`：5 个 promote-candidates 各独立 ADR 小节 + §6.1 升级决策。
- [ ] **Step 5:** `openspec archive synthia-interface-contract-closure-cycle-2`。

---

## Task 11: 升级为 blocking（可选，cycle #3 触发）

> **本 cycle 不执行；仅作为 cycle #3 的占位。**

- [ ] **Step 1:** 评估 §6.1 触发条件；若触发则新提议 `synthia-interface-contract-closure-cycle-3-promote-to-blocking`。
- [ ] **Step 2:** 移除 `.github/workflows/contract-closure.yml` 4 处 `continue-on-error: true`。
- [ ] **Step 3:** PR 模板加 "contract-coverage exit code 必须为 0" 校验。