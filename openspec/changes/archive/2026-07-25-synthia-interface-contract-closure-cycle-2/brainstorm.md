<!--
Raw capture of brainstorming for cycle #2.
Source material: cycle #1 retrospective (Cycle #2 kick-off §3) + user's explicit
direction "Archive #1 first, then open #2; first card = #002 message:send payload".
-->

# Brainstorm — `synthia-interface-contract-closure-cycle-2`

**日期:** 2026-07-25
**承接:** [archive/2026-07-25-synthia-interface-contract-closure/retrospective.md](../archive/2026-07-25-synthia-interface-contract-closure/retrospective.md) §3
**参与者:** 用户 + Assistant

---

## 0. 起点

> 继续 — Cycle #1 已 archive，用户决策：
> 1. 先 archive #1，再 open #2
> 2. 第一张修复卡片 = #002（`POST /a2a/message:send` payload `messageId` vs `message_id`）
> 3. 5 个 promote-candidates 在下个 cycle retrospective 评估

Cycle #1 完成：scanners（backend + frontend + unifier）、`docs/interface-contract/` 双视图、Playwright 4 specs、CI advisory、修复卡片 #001 闭环。

## 1. 项目上下文（继承自 cycle #1）

- `contract.yaml` 现有 37 endpoints（6 paired, 31 backend-only advisory, 0 frontend-only）。
- 5 promote-candidates 待评估：per-normalisation unit tests, fixture-before-parser ordering, state-machine parsing, contract-coverage advisory semantics, A2A SDK type-checkpoints。
- CI workflow `contract-closure.yml` 4 处 `continue-on-error: true`，等待习惯建立后升级 blocking。

## 2. Cycle #2 决议链

### Q1: 修复卡片顺序？
- 选 **#002 → #003 → #004 → #005 → #006 → #007 → #008 → #009** 顺序串。
- 理由：#002–#005 字段层（HTTP request/response），#006–#007 错误路径层，#008 SSE 重连，#009 流式用量。难度递增。
- **拒绝：随机顺序**（依赖链混乱）和 **倒序**（§5.2 SSE harness 还没建就跑 #008/009）。

### Q2: §5.2 SSE 完整事件序列何时做？
- 选 **先建 Playwright SSE harness，再做 #003/#004/#008**。
- 理由：三个 fix card 都改 SSE 字段，没 harness 写不了正向/反向用例。
- **拒绝：每个 card 自带 ad-hoc harness**（重复代码、风格不一致）。

### Q3: §5.3 contract-coverage "未覆盖路径"段落何时做？
- 选 **在 #002–#009 全部闭环后做**。
- 理由：报告"未覆盖"会持续被新修复卡片本身填满，过程数据没意义；终点数据才有意义。
- **拒绝：每张 card 跑一次覆盖率**（噪音 + reviewer fatigue）。

### Q4: §6.1 CI advisory → blocking 何时做？
- 选 **#002–#009 全部闭环 + §5.3 报告稳定为空（或只剩故意延后）后做**。
- 理由：cycle #1 retrospective 已明确 deliberate 推迟；本 cycle 是建立 contract-driven 习惯的窗口。
- **拒绝：现在就升 blocking**（习惯未建立，advisory 是 noise）。

### Q5: §2.3 PR 模板何时补？
- 选 **作为 cycle #2 第一个独立小任务**（早于 #002）。
- 理由：PR 模板是低风险高收益的提醒机制，先补可避免后续 fix card commit 漏掉 contract-table 同步。

### Q6: 5 个 promote-candidates 何时评估？
- 选 **cycle #2 结束时一并 retrospective 评估**（不预先采纳）。
- 理由：cycle #2 本身就是 5 个 candidate 的真实试金石；前置采纳会偏倚判断。

## 3. 设计取捨

| 取捨 | 选项 A | 选项 B | 选择 | 理由 |
|------|--------|--------|------|------|
| SSE harness 形态 | `helpers/sse-harness.ts`（纯 helper） | 独立 `playwright.sse.config.ts` | **A** | Playwright 已用一个 project，再开新 project 维护成本高；harness 作 helpers/ 即可 |
| 修复卡片 #002 改哪边 | 后端（加 snake_case alias） | 前端（改 camelCase） | **前端** | ARBITRATION.md 优先级 A2A 官方 > `@a2a-js/sdk`；`@a2a-js/sdk` 用 camelCase |
| commit 颗粒度 | 每张卡片一个 atomic commit | 一个 fix 一次性 squash | **atomic commit** | retrospective §1.2 已指 squash 丢失决策过程 |
| contract-coverage 报告触发 | 每张 card 后跑 | cycle 末跑 | **cycle 末** | 见 Q3 |

## 4. 待办（已写入 tasks.md）

1. **T0**：补 `.github/PULL_REQUEST_TEMPLATE.md`（§2.3）
2. **T1**：建 Playwright SSE harness（`synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.ts`）
3. **T2**：修复卡片 #002（message:send payload）
4. **T3–T9**：#003–#009 顺序串
5. **T10**：§5.3 contract-coverage 报告"未覆盖路径"段落
6. **T11**：cycle #2 收尾 → evaluate promote-candidates → 决定是否升 blocking