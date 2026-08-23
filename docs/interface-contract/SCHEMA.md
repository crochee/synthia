# Synthia 契约表 SCHEMA (v1)

> `docs/interface-contract/contract.yaml` 与 `contract.json` 的字段定义。
> 任何字段重命名/类型变更必须同步更新本文件与 CI 校验脚本。

## 顶层

```yaml
version: 1                   # schema 版本（int）
generated_at: "<ISO8601>"    # 本次扫描生成时间
endpoints:
  - ...                      # Endpoint 列表，按 method+path 字典序
```

## Endpoint

```yaml
- id: "GET /api/health"         # 唯一 id: `<METHOD> <PATH>`
  method: "GET"                  # HTTP 动词 或 chat JSON-RPC method
  path: "/api/health"            # axum 路由语法（含 {name} 占位符）
  source: "both"                 # backend | frontend | both
  source_files:
    backend:
      - "crates/synthia-server/src/server/router.rs:91"
    frontend:
      - "synthia-web/src/hooks/useServerHealth.ts:23"
  notes?: "optional human note"
  sse_events?:                   # 仅当该端点为 SSE 流时存在
    - name: "status-update"
      fields: ["sessionId", "sessionState"]
      cadence_ms: 250
  status?: "open" | "closed"     # fix-card lifecycle marker；默认 open
```

## 字段语义

- `id`：在本文件中必须唯一。命名约定为 `<METHOD> <PATH>`，不允许 URL-encode。
- `method`：枚举 `GET | POST | PUT | DELETE | PATCH | message:send`。
- `path`：与 axum router 完全一致，保留 `{...}` 占位符。
- `source`：报告该端点的来源状态。本字段由扫描器自动生成，**禁止手工编辑**。
- `source_files.backend / frontend`：来自双侧的源代码位置指针（`file:line`），多个可列举。
- `sse_events[]`：仅当端点为 SSE 流时存在；每个 event 一条，含 `name`、`fields`、可选 `cadence_ms`。
- `notes`：可选人工注释，只用于解释、绝不参与字段校验。
- `status`：可选 fix-card 生命周期标记。`closed` 表示对应的不一致已经被修复
  （参见 `openspec/changes/synthia-interface-contract-closure-cycle-2/tasks.md`
  中的修复卡片）；`open`（或缺省）表示仍需处理。仅在
  `unionEndpoints` 两侧扫描结果一致时才会保留；任一侧缺失或回归时
  自动降级为 `undefined`（视同 open）。

## 不变式

1. **`source=both` 必须既有 `backend` 又有 `frontend` 来源文件指针**。
2. **`source=backend` 必须只有 `backend` 来源文件指针**。
3. **`source=frontend` 视为悬空 (dangling)**，CI 闸门视作阻塞。
4. **`backend-only` 视为警告**：可能在演化中尚未被前端接入，不阻塞但需 review。
5. **`status` 在 `make contract-scan` 重新生成时保留**：`contract-scan`
   读取已存在的 `contract.yaml` 并把整个 entry 列表透传回
   `unionEndpoints` 的 `preserve` 参数，使得手工标记的
   fix-card 端点（scanner 看不到的 `nest_service` / 外部 SDK 路由）
   不会在每次重新扫描时被悄悄丢掉。

CI 校验脚本 `contract-check` 用本不变式判断退出码。
