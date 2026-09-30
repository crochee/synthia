# Retrospective — `synthia-interface-contract-closure`

**日期:** 2026-07-25
**范围:** Cycle #1 → Cycle #2 复用模板

---

## 1. Cycle #1 收获

### 1.1 做得好的

1. **AST + regex 混合扫描**：用 TypeScript Compiler API 处理 TS，用 paren-balanced state machine 处理 axum `Router::route(...).nest("/api", X)`，落地成本低、精度足够。
2. **Unifier 用 canonical 占位符 `{key}` 归一化**：把后端 `{id}` / 前端 `${encodeURIComponent(expr)}` 视为同一槽位，让"双侧"判定机器可执行。
3. **ARBITRATION.md 前置**：每个修复卡片都能引用（A2A 官方 > `@a2a-js/sdk` > Synthia stable spec），减少口头协商。
4. **Playwright 独立子项目**：`playwright.contract.config.ts` + 独立 `testDir`，不和历史三层混在一起，CI 报告粒度清晰。
5. **CI advisory 起步**：先 `continue-on-error: true` 暴露问题，等习惯建立再升级 blocking，避免首次上线就被打回。

### 1.2 做得不好的

1. **commit 一次性 squash**：28 commit 收成 1 个 `feat(contract-closure)` + 1 个 `chore(regenerate)`，丢失中间决策过程；reviewer 难以追因。
2. **tasks.md 没有逐 commit 勾选**：直到本 verify.md 落地才补 `[x]`；中间状态对外部 reviewer 不可见。
3. **PR 模板（§2.3）漏掉**：合约表变更与 spec 同步的提示没有写到 PR 模板，依赖 reviewer 自觉。
4. **fix card #001 的 commit message 没有明确 `Closes #001`**：要把 tasks.md 行号或 fix-card id 写进 commit footer 才能机器勾选。
5. **scanner 的 fixture 覆盖**：frontend-scanner 反复在归一化栽跟头（路径编码占位符），需要更多 per-normalisation 单元测试（已列入 promote-candidate #1）。

### 1.3 待办（已沉淀到 promote-candidates）

1. **Per-normalisation unit tests**（frontend-scanner 调试反复栽）— 卡片归一化的 fixture 矩阵覆盖 `{key}` / `{id}` / `${encodeURIComponent(expr)}` / 裸字面量。
2. **Fixture-before-parser ordering** — 测试套件应先有"已知 fixture 输入 → 期望输出"表，再写 parser；当前 scanner 是 parser-first，retro 时补 fixture。
3. **State-machine parsing for AST-style scanners** — 把 axum 的 `Router::route(...).nest(...)` 升级为完整 state machine（当前是手写 paren-balance，对嵌套 nest 的鲁棒性欠佳）。
4. **`make contract-coverage` advisory semantics** — 让覆盖率报告在 CI 中以"建议补充 spec"而非"失败"出现，给前端团队补 spec 时间。
5. **A2A SDK type-checkpoints before each protocol-touching fix-card** — 每张 fix card 开工前先 diff `@a2a-js/sdk` 版本类型，确保修复方向跟协议源一致；本次 cycle 缺这个前置检查。

---

## 2. 双层校验模板（供后续上线工程复用）

> "Contract-Scan + Playwright 双层契约闭环" — 用于任何"前后端字段/枚举/事件节奏容易漂移"的模块。

### 2.1 模板骨架（4 个产物）

| 产物 | 路径 | 作用 |
|------|------|------|
| 契约表主源 | `docs/interface-contract/contract.{yaml,md,json}` | 机器 + 人类双视图；JSON 是双视图之间的桥 |
| 仲裁文档 | `docs/interface-contract/ARBITRATION.md` | 协议源优先级，避免反复决策 |
| Schema 文档 | `docs/interface-contract/SCHEMA.md` | 契约表字段说明（哪行必填、哪行可选） |
| 扫描器 | `contract-closure/{contract-scan,contract-check,contract-coverage,contract-report}.ts` | 双侧 AST/正则 → 契约表；并校验无悬空 |

### 2.2 修复卡片三同步流程（cycle 内）

```
1. 阅读 tasks.md 修复卡片（引 ARBITRATION + 期望行为）
2. 定位 contract.yaml 中对应 entry，记录不一致点
3. 在 contract-closure/test/ 加 "修复后这个 entry 应消失 dangling" 用例 → 红
4. 修改后端或前端（按 ARBITRATION 优先级定向）
5. 在 tests/e2e/integration/contract-closure/ 补 spec（≥1 正向 + ≥1 反向）
6. make contract-check && make test-contract-closure 全绿
7. atomic commit: fix(contract): #<卡片编号> <一句话>
8. 勾 tasks.md §4.x；继续下一张
```

### 2.3 升级到 blocking 的时机

- 团队已跑 ≥ 2 个 cycle 的 contract-driven 习惯（修复卡片闭环 + 三同步习惯化）。
- `contract-coverage` 报告"未覆盖路径"段落稳定为空（或只剩故意延后的子集）。
- CI advisory 日志被实际消费（PR review 中有人引用），不是噪音。

### 2.4 不要做的（红线）

- 不要把契约表写在代码里（不要造 `contract-table.ts`）；YAML 主源必须独立 git diff。
- 不要把 SSE 事件从 HTTP 端点拆成两个契约表文件；保持原子化登记。
- 不要让 fix card 跳过 Playwright 正/反向用例；否则下个 cycle 会出现"修复卡片 × 行为守不住" 的事故。
- 不要在 CI blocking 之前就关闭"未覆盖路径"的容忍；advisory 期是建立覆盖率的窗口。

---

## 3. 给 Cycle #2 的具体动作（next cycle kick-off）

按用户决策，Cycle #2 启动顺序：

1. **本次 archive**（本文件落地后即触发）。
2. **新 change 创建**：`openspec/changes/synthia-interface-contract-closure-cycle-2/proposal.md`，聚焦：
   - 修复卡片 #002（`message:send` payload `messageId` vs `message_id`）— 协议源优先改前端。
   - 修复卡片 #003–#009（按依赖顺序串）。
   - §5.2 SSE 完整事件序列（先建 Playwright SSE harness）。
3. **§6.1 升级时机**：在 #002–#009 全部闭环且 `contract-coverage` 报告"未覆盖路径"段落稳定时再做。
4. **§2.3 PR 模板补漏**：作为 Cycle #2 的独立小任务。

## 4. 一句话总结

> **契约 = 显性资产，行为 = 显性回归，决策 = 显性优先级。** 这次 cycle 把三者从口头协调升级为 git-tracked artifacts；下一 cycle 把 artifact 数量从 1 个修复卡片扩到 9 个，并引入 SSE harness 守流式事件。