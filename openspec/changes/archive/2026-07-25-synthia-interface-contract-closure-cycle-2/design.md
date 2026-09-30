## Context

承接 cycle #1（archived 2026-07-25），Synthia 前后端契约表与 Playwright 契约集基线已就位（37 endpoints、`contract.{yaml,md,json}`、ARBITRATION.md、4 Playwright specs、CI advisory workflow、修复卡片 #001 闭环）。但仍有 8 张修复卡片（#002–#009）未闭环，且缺 SSE 完整事件序列的 Playwright harness。本 change（cycle #2）目标：按 #002→#003→#004→#005→#006→#007→#008→#009 顺序串闭环剩余 fix cards，并补 SSE harness、PR 模板（§2.3）、contract-coverage 未覆盖路径段落（§5.3）。§6.1（CI blocking 升级）保持 deliberate 推迟至 cycle #2 末评估。

Stakeholder：开发工程师（执行修复卡片 + 三同步）、CI 维护者（升级 advisory → blocking 时机评审）、前端团队（最密集的修改承担方）。

## Goals / Non-Goals

**Goals:**
- 闭环修复卡片 #002–#009，每张均完成"双侧实现 + contract.yaml entry + Playwright 正/反向用例"三同步。
- 落地 Playwright SSE harness（`_helpers/sse-harness.ts`），使修复卡片 #003/#004/#008 可写 SSE 流式事件序列用例。
- 落地 §2.3 PR 模板（`contract.yaml` 变更提示）。
- 落地 §5.3 `contract-coverage` 报告"未覆盖路径"段落（advisory 模式下 warn-level）。
- 评估 5 个 promote-candidates，决定是否纳入 cycle #3 或在本 cycle 内执行。

**Non-Goals:**
- 修改既有 capability（`interface-contract-arbitration`、`a2a-protocol-client`、`v2-session-api`、`session-replay-harness`）的 REQUIREMENT 文本。
- CI advisory → blocking 升级（§6.1，deliberate 推迟；本 cycle 末评估）。
- 视觉/交互测试、性能调优、安全审计、跨浏览器/响应式（其他 spec 范围）。
- 业务逻辑重构、模型路由改造、新增能力。

## Decisions

### D1：修复卡片执行顺序 = #002 → #003 → #004 → #005 → #006 → #007 → #008 → #009
- **选择**：按"字段层 → 错误路径 → SSE 重连 → 流式用量"难度递增顺序串。
- **理由**：每张 card 都需 harness + contract entry + spec 三同步；后续 card 复用前序 card 的 harness 基础设施。
- **已考虑 alternative**：随机顺序（依赖链混乱）；倒序（§5.2 harness 未建就跑 #008/009）。**拒绝理由**：见 brainstorm Q1。

### D2：SSE harness = helper 模块，非独立 Playwright project
- **选择**：在 `synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.ts` 提供 `subscribeAndCapture(url, options)`；不新增 `playwright.sse.config.ts`。
- **理由**：Playwright 已用一个 project 跑 `tests/e2e/integration/`，再开新 project 维护成本高；harness 作为 helpers 即可被各 spec 复用。
- **已考虑 alternative**：独立 `playwright.sse.config.ts` project。**拒绝理由**：与既有 `playwright.contract.config.ts` 双 runner 带来 CI 矩阵复杂度。

### D3：修复卡片 #002 改前端（不改后端）
- **选择**：前端 `useA2AClient` / `message:send` 调用 payload 的 `messageId` → 协议源（A2A 官方 + `@a2a-js/sdk`）规定的 camelCase。
- **理由**：ARBITRATION.md 优先级 (1) A2A 官方 > (2) `@a2a-js/sdk` 类型 > (3) Synthia stable spec；`@a2a-js/sdk` 暴露的 `MessageSendParams.messageId` 是 camelCase。
- **已考虑 alternative**：后端加 snake_case alias 兼容。**拒绝理由**：违反"以协议源为准"原则；后端将永久承担协议源不存在的别名。

### D4：commit 颗粒度 = 每张修复卡片一个 atomic commit
- **选择**：每张卡片完成三同步后单独立 commit，message 形如 `fix(contract): #<卡片> <一句话>`，footer 包含 `Closes #<卡片>` 或引用 `tasks.md` 行号。
- **理由**：cycle #1 retrospective §1.2 已指出 squash 丢失决策过程；atomic commit 让 reviewer 可逐卡 review。
- **已考虑 alternative**：cycle 末 squash。**拒绝理由**：破坏可追溯性。

### D5：§5.3 未覆盖路径段落 = advisory warn（不阻塞）
- **选择**：cycle #2 阶段 `make contract-coverage` 输出"未覆盖路径"段落但 exit code 仍为 0；§6.1 升级后才改 failing exit。
- **理由**：见 brainstorm Q3 — 报告"未覆盖"在 fix card 过程中持续被新卡片本身填满，过程数据没意义；advisory 模式是培养习惯的窗口。
- **已考虑 alternative**：每张 card 跑覆盖率 fail。**拒绝理由**：噪音 + reviewer fatigue。

