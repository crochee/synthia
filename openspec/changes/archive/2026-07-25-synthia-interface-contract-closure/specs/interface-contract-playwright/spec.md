## ADDED Requirements

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
