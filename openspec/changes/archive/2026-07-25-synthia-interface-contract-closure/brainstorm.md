<!--
Raw capture of the brainstorming session that produced this change.
保留完整的探索 → 澄清 → 方案对比 → 设计过程。该文件作为原始决策日志,
设计层面的提炼重组产物见 design.md。
-->

# Brainstorm — `synthia-interface-contract-closure`

日期: 2026-07-25
参与者: 用户 + Assistant
相关项目: Synthia (Rust AI Agent framework + React/Vite frontend)

---

## 0. 起点 — 原始请求

> 执行前后端系统的全面联调工作，具体包括以下任务：
> 1. 实施端到端（e2e）测试，构建完整的用户场景测试用例，验证从前端界面操作到后端数据处理的全流程正确性；
> 2. 开展用户界面（UI）测试，检查界面元素渲染、交互响应、跨浏览器兼容性及响应式布局表现。
> 对前后端数据接口及业务逻辑进行系统性优化和修正，确保数据传输格式准确无误、接口调用响应时间符合性能指标（建议平均响应时间<300ms）、业务流程符合需求规格说明书定义。
> 需全面验证所有功能模块的集成效果，包括但不限于用户认证、数据查询、业务操作等核心流程，详细记录联调过程中发现的各类问题（包括功能缺陷、性能瓶颈、兼容性问题等）并进行修复。
> 最终目标是提升系统整体性能（包括页面加载速度、接口吞吐量）和用户体验（包括操作流畅度、界面友好性），确保系统达到上线标准。

---

## 1. 项目上下文探索

通过 `LS` / `RunCommand` 摸清项目状态：

- **架构**: Rust workspace, 17 个 crate。重点：`synthia-server`（axum, A2A 协议） + `synthia-web`（React + Vite, 通过 `@a2a-js/sdk` 通信）。
- **Make 入口**: `make dev` 一键启 `synthia-server (:8080)` + `synthia-web (:5173)`。已有 `Dockerfile.server` / `Dockerfile.web` / `docker-compose.yml`。
- **测试栈**: 已有 Playwright 三层结构（`synthia-web/tests/e2e/`），`make test-e2e` 是入口。
- **最近活跃 changes**:
  - `2026-07-24-synthia-fullstack-integration`（**archived**，做过设计系统/CORS/A2A 客户端/Makefile/E2E 框架）
  - `2026-07-25-simplify-agent-event-stream`（活跃，agent event 流化）
  - `synthia-end-to-end-wiring`（活跃，server v2 wiring）
- **现存 OpenSpec specs**: 146 个 cap，覆盖 A2A 客户端、Agent Bus、Tool 路由、Session v2、Hook、OTel 等。

---

## 2. 范围评估（必须先分解）

原始请求**不是一个任务而是一个完整上线工程**，包含至少 6 个独立子系统：

1. e2e 测试体系
2. UI 测试（视觉 + 交互 + a11y）
3. 跨浏览器兼容 + 响应式
4. 性能调优（接口 < 300ms、页面加载）
5. 前后端接口契约闭环
6. 上线标准合规

若一个 spec 内全部处理 → 范围爆炸 + 验收不可量化。按 brainstorming skill HARD-GATE：

> "在单独 spec 中处理多个独立子系统会浪费工作量，**先把项目分解**。"

---

## 3. 澄清链路（决策链 Q1–Q7）

### Q1. 范围选择 — 哪一个子项目先做？

候选:
- A. E2E/UI 测试覆盖
- B. 前后端接口缺陷闭环
- C. 性能调优
- D. 跨浏览器 / 响应式

→ **选定 B**（前后端接口缺陷闭环）。其余三项作为 backlog 由后续 spec 接手。
→ 理由：联调工作的核心是把契约对齐；测试与优化都在错对齐之上会放大问题。

### Q2. 完成判据

候选:
- 全部接口 100% 贯通零差异
- 仅核心 5–7 条用户旅程
- 仅修正 schema 不动业务
- 仅适配现有差异

→ **选定「全部接口 100% 贯通零差异」**。
→ 理由：用户预期"上线前清零"。

### Q3. 接口清单来源

候选:
- 以前端调用点推导
- 以后端路由表推导
- **双侧并集交叉核对（★）**
- 仅限 A2A 协议面

→ **选定「双侧并集交叉核对」**。
→ 理由：两面兼顾，最严格，避免漏掉"后端有前端不知"或"前端调用后端未实现"的隐式差异。

### Q4. 冲突处理原则

候选:
- 以官方协议源为先
- 以后端实际能力为准
- 智能决策 + 用户确认
- 全部同步设计

