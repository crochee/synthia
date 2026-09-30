## Why

Cycle #1 把 Synthia 前后端接口契约从口头协调升级为 git-tracked artifacts（37 endpoints、`contract.{yaml,md,json}`、ARBITRATION.md、4 Playwright specs、修复卡片 #001 闭环）。但双侧契约仍未 100% 一致：`tasks.md` §4.2–4.9 仍有 8 张未闭环修复卡片（涉及 `message:send` payload 命名、SSE 事件状态机、`artifact-update.lastChunk` 缺失、cancel handler、错误响应封装、SSE 重连反压、token 用量上报字段），且 §5.2 SSE 完整事件序列未落地（缺 Playwright SSE harness）。本次 change 是 cycle #2，主轴是修复卡片 #002–#009 + SSE harness，并补 §2.3 PR 模板漏项。修复完成后，所有登记接口 100% 行为守恒，CI advisory 升级为 blocking 的前置条件就位。

## What Changes

**修复卡片循环（#002–#009）**
- From: `tasks.md` §4.2–4.9 8 张卡片未闭环，每张都对应一个真实双侧不一致。
- To: 每张卡片逐条走"双侧实现 + `contract.yaml` entry + Playwright 正/反向用例"三同步；按 ARBITRATION.md 优先级定向（A2A 官方 > `@a2a-js/sdk` > Synthia stable spec）。
- Reason: cycle #1 验证三同步流程可工作；现在批量闭环剩余 fix cards。
- Impact: 局部破坏性（修改字段/枚举时前端同步），由 Playwright 用例守住。

**Playwright SSE harness**
- From: cycle #1 4 个 spec 全部是 HTTP/JSON 端点，无 SSE 流式事件用例。
- To: 新增 `synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.ts`，暴露 `subscribeAndCapture(url, options)` helper；驱动 server 真实产生 SSE 事件并捕获到内存数组，供 spec 断言。
- Reason: §5.2 修复卡片 #003/#004/#008 都依赖 SSE 行为验证；无 harness 写不出正/反向用例。
- Impact: 非破坏性（仅新增 helper + 3+ spec 用例）。

**PR 模板补漏（§2.3）**
- From: 缺 `.github/PULL_REQUEST_TEMPLATE.md`。
- To: 落地模板，包含"修改 `docs/interface-contract/contract.yaml` 必须同步 `contract-closure.*.spec.ts`"提示。
- Reason: cycle #1 retrospective §1.2 已指出此漏项；早补避免后续 fix card 漏同步。
- Impact: 非破坏性。

**§5.3 contract-coverage 报告"未覆盖路径"段落**
- From: `contract-coverage.ts` 只列已覆盖接口，无"未覆盖路径"。
- To: 报告新增 "未覆盖路径" 段落 + JSON 字段；CI advisory 阶段先列出，等 §6.1 升级后改 failing exit code。
- Reason: 为 blocking 升级做数据准备。
- Impact: 非破坏性（仅报告扩展）。

## Capabilities

### New Capabilities
（无 — cycle #2 不引入新 capability；#002–#009 是 cycle #1 引入的 `interface-contract-matrix` / `interface-contract-playwright` / `interface-contract-arbitration` 的持续执行）

### Modified Capabilities
- `interface-contract-playwright`: 新增 "SSE 事件 harness" requirement + "未覆盖路径在 coverage 报告中必须列出" requirement（从 cycle #1 的"100% 覆盖"细化到"SSE 事件 + 报告格式化"两个新 requirement）。
- `interface-contract-matrix`: 新增"修复卡片 #002–#009 对应 contract entry 全部登记" requirement（cycle #1 spec 只承诺"机器可读契约表"，未承诺"修复卡片对应 entry 落表"）。

## Impact

**代码影响**
- 修改：`synthia-web/src/api/**` / `synthia-web/src/pages/**`（按 fix card 定向，#002 改前端 message:send payload；#003/#004/#008 SSE reducer + 监听逻辑；#009 token usage 读取）
- 修改：`crates/synthia-server/src/server/**`（#004 SSE artifact-update lastChunk 字段；#006 cancel handler；#007 错误响应统一封装；#008 cadence metadata；#009 token usage 字段命名）
- 新增：`synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.ts`
- 新增：`.github/PULL_REQUEST_TEMPLATE.md`
- 修改：`contract-closure/contract-coverage.ts`（加 "未覆盖路径" 段落）
- 修改：`docs/interface-contract/contract.{yaml,md,json}`（每张 fix card 落地后重生成）

**API 影响**
- 客户端调用路径不变；后端路由不变（仅字段/枚举对齐，#006 唯一新增 cancel handler）。
- SSE 事件名/字段对齐；任何 schema 变更在修复卡片中明示"前端同步改动点"。

**依赖影响**
- 不引入新第三方依赖；Playwright 现有栈足够。

**系统影响**
- CI advisory 仍维持（§6.1 推迟到 cycle #2 末评估）。
- 升级为 blocking 在 cycle #2 archive 后单独评审。