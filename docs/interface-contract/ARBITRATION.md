# Synthia 接口契约仲裁规则 (ARBITRATION)

> 当双侧契约出现不一致时，本规则决定修改方向。

## 优先级（高 → 低）

1. **A2A 官方协议 spec**
   - 适用于 A2A 协议面（`/a2a/*` 端点、JSON-RPC method `message:send` / `tasks:get` / `tasks:cancel`、A2A SSE 事件等）。
   - 引用方式：在修复卡片 Reason 段写明 "A2A v0.x §xx.x"。

2. **`@a2a-js/sdk` TypeScript 类型**
   - 适用于前端实际依赖的 A2A 客户端 SDK 类型。
   - 引用方式：在 Reason 段写明 "`@a2a-js/sdk` <版本> `Message`/`Task`/`Part` 类型"。

3. **Synthia 既有 stable spec**
   - 适用于本地协议面（`/api/v2/*`、自研 SSE、`event-v2-system`、`session-replay-harness` 等）。
   - 引用方式：在 Reason 段写明 "spec:`openspec/specs/<capability>/spec.md` §<Requirement>"。

## 无协议源时

如果某条端点在 1–3 都没有源参考：
- 必须附带最小 ADR（短文 ≤ 200 字）说明：为什么这个端点需要存在、命名约定参考哪一个上游、由谁 review。
- 在 [verify.md](../../openspec/changes/synthia-interface-contract-closure/verify.md)（如果本仓库内）或 PR 描述里**显式提升为阻塞项**，禁止直接拍脑袋修改。

## 冲突 / 兼容性

如果一个修改同时影响多个端点（例如 SSE 事件重命名涉及整个事件序列）：
- 必须保留原事件名/字段名作为 deprecated alias 至少一个 minor 版。
- Playwright 契约集中同时存在新/老两个用例，确保迁移期间前端可读老字段。

## 何时修订本规则

修改本规则需要：
1. 在 PR 中显式说明触发原因；
2. 引用一次具体的"之前错误引用本规则"的反例；
3. 在 `openspec/specs/interface-contract-arbitration/spec.md` 中按 MODIFIED 流程更新。
