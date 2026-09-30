# `synthia-interface-contract-closure` Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development (or
> any suitable implementation driver) to execute this plan step-by-step.

**Goal:** 落地"双侧契约并集表 + Playwright 契约集"双层校验，把 Synthia 前后端接口 100% 收敛为一致；任何不一致都通过 `tasks.md` 的修复卡片走三同步（实现 / 契约表 / Playwright 用例）。

**Architecture:** 在仓库根新增 `scripts/` 扫描 + 报告 + 校验三件套；`docs/interface-contract/` 维护 YAML 主源 + Markdown 衍生视图 + ARBITRATION 文档；前端 `tests/e2e/integration/contract-closure/` 落地 Playwright 子套件；CI（`.github/workflows/`）以 advisory→blocking 两阶段接入。

**Tech Stack:** TypeScript（`scripts/`, Playwright config）, Rust（仅在修复卡片中触发局部改动）, Makefile, GitHub Actions.

---

## Task 1: 落地契约扫描器骨架

**Files:**
- `scripts/contract-scan.ts` (NEW)
- `scripts/contract-check.ts` (NEW)
- `scripts/contract-report.ts` (NEW)
- `scripts/contract-scan.test.ts` (NEW)
- `scripts/contract-check.test.ts` (NEW)
- `scripts/package.json` (NEW — minimal lock-step env)
- `tsconfig.json` (NEW — strict + moduleResolution Node)

- [ ] **Step 1:** Create `scripts/` directory at repo root with empty `package.json` (`type: "module"`, dev deps: `typescript`, `vitest`, `playwright`, `yaml`, `@types/node`).
- [ ] **Step 2:** Add `tsconfig.json` strict with `moduleResolution: "bundler"`, target `ES2022`.
- [ ] **Step 3:** Implement `scripts/lib/backend-router-scanner.ts` — reads `crates/synthia-server/src/router.rs`, extracts `Route::on(METHOD, "/path", ...)` lines via reg-exp (write a 12-line parse fn with 3 unit cases).
- [ ] **Step 4:** Implement `scripts/lib/frontend-calls-scanner.ts` — walks `synthia-web/src/{api,hooks,pages}/**/*.{ts,tsx}`, scans `fetch(`, `client.send(`, `a2a.sendMessage(`, `a2a.subscribe(` for method/path/method-name.
- [ ] **Step 5:** Wire both scanners in `scripts/contract-scan.ts` producing `contract.yaml` with schema version `1`.
- [ ] **Step 6:** Write vitest tests: (a) `routeScanner.test.ts` on a 5-line fixture `router.rs`; (b) `frontendScanner.test.ts` on 5 fixture `.ts` files; (c) `unionDiff.test.ts` enumerating per-path status (`both-sides` / `frontend-only` / `backend-only`).
- [ ] **Step 7:** `pnpm -C scripts test` must pass. Commit: `feat(contract-scan): add backend/frontend scanners + union diff with tests`.

---

## Task 2: 落地契约表与 ARBITRATION 文档

**Files:**
- `docs/interface-contract/contract.yaml` (NEW)
- `docs/interface-contract/contract.md` (NEW)
- `docs/interface-contract/ARBITRATION.md` (NEW)
- `docs/interface-contract/SCHEMA.md` (NEW)
- `scripts/contract-report.ts` (extends Task 1)

- [ ] **Step 1:** Hand-author `contract.yaml` v1 by listing known backend routes from `crates/synthia-server/src/router.rs` walk-through AND every frontend call point in `synthia-web/src/api/`. Each path entry has: `id`, `method`, `path`, `request`, `response`, `sse_events?: [{name, fields, cadence}]`, `source_files: {frontend, backend}`.
- [ ] **Step 2:** Implement `scripts/contract-report.ts` to render `contract.md` from YAML (grouped by HTTP method, sorted alphabetically).
- [ ] **Step 3:** Author `docs/interface-contract/ARBITRATION.md`: priority A2A-official > `@a2a-js/sdk` types > `event-v2-system` / `session-replay-harness` stable specs.
- [ ] **Step 4:** Add `docs/interface-contract/SCHEMA.md` with JSON-schema for `contract.yaml` (mandatory fields per entry).
- [ ] **Step 5:** `make contract-scan && make contract-report` regenerates files; verify diff is empty. Commit: `docs(contract): bootstrap contract.yaml + ARBITRATION + SCHEMA`.

---

## Task 3: Makefile + CI (advisory)

**Files:**
- `Makefile` (extend)
- `.github/workflows/contract-closure.yml` (NEW)

