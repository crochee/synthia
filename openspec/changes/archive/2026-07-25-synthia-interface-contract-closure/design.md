## Context

Synthia（Rust `synthia-server` + React `synthia-web`）已就位 A2A 协议接入、`@a2a-js/sdk` 客户端、CORS、Makefile 驱动的开发与 CI、Docker 化、Playwright 三层 e2e（见 archived `2026-07-24-synthia-fullstack-integration`）。当前主要空白：

- 接口契约**没有机器可读的双侧并集表**，对齐靠口头/PR review 反复发生。
- 任何后端字段重命名或前端新增调用点都缺少覆盖率校验。
- Playwright 三层用例虽在，但与 server 端契约的耦合未量化（缺覆盖率图）。
- 冲突仲裁规则（哪个名字/枚举值优先）没有沉淀文档，修改方向不定。

主要 stakeholder：开发工程师（要维护 `tasks.md` 修复卡片）、CI（要把契约扫描 + Playwright 契约集做成必跑步骤）、发布负责人（要把"全部接口 100% 一致"列为上线硬判据）。

## Goals / Non-Goals

**Goals:**
- 产出一份 `docs/interface-contract/contract.yaml` + 同源 `.md`，覆盖 Synthia 前后端**全部**交互路径（HTTP/REST + A2A JSON-RPC + SSE 事件）。
- 把"以协议源为准"的仲裁规则文档化、引用前置到每个修复卡片。
- 在 `synthia-web/tests/e2e/integration/contract-closure.*.spec.ts` 提供契约表 100% 覆盖的 Playwright 集成测试。
- 把扫描 + Playwright 契约集接入 CI。

**Non-Goals:**
- UI 测试（视觉/交互/a11y） — 留待后续 spec `e2e-ui-test-system`。
- 性能调优（接口 < 300ms / 页面加载） — 留待后续 spec `perf-tuning-launch-readiness`。
- 跨浏览器 / 响应式 — 留待后续 spec `cross-browser-responsive`。
- 安全审计 — 留待后续 spec `security-audit-launch-gate`。
- 业务逻辑重构、模型路由改造、新增能力 — 本 spec 严格只做契约对齐。
- 修改既有 capability 的 REQUIREMENT 文本（避免污染 archive apply）。

## Decisions

### D1：契约表存储格式 = YAML 主源 + Markdown 衍生视图
- **选择**：`.yaml` 是 single source of truth（机器可读、CI 校验、Playwright 解析）；`.md` 由 `.yaml` 生成，仅供人类 review。
- **理由**：双格式覆盖两类受众（CI/脚本 vs 人）。`.yaml` 直接在 git 中 diff 友好。
- **已考虑 alternative**：纯 Markdown（机器解析差）；TS 类型中央化（侵入业务）；TypeBox/Zod schema（双语言维护负担）。**拒绝理由**：维护成本与本次范围不匹配。

### D2：契约扫描 = TS 双侧 + Rust 一侧的混合策略
- **选择**：`scripts/contract-scan.ts` 用 TypeScript Compiler API（前端）+ 自定义枚举（后端 router）扫描；或用 ripgrep + 结构化提取。
- **理由**：Synthia 前端是大型 TS 表面，TS Compiler API 提供精确 AST；后端 Rust 路由表小，用 ripgrep 正则扫 `Route::on(method, path)` 即可。混合策略在精度与维护成本间平衡。
- **已考虑 alternative**：纯 AST（后端要造完整 Rust syn tree，过重）；手动维护清单（不可持续）。**拒绝理由**：分别高估工程量、低估维护成本。

### D3：SSE 事件契约登记在 YAML 中以 `events: [{name, fields, cadence}]` 表达
- **选择**：每个 A2A SSE 流端点对应一行，SSE 事件清单作为子节点。
- **理由**：A2A 协议本身 SSE 模型是离散事件；YAML 嵌套能完整表达。
- **已考虑 alternative**：把事件和 HTTP 端点分两个文件（导致双侧关联丢失）。**拒绝理由**：本次目标是"双侧 0 差异"，必须把端点和它的流保持原子化登记。

### D4：Playwright 契约集专门放 `tests/e2e/integration/`，不混进既有 specs
- **选择**：本次新增的契约 spec 全部位于 `tests/e2e/integration/contract-closure.*.spec.ts`。
- **理由**：archived `fullstack-integration` 已经定义了"三层"（UI/联调/Agent）。把"全接口契约"作为新一层放在 `integration/`，不和历史层冲突。
- **已考虑 alternative**：用 `describe.serial` 在既有文件中混加（破坏历史层语义）。**拒绝理由**：CI 报告与人类 review 都会更乱。

