# Verify — `synthia-interface-contract-closure`

**日期:** 2026-07-25
**范围:** Cycle #1（基线扫描器 + 契约表 + Playwright 骨架 + 修复卡片 #001）

---

## 1. 指标复盘（按 tasks.md §6.2）

| 指标 | 目标 | 实际 | 备注 |
|------|------|------|------|
| (a) 契约表行数 | ≥ 双侧接口并集 | **37 endpoints** | `docs/interface-contract/contract.yaml`；其中 backend-only / frontend-only / both 均已标记 |
| (b) 修复卡片总数 | 按需发现 | **9 张已登记，1 张已闭环（#001）**，8 张（#002–#009）留待下一 cycle | 见 `tasks.md` §4 |
| (c) Playwright 用例数 | ≥ 首批 3 用例 | **4 用例** | `health.spec.ts` / `agent-card.spec.ts` / `models-list.spec.ts` / `tasks-list.spec.ts` |
| (d) 仲裁源引用次数 | ≥ 3 处 | **11 处** | `docs/interface-contract/*` + `contract-closure/*` + `synthia-web/tests/e2e/integration/contract-closure/*` |

**额外指标：**
- `contract-closure/` 单元测试（vitest）覆盖 3 个模块：backend-scanner、frontend-scanner、unifier。
- 双侧悬空检测 (`contract-check`) 与覆盖率报告 (`contract-coverage`) 落地。
- CI workflow `.github/workflows/contract-closure.yml` 以 advisory 模式就位（4 处 `continue-on-error: true`）。

## 2. 验证命令（可重跑）

```bash
# 1. 扫描双侧契约并集
make contract-scan

# 2. 校验悬空（CI advisory）
make contract-check

# 3. 人类可读报告
make contract-report

# 4. Playwright 契约集覆盖
make contract-coverage

# 5. scanner 单元测试
make test-contract-closure

# 6. Playwright e2e（假定 synthia-server 在 :8080）
make test-contract-closure-playwright
```

## 3. 已闭环（§4 修复卡片）

- **#001** `GET /.well-known/agent-card.json` — 前端 `TasksPage.tsx` 字段读取与后端响应命名对齐；契约表 `agent-card` 行已登记；`contract-closure.agent-card.spec.ts` 守住正向。

## 4. 推迟到下一 cycle（§4.2–4.9 + §5.2 + §5.3 + §6.1）

详细列表见 `tasks.md`，本文件不重复。**核心推迟原因**：

- **修复卡片 #002–#009**（8 张）：每张都需要"前后端 + 契约表 + Playwright 正/反向用例"三同步；属于下一 cycle 的主轴。
- **§5.2 SSE 完整事件序列**：缺乏 Playwright SSE harness，需要先建脚手架。
- **§5.3 contract-coverage 报告格式化**："未覆盖路径"段落需要 CI 升级到 blocking 后才能体现完整价值。
- **§6.1 CI advisory → blocking**：deliberate 推迟，让团队先建立 contract-driven 习惯，再做闸门升级。

## 5. 已知的"未完成小尾巴"

- `tasks.md` §2.3 PR 模板未落地（缺 `.github/PULL_REQUEST_TEMPLATE.md`）。
- 修复卡片 #001 的 commit `b693fa8` 没有逐 commit 拆出（一次性 squash）；后续卡片按 plan §Task 5 的"atomic commit"严格执行。
- `contract.yaml` `generated_at` 字段使用 ISO 字符串但 schema 文档未声明此字段是必填还是衍生。

## 6. 复跑证据

```
$ git log --oneline -3
5f48122 chore(contract-closure): regenerate contract table against master paths
b693fa8 feat(contract-closure): dual-side scanner + Playwright contract suite + 1 fix
ada2a68 feat: as
```

`b693fa8` 携带全部 Cycle #1 实现；`5f48122` 用最新 master 重新生成契约表（确认可复跑）。