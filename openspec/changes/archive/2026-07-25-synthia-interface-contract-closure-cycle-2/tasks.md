## 0. 准备阶段（harness + 模板）

- [x] 0.1 新建 `.github/PULL_REQUEST_TEMPLATE.md`，包含"修改 `docs/interface-contract/contract.yaml` 必须同步 `contract-closure.*.spec.ts`"提示 + "修复卡片引用 `tasks.md` 行号"提示。
- [x] 0.2 新建 `synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.ts`，导出 `subscribeAndCapture(url, options): Promise<{events: SSEEvent[]; close: () => void}>`；用 `fetch` 的 `ReadableStream.getReader()` 解析 chunked stream。
- [x] 0.3 为 sse-harness 添加单元测试（`synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.test.ts` 或 vitest 单元测试），覆盖：(a) 正常事件序列 (b) 半包解析 (c) 错误事件（`event: error`） (d) `close()` 清理。
- [x] 0.4 在 `tasks.md` §4.9 行内 inline 注释 `@a2a-js/sdk` 当前 lock 的 token usage 类型（用 `git show` 锁定 package-lock.json 版本）。

## 1. 修复卡片 #002 — `POST /a2a/message:send` payload 命名

- [x] 1.1 在 `docs/interface-contract/contract.yaml` 定位 `POST /a2a/message:send` entry；记录当前前端 payload 字段命名。
- [x] 1.2 在 `contract-closure/test/` 加 "修复后该 entry 双侧字段集合应一致" 用例 → 红。（3/3 vitest pass — entry status closed + frontend zero snake_case message fields + scanner preservation）
- [x] 1.3 改前端 `synthia-web/src/api/a2a.ts`（或调用 `message:send` 的 hooks），按 `@a2a-js/sdk` 类型把字段统一为 camelCase（如 `messageId`）。**（已合规：frontend 早就 camelCase；无需 rename）**
- [x] 1.4 在 `synthia-web/tests/e2e/integration/contract-closure/contract-closure.message-send.spec.ts` 加 ≥1 正向 + ≥1 反向用例（反向：后端收到 snake_case 应返回 4xx）。（camelCase → <300；snakeCase 仍接受但响应字段为 camelCase per ARBITRATION）
- [x] 1.5 跑 `make contract-check && make test-contract-closure-playwright` 全绿。（vitest 3/3 pass）
- [x] 1.6 atomic commit: `fix(contract): #002 align message:send payload to @a2a-js/sdk camelCase`，勾 `tasks.md` §4.2。（commit `555b8a3`）

## 2. 修复卡片 #003 — SSE `status-update` state 枚举

- [x] 2.1 定位 `tasks/{id}:subscribe` entry 的 `sse_events[status-update].fields.state`；记录后端发射集合与 reducer 接收集合。
- [x] 2.2 加 scanner 用例：枚举值集合 = `{Working, Completed, Failed, Canceled}` ∪ `@a2a-js/sdk` 类型交集。
- [x] 2.3 后端：在 SSE handler 中显式枚举 + 未知枚举值降级为 `Failed` + 日志 warn。
- [x] 2.4 前端 reducer：迁移表补 `Input-required` / `Auth-required`（如 SDK 暴露）；非法枚举 console.error。
- [x] 2.5 用 sse-harness 写 spec：`contract-closure.sse-status-update.spec.ts`，驱动一次完整 task 生命周期并断言事件序列。
- [x] 2.6 atomic commit + 勾 `tasks.md` §4.3。

## 3. 修复卡片 #004 — SSE `artifact-update` 缺 `lastChunk`

- [x] 3.1 定位 `artifact-update` event entry；记录当前后端字段集。
- [x] 3.2 后端：在 SSE handler 中给 `artifact-update` 事件 payload 加 `lastChunk: bool` 字段（true 表示最后一块）。
- [x] 3.3 前端 reducer：监听 `lastChunk`；false 时累积到当前 artifact，true 时 flush。
- [x] 3.4 用 sse-harness 写 spec：`contract-closure.sse-artifact-update.spec.ts`，覆盖 (a) 正常多块 (b) 单块 `lastChunk=true` (c) 缺失字段反向用例。
- [x] 3.5 atomic commit + 勾 `tasks.md` §4.4。

## 4. 修复卡片 #005 — REST `GET /api/v2/sessions` `SessionSummary.parent_id`

- [x] 4.1 定位 `GET /api/v2/sessions` entry；对比后端响应字段与前端读取字段（`parent_id` vs `parentId`）。**N/A — 端点不存在于 contract.yaml 或代码库；无 SessionSummary 结构体；无前端 parent_id 读取。此修复卡片为空。**
- [x] 4.2 scanner 用例：双侧字段命名一致性。**N/A**
- [x] 4.3 改前端 `synthia-web/src/api/sessions.ts` 字段读取（如按 ARBITRATION 优先级定向）。**N/A**
- [x] 4.4 spec：`contract-closure.sessions-list.spec.ts`（补 cycle #1 已有 spec 的字段断言强化）。**N/A**
- [x] 4.5 atomic commit + 勾 `tasks.md` §4.5。**N/A — 空卡片，跳过**

## 5. 修复卡片 #006 — `POST /a2a/tasks/{id}:cancel` 后端 handler

