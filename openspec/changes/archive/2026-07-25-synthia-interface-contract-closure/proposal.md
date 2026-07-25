## Why

Synthia 前后端联调存在**接口契约系统性脱节**风险：后端 `synthia-server` 已稳定提供 A2A 协议端点（REST + SSE），前端 `synthia-web` 通过 `@a2a-js/sdk` 调用，但双侧 schema / 字段命名 / 流式事件节奏靠手工对齐，缺乏"双侧契约源"+"覆盖率表"+"Playwright 行为验证"的三件套。本次 fix 闭环之前，任意后端字段重命名或前端新增调用点都可能静默失效，难以系统化回归。

本次 change 是上线工程分解后的**第一个子项目**（接口契约闭环），后续 spec 将分别承担 UI 测试深化、性能 < 300ms 调优、跨浏览器/响应式。本 spec 只承诺：100% 双侧接口 0 差异，并由 Playwright 集成测试守住。

## What Changes

**双侧契约并集表**
- From: 散落在 `crates/synthia-server/src/router.rs`、`.tsx/.ts` 调用点的临时一致性，无统一覆盖矩阵
- To: 在仓库内落地 `docs/interface-contract/contract.yaml`（机器可读）+ `docs/interface-contract/contract.md`（人可读），由扫描脚本从双侧 AST/正则提取并每日校验
- Reason: 把隐性契约变成显性资产
- Impact: 非破坏性，仅新增文档 + 扫描脚本

**冲突仲裁口径**
- From: 冲突发现后靠口头协调，决策不可追溯
- To: 写明「以官方协议源（A2A 官方 / `@a2a-js/sdk` / Synthia 既有 replay 模型）为准」并落到 README
- Reason: 减少反复决策的认知开销
- Impact: 非破坏性，仅新增规则文档

**Playwright 集成联调（合同层）**
- From: `synthia-web/tests/e2e/` 已有"三层"用例，但与 server 端契约耦合度不一，缺覆盖率统计
- To: 新增 `synthia-web/tests/e2e/integration/contract-closure.*.spec.ts`，每个被双侧契约表登记的接口至少 1 条测试；接入 CI 后所有用例绿作为本次完成的硬判据
- Reason: 把契约变成可执行的回归网
- Impact: 非破坏性（新增 spec files + CI 步骤），但需 CI 升级

**修复路径（按典型场景举例）**
- 场景 1：后端返回字段名 `userId`，前端读 `user_id` → 改前端（贴近协议源命名）
- 场景 2：前端调用 `POST /a2a/tasks/{id}:cancel`，但后端未注册 → 后端补 handler
- 场景 3：SSE `artifact-update` 事件缺 `lastChunk` 字段 → 后端补 + 前端处理 null
- 各场景逐项作为 tasks.md 的"修复卡片"，每张修复卡片自带 verification step
- Impact: 局部破坏性（修改 schema 时前端需同步），由 Playwright 用例守住

## Capabilities

### New Capabilities
- `interface-contract-matrix`: 双侧契约并集扫描 + 覆盖率表（`docs/interface-contract/*`）。
- `interface-contract-playwright`: `synthia-web/tests/e2e/integration/contract-closure.*.spec.ts` 全集，覆盖每条登记的接口。
- `interface-contract-arbitration`: 冲突仲裁规则文档（协议源优先），供所有未来修改参考。

### Modified Capabilities
（无 — 本 change 不修改既有 capability 的 REQUIREMENT 文本。若发现 `a2a-protocol-client` / `v2-session-api` / `session-replay-harness` 与本次契约表存在冲突，作为修复卡片走"三同步"流程但不动该 cap 的 spec.md，避免污染上游 archive apply。）

## Impact

**代码影响**
- 新增：`scripts/contract-scan.{ts,rs}`（双侧扫描 + 报告生成）
- 新增：`docs/interface-contract/contract.{yaml,md}`（契约表）
- 新增：`synthia-web/tests/e2e/integration/contract-closure.*.spec.ts`
- 修改：`synthia-server/src/router.rs`、`synthia-web/src/api/**`（仅契约不一致项）
- 修改：`synthia-web/playwright.config.ts` / `package.json`（CI 跑契约集）
- 修改：`crates/synthia-server/src` 内部错误响应统一封装（若发现当前分散）

**API 影响**
- 客户端调用路径不变；后端路由不变（仅字段/枚举对齐）。
- SSE 事件名/字段对齐；任何 schema 变更在修复卡片中明示"前端同步改动点"。
- 所有变更逐项走 `tasks.md` 的修复卡片 + Playwright 用例验证。

**依赖影响**
- 锁定 `@a2a-js/sdk` 在 `synthia-web/package.json` 当前版本（提交 `package-lock.json`）。
- 不引入新的第三方依赖（仅在 `playwright` 已有栈内扩展）。

**系统影响**
- CI（`.github/workflows/` 或 Makefile 目标）新增 `make test-contract-closure`，跑契约扫描 + Playwright 契约集。
- `make dev`、`make build`、`make test` 三条命令语义不变，仅行为更严格。