- [ ] **Step 1:** Add `Makefile` targets: `contract-scan`, `contract-check`, `contract-report`, `contract-coverage`, `test-contract-closure`. All gate on `pnpm -C scripts` being installed.
- [ ] **Step 2:** Update `make help` to include new targets.
- [ ] **Step 3:** Create `.github/workflows/contract-closure.yml` with two jobs: `contract-check` and `test-contract-closure`; both `continue-on-error: true` initially (advisory).
- [ ] **Step 4:** Add `.github/PULL_REQUEST_TEMPLATE.md` note: "If you modified `docs/interface-contract/contract.yaml`, you MUST also modify the matching `contract-closure.*.spec.ts`." (PR body check via `paths` filter).
- [ ] **Step 5:** Commit: `ci(contract-closure): advisory gates for scan + Playwright contract set`.

---

## Task 4: Playwright 契约集子项目骨架

**Files:**
- `synthia-web/playwright.contract.config.ts` (NEW)
- `synthia-web/tests/e2e/integration/contract-closure/_fixtures/` (NEW)
- `synthia-web/tests/e2e/integration/contract-closure/_helpers/list-endpoints-from-yaml.ts` (NEW)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.health.spec.ts` (NEW)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.agent-card.spec.ts` (NEW)
- `synthia-web/tests/e2e/integration/contract-closure/contract-closure.session-list.spec.ts` (NEW)

- [ ] **Step 1:** `playwright.contract.config.ts` with `testDir: 'tests/e2e/integration/contract-closure'`, `reporter: [['list'], ['html', {outputFolder: 'test-results/contract-closure'}]]`.
- [ ] **Step 2:** Implement `_helpers/list-endpoints-from-yaml.ts` reading `docs/interface-contract/contract.yaml`; exports typed `Endpoint[]`.
- [ ] **Step 3:** Add `_fixtures/server.ts` to spawn `cargo run -p synthia-server --release` as background child, wait for `/health` 200, return baseURL.
- [ ] **Step 4:** `contract-closure.health.spec.ts` asserts `GET /health` returns `{status: "ok"}` schema.
- [ ] **Step 5:** `contract-closure.agent-card.spec.ts` asserts `GET /.well-known/agent-card.json` fields per contract YAML.
- [ ] **Step 6:** `contract-closure.session-list.spec.ts` asserts `GET /api/v2/sessions` returns array with `parent_id` field.
- [ ] **Step 7:** `pnpm -C synthia-web exec playwright test --config=playwright.contract.config.ts` passes locally. Commit: `test(contract-closure): bootstrap Playwright contract sub-suite`.

---

## Task 5: 修复卡片循环（核心执行期 — 内容由任务 4.x 持续追加）

**Files:** 由 `tasks.md` §4 中每个修复卡片指定；本循环不预设。

- [ ] **Step 1:** 打开 `tasks.md` §4.1 第一张修复卡片；阅读卡片内描述（包含 arbitration source、expected behavior）。
- [ ] **Step 2:** 在 `contract.yaml` 中精确定位对应 entry；记录当前双侧字段不一致点。
- [ ] **Step 3:** 在 `scripts/test/contract-check.test.ts` 中追加"修复后这个 entry 应消失 dangling"的预期，并跑失败（红）。
- [ ] **Step 4:** 修改后端或前端（按 ARBITRATION 优先级定向），保持 diff 局部。
- [ ] **Step 5:** 在 `tests/e2e/integration/contract-closure/` 中补对应 spec（至少 1 正向 + 1 反向用例）。
- [ ] **Step 6:** `make contract-check && make test-contract-closure`，全绿后提交一个 atomic commit：`fix(contract): <卡片 #> <一句话>`。
- [ ] **Step 7:** 在 `tasks.md` 勾选 §4.x 对应行；继续下一张卡片。

---

## Task 6: 升级为 blocking + 收尾

**Files:**
- `.github/workflows/contract-closure.yml` (modify — drop `continue-on-error`)
- `openspec/changes/synthia-interface-contract-closure/verify.md` (NEW)
- `openspec/changes/synthia-interface-contract-closure/retrospective.md` (NEW)

- [ ] **Step 1:** Confirm all §4 修复卡片 closed and all §5 三同步 met. Capture metrics in `verify.md`: row count, cards closed, Playwright tests added, arbitration-source refs.
- [ ] **Step 2:** Drop `continue-on-error: true` from CI yaml; merge to main.
- [ ] **Step 3:** Make `make contract-check && make test-contract-closure` blocking on PR.
- [ ] **Step 4:** Author `retrospective.md` with "双层契约闭环" template export.
- [ ] **Step 5:** Run `openspec archive synthia-interface-contract-closure` per project convention.
- [ ] **Step 6:** Commit: `chore(contract-closure): promote advisory → blocking + archive`.