### D6：§2.3 PR 模板 = 独立小任务，最先做
- **选择**：在修复卡片 #002 之前落地 `.github/PULL_REQUEST_TEMPLATE.md`，包含"修改 contract.yaml 必须同步 contract-closure specs"提示。
- **理由**：见 brainstorm Q5 — 早补避免后续 fix card commit 漏同步。
- **已考虑 alternative**：cycle 末补。**拒绝理由**：窗口期内（即 #002–#009）已经有 PR 在走，模板生效晚一轮。

### D7：promote-candidates 评估 = cycle 末一次性 retrospective
- **选择**：5 个 promote-candidates 不在本 cycle 预先采纳；cycle #2 retrospective 中基于 cycle #2 实际经验评估。
- **理由**：见 brainstorm Q6 — cycle #2 本身就是真实试金石；前置采纳会偏倚判断。
- **已考虑 alternative**：cycle 启动时一并采纳。**拒绝理由**：偏倚风险。

## Risks / Trade-offs

- [Risk] SSE harness 在 Playwright fetch API 下处理 chunked stream 不稳定 → Mitigation：参考 `playwright/` 官方示例（`tests/e2e/network/`），把 SSE 解析走 `ReadableStream.getReader()` 而非 axios/event-source polyfill。
- [Risk] 修复卡片 #003（state 枚举值对齐）可能引发前端 reducer 死循环 → Mitigation：在 spec 中加"枚举值集合完整性"反向用例；状态机非法迁移时 console.error 而非 throw。
- [Risk] 修复卡片 #006（cancel handler 新增）可能引入 server 资源泄漏 → Mitigation：handler 必须持有 task handle 并显式清理；附 server 端单元测试。
- [Risk] 修复卡片 #008（重连/反压）需要前后端对齐 cadence 数值（如 `max_idle_ms`）→ Mitigation：数值由 cycle #2 第一张跑过的 spec 经验值定，contract.yaml 字段显式登记。
- [Risk] 修复卡片 #009（token usage 字段）双侧命名若未先 diff `@a2a-js/sdk` 类型会出现反复返工 → Mitigation：cycle #2 启动时一次性 diff SDK 类型，记录在 `tasks.md` §4.9 行内。
- [Trade-off] 8 张卡片顺序串不能并行 → 接受理由：每张卡片都需前序 harness 基础设施；并行会增加 contract.yaml 冲突概率。
- [Trade-off] §6.1 仍维持 advisory → 接受理由：cycle #2 末评估；过早升 blocking 会把"未覆盖路径"段落作为噪音打回 PR。

## Migration Plan

本 change 不涉及 endpoint / DB / 模型 schema 的运行时变更（#006 唯一新增 cancel handler，单独评审；其他为字段/枚举对齐）。

1. **阶段 A — 准备（T0–T1）**
   - T0：落地 `.github/PULL_REQUEST_TEMPLATE.md`。
   - T1：建 `synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.ts`，含单元测试（用 vitest）。
2. **阶段 B — 修复卡片循环（T2–T9）**
   - 按 #002 → #003 → #004 → #005 → #006 → #007 → #008 → #009 串执行。
   - 每张卡片：读 tasks.md 行 → 定位 contract.yaml entry → 加 scanner 测试 → 改双侧实现 → 补 Playwright 正/反向 spec → atomic commit + 勾 tasks.md。
3. **阶段 C — 报告与评估（T10–T11）**
   - T10：扩展 `contract-coverage.ts` 加"未覆盖路径"段落。
   - T11：cycle #2 收尾 → evaluate promote-candidates → 评估 §6.1 升级时机。

**回滚**：每张卡片 atomic commit 单独 revert。`sse-harness.ts` 不破坏既有 4 spec；`PR 模板` revert 无副作用；`contract-coverage` 报告扩展不影响既有逻辑。

## Open Questions

- 修复卡片 #003 SSE state 枚举值具体集合（`Working` / `Completed` / `Failed` / `Canceled` 之外是否还有 `Input-required` / `Auth-required`）？→ **决议**：以 `@a2a-js/sdk` v0.3 + A2A 官方 v0.2 交集为准；首次跑 spec 时校准。
- 修复卡片 #009 token usage 字段具体命名（`prompt_tokens` vs `promptTokenCount`）？→ **决议**：cycle #2 启动时一次性 diff `@a2a-js/sdk` 类型再决定。
- §6.1 是否在 cycle #2 archive 后单独提议新 change 升级 blocking？→ **决议**：cycle #2 末评估；若 promote 则提议 `synthia-interface-contract-closure-cycle-3-promote-to-blocking`。