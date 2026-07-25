## ADDED Requirements

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