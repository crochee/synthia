# interface-contract-matrix Specification

## Purpose
TBD - created by archiving change synthia-interface-contract-closure. Update Purpose after archive.
## Requirements
### Requirement: 双侧契约并集表必须可机器读取

系统 MUST 在 `docs/interface-contract/contract.yaml` 维护一份覆盖 Synthia 前后端全部交互路径的契约并集表。每一行必须包含双侧端点（HTTP 方法 + 路径，或 SSE 事件名）、双侧载荷字段、双侧流式事件节奏元数据、来源文件指针。

#### Scenario: 生成契约表
- **WHEN** 开发者执行 `make contract-scan`
- **THEN** 系统 MUST 扫描 `crates/synthia-server/src/router.rs` 与 `synthia-web/src/api/**`（包括 `.ts/.tsx` 中所有 `fetch`、A2A client 调用）并写出 `docs/interface-contract/contract.yaml` 与人类可读的 `docs/interface-contract/contract.md`

#### Scenario: 契约表覆盖率校验
- **WHEN** 开发者执行 `make contract-check`
- **THEN** 系统 MUST 校验 contract.yaml 的每一行既有前端调用点又有后端注册，且双侧字段集合对齐；任何悬空（双侧任一缺失）必须列出并返回非零退出码

### Requirement: 契约冲突仲裁规则必须文档化

`docs/interface-contract/ARBITRATION.md` MUST 明确以下规则：当双侧字段命名/类型不一致时，**以官方协议源为准**，来源优先级：(1) A2A 官方 spec（若端点属于 A2A），(2) `@a2a-js/sdk` TypeScript 类型（前端依赖的官方客户端），(3) Synthia 既有 replay 模型 / event-v2-system spec。任何修改必须援引该规则。

#### Scenario: 冲突项处理
- **WHEN** 在 `tasks.md` 修复卡片中出现命名不一致
- **THEN** 该卡片 MUST 注明引用的仲裁源条款，并按源条款执行修改

#### Scenario: 无仲裁源时
- **WHEN** 修复卡片对应端点不属于 (1)(2)(3) 任一源
- **THEN** 卡片 MUST 写明"无协议源"并在 `verify.md` 提升为阻塞项

### Requirement: 契约表每日版本化

`docs/interface-contract/` 下的契约表 MUST 接受 git 跟踪；任何 PR 修改契约表必须有 Playwright 用例同步更新。

#### Scenario: PR 中修改契约表
- **WHEN** PR 修改了 `docs/interface-contract/contract.yaml`
- **THEN** CI MUST 检查 `synthia-web/tests/e2e/integration/contract-closure.*.spec.ts` 是否同步更新，未同步则 PR 校验失败

### Requirement: 修复卡片对应 contract entry 必须落表

系统 MUST 在 `docs/interface-contract/contract.yaml` 中为每张已登记的修复卡片（`tasks.md` §4.x）维护至少一行 entry，标注该卡片对应的端点、方法、路径、双侧字段、不一致描述、以及引用的仲裁源条款（见 `interface-contract-arbitration`）。

#### Scenario: 修复卡片新增 entry
- **WHEN** `tasks.md` §4 中追加一张新修复卡片
- **THEN** 该卡片对应的 contract entry MUST 在同一 PR 中落地；entry 的 `arbitration_source` 字段 MUST 引用 ARBITRATION.md 的 (1) A2A 官方 / (2) `@a2a-js/sdk` / (3) Synthia stable spec 中的一条

#### Scenario: 修复卡片闭环后
- **WHEN** 一张修复卡片完成双侧实现 + Playwright 用例同步
- **THEN** 该 entry 的 `status` MUST 标记为 `closed`；`make contract-check` MUST 校验所有 §4 卡片均有对应 entry 且状态一致

### Requirement: 契约表必须包含 SSE 事件契约

`contract.yaml` MUST 为每个 SSE 端点（`tasks/{id}:subscribe` 等）维护子节点 `sse_events: [{name, fields, cadence}]`，列出该端点产生的全部事件名、必填字段、流式节奏元数据。

#### Scenario: SSE 事件字段登记
- **WHEN** 后端 SSE handler 在 `crates/synthia-server/src/**` 发出新事件类型
- **THEN** `make contract-scan` MUST 在 contract.yaml 中自动生成对应事件条目；前端 reducer 在收到未登记事件时 MUST 输出 console.error

#### Scenario: SSE cadence 登记
- **WHEN** 修复卡片 #008 落地后
- **THEN** `tasks/{id}:subscribe` entry 的 `sse_events[*].cadence.max_idle_ms` MUST 设置为具体数值（默认 30000），前端按 `Retry-After` header 处理重连