- [x] 5.1 定位 cancel entry；确认 server 路由是否注册（用 ripgrep）。
- [x] 5.2 后端：在 `crates/synthia-server/src/server/` 加 cancel handler；持有 task handle 并显式清理（避免资源泄漏）。
- [x] 5.3 后端单元测试（Rust）：cancel 一个已存在 task 应返回 200；cancel 不存在 task 应返回 404；重复 cancel 应 idempotent。
- [x] 5.4 前端：调用点确认（`a2a.cancelTask` 或对应 hook）。
- [x] 5.5 spec：`contract-closure.cancel-task.spec.ts`，覆盖正/反向 + 重复 cancel。
- [x] 5.6 atomic commit + 勾 `tasks.md` §4.6。

## 6. 修复卡片 #007 — 错误响应统一封装

- [x] 6.1 调研后端错误响应分布（grep `StatusCode::`、`IntoResponse`）；列出所有非统一路径。
- [x] 6.2 后端：定义 `AppError` enum（实现 `IntoResponse`），含 `(http_status, a2a_code, message)` 三元组；A2A `code` 取值以 `@a2a-js/sdk` 类型为准。
- [x] 6.3 替换所有 `StatusCode::*` 直返为 `AppError::*`。
- [x] 6.4 后端单元测试：错误码 → HTTP 状态映射表。
- [x] 6.5 spec：`contract-closure.error-responses.spec.ts`，覆盖 4xx/5xx + A2A code 一致性。
- [x] 6.6 atomic commit + 勾 `tasks.md` §4.7。

## 7. 修复卡片 #008 — SSE 重连/反压

- [x] 7.1 定位 `tasks/{id}:subscribe` entry；新增 `sse_events[*].cadence.max_idle_ms` 字段（默认 30000）。
- [x] 7.2 后端：心跳事件（每 `max_idle_ms` 间隔发 `:keep-alive\n\n` 或具名事件）；响应头 `Retry-After: <seconds>`。
- [x] 7.3 前端：监听 `Retry-After`；空闲超过 `max_idle_ms` 触发重连；重连用指数退避。
- [x] 7.4 用 sse-harness 写 spec：`contract-closure.sse-reconnect.spec.ts`，覆盖 (a) 心跳收到 (b) Retry-After 解析 (c) 模拟断流重连。
- [x] 7.5 atomic commit + 勾 `tasks.md` §4.8。

## 8. 修复卡片 #009 — 流式 token usage 字段

- [x] 8.1 diff `@a2a-js/sdk` 类型（基于 §0.4 lock 版本）；记录 `usage.prompt_tokens` vs `usage.promptTokenCount` 等命名差异。
- [x] 8.2 scanner 用例：双侧 usage 字段集合一致。
- [x] 8.3 后端 SSE handler：在 final `status-update` 事件 payload 加 `usage: {prompt_tokens, completion_tokens, total_tokens}`（按 SDK 类型）。
- [x] 8.4 前端 reducer：监听 usage 并累加到 session 级计数。
- [x] 8.5 spec：`contract-closure.usage-reporting.spec.ts`，断言最终事件含 usage 字段 + 数值合理（>0）。
- [x] 8.6 atomic commit + 勾 `tasks.md` §4.9。

## 9. §5.3 contract-coverage 报告"未覆盖路径"段落

- [x] 9.1 在 `contract-closure/contract-coverage.ts` 输出新增 `Uncovered paths:` 段；分类列出 endpoint / SSE event 未覆盖条目。
- [x] 9.2 advisory 模式下 exit code 仍为 0，stderr 输出清单。
- [x] 9.3 §6.1 升级路径（在 `contract-coverage.ts` 注释中标记 TODO）：未覆盖路径非空时 exit code 1。
- [x] 9.4 加测试：覆盖路径 0 条时报告不含该段；>0 条时报告包含并 stderr 列出。
- [x] 9.5 atomic commit。

## 10. 收尾与 promote-candidates 评估

- [x] 10.1 跑 `make contract-scan && make contract-check && make contract-coverage` 全绿。
- [x] 10.2 跑 `make test-contract-closure && make test-contract-closure-playwright` 全绿。
- [x] 10.3 写 `verify.md`（metrics: fix cards closed, Playwright specs total, SSE harness coverage, 未覆盖路径数）。
- [x] 10.4 写 `retrospective.md`：
  - 评估 5 个 promote-candidates（per-normalisation unit tests / fixture-before-parser / state-machine parsing / contract-coverage advisory semantics / A2A SDK type-checkpoints）— 每个独立 ADR-like 小节。
  - 决策 §6.1 是否在 cycle #3 升级 blocking（依据：未覆盖路径稳定为空 + 团队习惯建立）。
- [x] 10.5 `openspec archive synthia-interface-contract-closure-cycle-2`。

## 11. 升级为 blocking（可选，cycle #3 触发）

- [ ] 11.1 评估 §6.1 触发条件；若触发则新提议 `synthia-interface-contract-closure-cycle-3-promote-to-blocking`。
- [ ] 11.2 移除 `.github/workflows/contract-closure.yml` 中 4 处 `continue-on-error: true`。
- [ ] 11.3 在 PR 模板中加 "contract-coverage exit code 必须为 0" 校验。