→ **选定「以官方协议源为先」**。
→ 理由：相对于单方实现，协议源（`@a2a-js/sdk`、A2A 官方 spec、Synthia 既有 replay 模型）最稳定。

### Q5. 产出与追踪形式

候选:
- OPENSPEC 变更 + Can-be 报告（★）
- 脚本驱动的契约校验报告
- 代码 + commit 记录

→ **选定「OPENSPEC 变更 + Can-be 报告」**。
→ 理由：可追溯 + 准上线场景刚需。

### Q6. 验证闸门

候选:
- 仓内契约校验脚本（contract + JSON Schema）
- **Playwright 集成联调用例（★）**
- ad-hoc 命令行（curl + jq）
- telemetry 交叉验证

→ **选定「Playwright 集成联调用例」**。
→ 理由：复用既有 Playwright 三层结构；产出本身就是上线回归网。

### Q7. 执行策略（方案对比）

| 方案 | 做法 | 优点 | 缺点 | 推荐度 |
|---|---|---|---|---|
| 1 | 静态合约清单 + 逐项修复 | 范围明确、进度可视化、每条可独立验收 | 静态扫描本身要先写体力活 | ★★★ |
| 2 | Playwright 端到端驱动 | 复用 Playwright、产出即回归网 | 难做到 100% 接口覆盖（UI 不一定触发所有接口） | ★★ |
| **3（★）** | **1 + 2 双层** | 静态扫描 = 覆盖率图，Playwright = 行为层 | 工作量翻倍但可控 | ★★★★★ |

→ **选定方案 3：静态合约表 + Playwright 双层**。
→ 理由：静态表保证 100% 接口覆盖，Playwright 保证行为验证 — 互补。

---

## 4. 设计意图汇总（将在 design.md 重组）

- **范围**: 仅本子项目（前后端接口缺陷闭环）。UI/性能/兼容等独立子项目以 backlog 列出。
- **数据源**: 双侧并集（前端 api/** + 后端 router.rs）。
- **冲突仲裁**: 以官方协议源为先（A2A 官方 / `@a2a-js/sdk` / Synthia 既有 replay 模型）。
- **执行方案**: 静态合约表 + Playwright 集成联调双层。
- **产出物**: 本 openspec change（proposal/design/tasks/plan/verify/retrospective）+ 双侧契约并集表（Markdown + CI 可读）+ Playwright e2e 用例覆盖矩阵。
- **完成判据**: 静态契约表 100% 行双侧对齐；Playwright 全部用例绿；CI 接入并跑通。
- **验证闸门**: Playwright 全绿（CI 必跑）+ 手动 review 静态表无冲突项。

---

## 5. 留待本次未决议项（移出本次 spec）

| 项 | 备注 | 推荐后续 spec 名 |
|---|---|---|
| E2E/UI 测试体系深化（视觉、交互、a11y） | 测用例以外的质量维度 | `e2e-ui-test-system` |
| 性能调优（接口 < 300ms / 页面加载） | 需要采集 baseline | `perf-tuning-launch-readiness` |
| 跨浏览器 / 响应式 | 需明确目标浏览器清单 + 关键视口 | `cross-browser-responsive` |
| 安全审计 | 含 SPEC 之外的安全要求 | `security-audit-launch-gate` |
| 业务逻辑重构 | 不在本次范围 | （待提名） |

---

## 6. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 双侧扫描失败（AST/正则不稳定） | 静态表不准 | 同时输出一份人工可读表，使用结构化 markdown；CI 校验仅校验"是否生成" |
| Playwright 用例与已有套件冲突 | CI 抖动 | 新增 specs 隔离在 `tests/e2e/integration/contract-closure.*`，不破坏既有 |
| 修改后端路由表导致旧 e2e 失败 | 回归 | 修复前先跑 baseline，列入 `tasks.md` 的 regression-gate 任务 |
| SSE 流式契约微调导致 UI 长连接断 | 体验回归 | 任何 SSE 字段调整必须配 Playwright 流式用例与 OTel span 截图回归 |
| 协议源 `@a2a-js/sdk` 版本漂移 | 不可控 | 在 plan 中锁版本 + 提交 `package-lock.json`；超出锁版本时新建 spec |

---

## 7. 决策摘要（一句话）

> 以"双侧接口契约并集表 + Playwright 集成联调用例"双层落地前后端接口
> 100% 一致；冲突以官方协议源（A2A / `@a2a-js/sdk` / Synthia replay）为准；
> 完成判据 = 静态表 100% 覆盖且无冲突 + Playwright 全绿 + CI 接入。
