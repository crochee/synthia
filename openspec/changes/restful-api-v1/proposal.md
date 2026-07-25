## Why

Synthia Management API 当前使用 ApiResponse<T> 信封包裹所有响应，列表接口双层嵌套 (data.data)，每个列表资源独立定义 XxxListResponse 无通用泛型，无分页支持，DELETE 响应不统一，路由无版本前缀。参考 CIM 项目的 List<T> + WithBacktrace 错误模式，统一为裸返回 + keyset 分页 + 标准错误格式，提升 API 一致性和前端开发体验。

## What Changes

**响应格式**
- From: `ApiResponse<T>` 信封 `{ status: "ok", data: T }` / `{ status: "err", error: {...} }`
- To: 成功裸返回 T，错误 HTTP Status + `{ code, message, result? }`
- Reason: 消除列表双层嵌套，前端解析更直接
- Impact: breaking — 前端所有 API 调用需适配

**列表响应**
- From: 各资源独立 `XxxListResponse { items, count }`
- To: 通用 `List<T> { data, next_cursor?, total? }`
- Reason: 统一分页，支持大规模数据集
- Impact: breaking — 前端 items → data，新增 cursor 翻页

**分页**
- From: 无分页，全量返回
- To: cursor (base64 资源 ID) + limit，keyset pagination
- Reason: 新增/删除记录不跳过/重复，DB 友好
- Impact: 新增 — 前端实现翻页 UI

**路由版本**
- From: `/api/*`
- To: `/api/v1/*`
- Reason: 支持后续 API 演进
- Impact: breaking — 路由变更

**Providers**
- From: POST/PUT/DELETE 端点存在但空实现或 403
- To: 仅保留 GET，只读资源
- Reason: Provider 配置通过 config.toml 管理，运行时修改不持久化
- Impact: non-breaking — 原有写操作本就不可用

**DELETE 响应**
- From: 200 + body / 204 混用
- To: 统一 204 No Content
- Reason: RESTful 一致性
- Impact: breaking — 前端 DELETE 处理需适配

**MCP JSON-RPC 路径**
- From: `/api/mcp` 混合 JSON-RPC + REST
- To: `/api/v1/mcp/rpc` (JSON-RPC) + `/api/v1/mcp/servers/*` (REST)
- Reason: 协议分离，路径语义清晰
- Impact: breaking — MCP 客户端需适配

## Capabilities

### New Capabilities
- `api-list-pagination`: 通用 List<T> 泛型 + cursor/limit keyset 分页 + PageQuery 参数解析 + sort 白名单
- `api-error-response`: UserError IntoResponse 实现 (HTTP Status + { code, message, result? }) + ErrorCode 补充 (InvalidCursor, InvalidSortField) + 资源名校验
- `api-bare-response`: 裸返回模式 — handler 返回类型从 ApiResponse<T> 改为直接 Json(T) + 统一 DELETE 204

### Modified Capabilities
- `api-management-routes`: 路由从 /api/* 迁移到 /api/v1/*，Providers 降级只读，Skills/Settings PUT 语义明确，MCP RPC 路径分离，Tasks 补充详情端点和过滤参数，MCP Server 补充连接状态

## Impact

**后端 (synthia-server)**
- 废弃 `src/api/envelope.rs`, `src/api/pagination.rs`
- 改造 `src/api/error.rs` 对齐新错误格式
- 所有 handler 返回类型变更 (ApiResponse<T> → 裸返回)
- 路由表从 /api/* 迁移到 /api/v1/*
- 新增 List<T>, PageQuery, Cursor 编解码到 synthia-core

**后端 (synthia-core)**
- 新增 List<T>, PageQuery 公共类型
- UserError 新增 IntoResponse impl
- Registry trait 新增 list_paginated 方法 (长期)

**前端 (synthia-web)**
- API 响应解析: response.data → 直接 response
- 错误处理: HTTP status check 替代 status==="err"
- 分页: 新增 cursor 传参
- 列表: response.items → response.data
- DELETE: 204 无 Body
