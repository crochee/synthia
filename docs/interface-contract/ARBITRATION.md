# Synthia 接口契约仲裁规则 (ARBITRATION)

> 当双侧契约出现不一致时，本规则决定修改方向。

## 优先级（高 → 低）

1. **Synthia 既有 stable spec**
   - 适用于本地协议面（`/api/v1/*`、自研 SSE、`event-v2-system`、`session-replay-harness` 等）。
   - 引用方式：在 Reason 段写明 "spec:`openspec/specs/<capability>/spec.md` §<Requirement>"。

2. **Synthia chat wire contract**
   - 适用于 `/api/v1/chat/sessions/*` 与 `/api/v1/sessions/*` 端点、REST + SSE 事件（`sessionStatus` / `message` / `turnStatus` / `attachment`）等。
   - 引用方式：在 Reason 段写明 "spec:`docs/interface-contract/SCHEMA.md` §<Endpoint or SSE event>"。

3. **历史 JSON-RPC 兼容**
   - 适用于历史 `message:send` JSON-RPC method 字段命名（保留以兼容旧 contract.yaml 行）。
   - 引用方式：在 Reason 段写明 "method:`message:send` (Synthia legacy)"。

## 无协议源时

如果某条端点在本协议集内没有源参考：
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