### D5：CI 闸门 = `make contract-check && make test-contract-closure`
- **选择**：在 CI 中先跑 `make contract-check`（契约表双侧一致闸门），再跑 `make test-contract-closure`（Playwright 契约集）。
- **理由**：先静态覆盖、再行为验证。任一不过均阻塞 PR。
- **已考虑 alternative**：把契约集直接进 `make test-e2e`（会和既有 agent 三层用例混淆）。**拒绝理由**：失败信号粒度不足。

### D6：冲突仲裁源优先级
- **选择**：A2A 官方 spec > `@a2a-js/sdk` 类型 > Synthia 既有 stable spec（`event-v2-system` / `session-replay-harness`）。
- **理由**：协议源比单方实现稳定；本地 spec 是最后兜底。
- **已考虑 alternative**：直接后端为准（与提案"以协议源为先"冲突）；优先时间靠后者（无意义）。**拒绝理由**：违反 brainstorm Q4 决策。

## Risks / Trade-offs

- [Risk] 双侧扫描在不同 Rust 版本/TS 版本下输出漂移 → Mitigation：CI 锁定版本（`rust-toolchain.toml` + `package-lock.json`）；扫描脚本本身加单测。
- [Risk] Playwright 契约集与历史 e2e 三层用例互相绊倒 → Mitigation：本套件隔离 `integration/`，独立 Playwright 项目（`playwright.config.ts` 用 `testDir: 'tests/e2e/integration'` 跑子集）。
- [Risk] SSE 字段修改导致前端断连 → Mitigation：所有 SSE 修改必须带正向 + 反向 Playwright 用例，且保留原字段作为 deprecated alias 一个 minor 版。
- [Risk] `@a2a-js/sdk` 上游版本漂移 → Mitigation：`package-lock.json` 锁版本；超出锁需新 spec 处理。
- [Trade-off] 双层（扫描 + Playwright）的维护工作量 ≈ 1.5× — 这是 brainstorm Q7 方案 3 的固有代价；接受理由：覆盖率+行为层互补，单层都会留盲区。

## Migration Plan

本 change 不涉及 endpoint / DB / 模型 schema 的运行时变更，仅契约对齐与新增测试。**没有"功能上线"动作，只有契约表与 CI 接入**：

1. **阶段 A — 扫描基线（无功能性变更）**
   - 落地 `docs/interface-contract/contract.yaml` 第一版（手工+半自动）。
   - 落地 `make contract-scan` + `make contract-check`。
   - CI 新增 `contract-check` 步骤，禁用为 advisory（首次仅报告）。
2. **阶段 B — Playwright 契约集基线**
   - 新增 `tests/e2e/integration/contract-closure.*.spec.ts` 框架（空壳）。
   - CI 新增 `test-contract-closure` 步骤，advisory。
3. **阶段 C — 修复闭环**
   - 逐条修复卡片驱动契约表 + 双侧实现 + Playwright 用例。
   - 每个修复卡片自带 verification step。
   - 完成后 CI 升级为 blocking。
4. **阶段 D — 收尾**
   - 在 `verify.md` 上签字；在 `retrospective.md` 沉淀"双层校验"模板供后续 spec 复用。

**回滚**：契约表文档与脚本可单独 revert（不破坏运行时）。Playwright 契约集同理。任何生产路径不变更，因此无"部署回滚"。

## Open Questions

- 是否把 `a2a-protocol-client` / `v2-session-api` 上游 spec 的小字段命名在本次顺手修一次？→ **决议**：不动既有 spec REQUIREMENT（避免污染 archive），仅在 contract.yaml 注释中标注"上游需修正的边界"。
- 第一次跑扫描时是否对后端 route 走 syn-based 全 AST？→ **决议**：先用 ripgrep 正则；后续视修复卡片数量决定要不要升级。
- Playwright 契约集是否覆盖 SSE 失败重连？→ **决议**：纳入。新增"反压/重连"spec 子节点（SSE 用例 v2）。
- 是否在 CI 中对契约表做 schema 校验（YAML 结构本身）？→ **决议**：纳入，避免坏 YAML 误判全部"已覆盖"。
