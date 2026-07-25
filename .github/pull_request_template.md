# PR 模板 — Synthia (contract-closure cycle #2)

## 改了哪些东西

<!--
用一句话写明变更意图。例：
- 修复修复卡片 #004 (SSE artifact-update 缺 lastChunk)
- 新增 contract-closure scanner 的 A2A SDK 调用覆盖
- 升级 CI 闸门
-->

## Checklist

- [ ] contract.yaml 同步（`docs/interface-contract/contract.yaml` 与本次改动一致；运行 `make contract-scan` 后已 commit）
- [ ] contract-closure.*.spec.ts 同步（若改了 `contract.yaml` 中任何端点，**必须**同步修改对应的 `synthia-web/tests/e2e/integration/contract-closure/*.spec.ts`）
- [ ] tasks.md §4.x 行号引用（`openspec/changes/synthia-interface-contract-closure-cycle-2/tasks.md` §4.x 中的文件:行号引用在改动后仍然准确）
- [ ] `make contract-check` 全绿（exit 0，无 frontend-only endpoint）
- [ ] Rust 代码改了 → `make fmt && make lint && make test-rust -p <crate>` 全部绿
- [ ] 前端代码改了 → `make fmt-web && make lint-web` 绿
- [ ] 新增能力（如新增端点）→ 在 `openspec/specs/<capability>/spec.md` 中按 ADDED/MODIFIED 流程补 Requirement
- [ ] 修复卡片的"仲裁源"已写明（在 commit message 或 PR 描述里写明 protocol-source 引用：`A2A 官方 spec §X.Y` / `@a2a-js/sdk v1.0.0` / `Synthia stable spec §X.Y`）

## 关联

- Fix card(s) #:
- Spec(s) updated:
- Breaking change: 是 / 否
