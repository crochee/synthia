# PR 模板 — Synthia

## 改了哪些东西

<!--
用一句话写明变更意图。例：
- 修复修复卡片 #004 (SSE artifact-update 缺 lastChunk)
- 新增 contract-closure scanner 的 A2A SDK 调用覆盖
- 升级 CI 闸门
-->

## Checklist

- [ ] 跑了 `make contract-scan && make contract-check`，输出 contract.yaml 已提交
- [ ] 如果改了 `docs/interface-contract/contract.yaml`，**必须**同步修改对应的 `synthia-web/tests/e2e/integration/contract-closure/*.spec.ts`
- [ ] Rust 代码改了 → `make fmt && make lint && make test-rust -p <crate>` 全部绿
- [ ] 前端代码改了 → `make fmt-web && make lint-web` 绿
- [ ] 新增能力（如新增端点）→ 在 `openspec/specs/<capability>/spec.md` 中按 ADDED/MODIFIED 流程补 Requirement
- [ ] 修复卡片的"仲裁源"已写明（在 commit message 或 PR 描述里写明 protocol-source 引用）

## 关联

- Fix card(s) #:
- Spec(s) updated:
- Breaking change: 是 / 否
