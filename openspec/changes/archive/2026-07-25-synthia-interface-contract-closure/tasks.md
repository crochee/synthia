## 1. 扫描器与契约表（基线）

- [x] 1.1 在 `contract-closure/` 新增 `contract-scan.ts` 与 `contract-check.ts`，实现合成"双侧契约并集"（HTTP 路径 + JSON-RPC method + SSE 事件）。
- [x] 1.2 在 `contract-closure/` 新增 `contract-report.ts`，从 `contract.yaml` 生成人类可读 `contract.md`。
- [x] 1.3 添加 `contract-closure/test/*.test.ts`（vitest），覆盖 backend-scanner / frontend-scanner / unifier（fixture + 双侧悬空检测）。
- [x] 1.4 落地首版 `docs/interface-contract/contract.yaml` + `docs/interface-contract/contract.md` + `contract.json`。
- [x] 1.5 落地 `docs/interface-contract/ARBITRATION.md`，固化协议源优先级（A2A 官方 / `@a2a-js/sdk` / Synthia stable spec）。
- [x] 1.6 把 `contract-scan` / `contract-check` / `contract-coverage` / `test-contract-closure` 接入 `Makefile`，并与 `make help` 文档对齐。

## 2. CI 接入（advisory 起步）

- [x] 2.1 在 `.github/workflows/contract-closure.yml` 中以 advisory（`continue-on-error: true`）模式运行 `make contract-check`。
- [x] 2.2 同样 advisory 运行 `make contract-scan` 与 `make contract-coverage`。
- [ ] 2.3 在 PR 模板加入"修改 contract.yaml 必须同步 contract-closure specs"的提示。

## 3. Playwright 契约集基线

- [x] 3.1 新建 `synthia-web/tests/e2e/integration/contract-closure/` 目录骨架（fixtures、helpers、配置）。
- [x] 3.2 新增 `synthia-web/playwright.contract.config.ts`，将 `testDir: 'tests/e2e/integration/contract-closure'` 并独立 `reportDir`。
- [x] 3.3 落地 `contract-closure.health.spec.ts`、`contract-closure.agent-card.spec.ts`、`contract-closure.models-list.spec.ts`、`contract-closure.tasks-list.spec.ts`（首批 4 个"健康/发现/资源"用例，断言状态码 + 关键字段）。
- [x] 3.4 添加 `synthia-web/tests/e2e/integration/contract-closure/_helpers/list-endpoints-from-yaml.ts`，从 `docs/interface-contract/contract.yaml` 自动枚举测试路径。

## 4. 双侧契约修复（按典型场景的修复卡片）

- [x] 4.1 修复卡片 #001：GET `/.well-known/agent-card.json` — 前端 `useServerHealth` 字段读取与后端响应字段命名差异。引用 ARBITRATION.md 优先级；按 A2A 官方对齐。（见 commit `b693fa8`，前端 `TasksPage.tsx` 调整；契约表 `agent-card` 行落地；Playwright `contract-closure.agent-card.spec.ts` 守住。）
- [ ] 4.2 修复卡片 #002：POST `/a2a/message:send` — request payload 中 `messageId` vs `message_id` 命名差异。修改前端（协议源为准）。
- [ ] 4.3 修复卡片 #003：SSE `tasks/{id}:subscribe` 事件 `status-update` 字段 `state` 枚举值（`Working` / `Completed` / `Failed` / `Canceled`）与前端 reducer 状态机对齐。
- [ ] 4.4 修复卡片 #004：SSE `artifact-update` 缺 `lastChunk` 字段 — 后端补齐 + 前端 null 处理。
- [ ] 4.5 修复卡片 #005：REST `GET /api/v2/sessions` `SessionSummary.parent_id` — 复核前端取字段命名。
- [ ] 4.6 修复卡片 #006：前端调用了 `POST /a2a/tasks/{id}:cancel`，但 server 路由未注册 → 后端补 handler + 单元测试。
- [ ] 4.7 修复卡片 #007：错误响应统一封装（A2A `code` 取值集合；HTTP 状态码语义）— 以 `@a2a-js/sdk` 类型为准。
- [ ] 4.8 修复卡片 #008：SSE 重连/反压策略 — 在契约表 `events[].cadence` 中固化最大静止间隔，前端按 `Retry-After` 处理。
- [ ] 4.9 修复卡片 #009：流式 token 用量上报字段（`usage.prompt_tokens` / `completion_tokens`）双侧对齐。
- [ ] 4.10 （发现更多修复卡片 → 在 `tasks.md` 末尾追加编号，引用 4.x 模式）

## 5. Playwright 用例与三同步

- [x] 5.1 每个修复卡片 MUST 同步：(a) 双侧实现 (b) `contract.yaml` (c) 对应 `contract-closure.*.spec.ts` 正向/反向用例。（#001 三同步达成；#002–#009 留待下一 cycle。）
- [ ] 5.2 为 SSE 事件补"完整事件序列"用例（task 创建 → status-update×N → artifact-update×N → final status）。（需 Playwright SSE harness）
- [ ] 5.3 在 contract-coverage 报告中加入"未覆盖路径"段落，CI 升级为 blocking 后，任何缺覆盖路径必须 ticket 化。

## 6. 升级与收尾

- [ ] 6.1 把 `make contract-check` 与 `make test-contract-closure` 在 CI 中由 advisory 升级为 blocking。（deliberate，让团队先建立 contract-driven 习惯）
- [x] 6.2 在 `verify.md` 复盘：(a) 契约表行数 (b) 修复卡片总数 (c) Playwright 用例数 (d) 仲裁源引用次数。
- [x] 6.3 在 `retrospective.md` 沉淀"双层（contract-scan + Playwright）模板"，供后续上线工程复用。
