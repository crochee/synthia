# interface-contract-playwright Specification

## Purpose
TBD - created by archiving change synthia-interface-contract-closure. Update Purpose after archive.
## Requirements
### Requirement: Playwright 契约集必须 100% 覆盖契约表每一行

`synthia-web/tests/e2e/integration/contract-closure.*.spec.ts` MUST 为 `docs/interface-contract/contract.yaml` 中登记的**每一条**接口路径（含 SSE 事件）提供至少 1 条集成测试；测试通过后才视为该接口已闭环。

#### Scenario: 接口覆盖率
- **WHEN** 开发者执行 `make contract-coverage`
- **THEN** 脚本 MUST 读取 contract.yaml 并核对 contract-closure.\*.spec.ts 中 test() 或 test.describe() 标题包含 contract.yaml 中每个 path/id；缺失的路径必须在报告中列出并以非零退出码失败

#### Scenario: SSE 事件覆盖
- **WHEN** 契约表登记一个 SSE 事件名（例如 `artifact-update`）
- **THEN** MUST 至少存在一条以该事件名命名的 test 或 describe，且该用例驱动 server 真实产生该事件并校验前端收到了对应字段

### Requirement: Playwright 契约集必须接入 CI

`make test-contract-closure` MUST 在 CI（`.github/workflows/`）中作为必跑步骤；任何失败阻塞合并。

#### Scenario: CI 跑契约集
- **WHEN** PR 推送到 GitHub
- **THEN** CI MUST 调用 `make test-contract-closure`；任何用例失败 MUST 使 PR 不可合并

### Requirement: Playwright SSE harness 必须支持事件序列断言

`synthia-web/tests/e2e/integration/contract-closure/_helpers/sse-harness.ts` MUST 提供 `subscribeAndCapture(url, options)` 函数，驱动 server 真实产生 SSE 事件流并把事件序列捕获到内存数组；spec MUST 能对该数组做"完整事件序列"断言（如 task 创建 → status-update×N → artifact-update×N → final status）。

#### Scenario: SSE 完整事件序列用例
- **WHEN** 一张修复卡片（如 #003/#004/#008）落地后
- **THEN** `tests/e2e/integration/contract-closure/` MUST 至少新增 1 条 spec 驱动 server 完成一次完整任务生命周期，并断言 SSE 事件序列与 contract.yaml 中登记的 `sse_events` 一致

#### Scenario: SSE 反向用例
- **WHEN** 后端漏发 `artifact-update` 或错发 `status-update` 枚举值
- **THEN** 对应 spec MUST 失败；失败信息 MUST 指出缺失/错误的事件名

### Requirement: contract-coverage 报告必须包含"未覆盖路径"段落

`make contract-coverage` 的输出 MUST 包含 "未覆盖路径" 段落，列出 contract.yaml 中登记但 Playwright 契约集未覆盖的所有 entry（按 endpoint / SSE 事件分类）。该段落在 CI advisory 模式下 MUST 列出（warn-level），§6.1 升级为 blocking 后 MUST 改为 failing exit code。

#### Scenario: 报告输出未覆盖路径
- **WHEN** 开发者执行 `make contract-coverage`
- **THEN** 输出 MUST 包含 `Uncovered paths:` 段，每条形如 `<METHOD> <path>` 或 SSE 事件 `<event_name>`；总数大于 0 时 advisory 模式下 exit code 为 0 但 stderr 列出清单

#### Scenario: §6.1 升级后未覆盖路径阻塞
- **WHEN** CI 模式升级为 blocking（cycle #2 末评估）
- **THEN** 未覆盖路径非空时 `make contract-coverage` MUST exit code 非 0，CI 失败

