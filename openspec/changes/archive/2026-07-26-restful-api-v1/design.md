## Context

Synthia Management API 当前为 V2 风格，使用 `ApiResponse<T>` 信封包裹所有响应。列表端点每资源独立定义 `XxxListResponse`，无通用泛型，无分页。错误走 `ApiError` + `IntoResponse`。路由无版本前缀。

参考 CIM 项目的两处设计:
- `cim-storage/src/model.rs`: `List<T>` 泛型 + `Pagination` 请求
- `cim-slo/src/errors.rs`: `WithBacktrace` + `IntoResponse` (HTTP Status + JSON body)

经三轮审查确认 37 个决策点，本设计记录全部技术决策与实现路径。

## Goals / Non-Goals

**Goals:**
- 统一列表响应为 `List<T>` 泛型，支持 cursor+limit keyset 分页
- 移除 `ApiResponse<T>` 信封，成功裸返回，错误靠 HTTP Status
- 统一错误响应为 `{ code, message, result? }`，补充 InvalidCursor/InvalidSortField 错误码
- 路由迁移到 `/api/v1/*`，版本化
- Providers 降级只读，DELETE 统一 204，PUT 替代 PATCH
- MCP JSON-RPC 路径分离，Tasks 补充详情和过滤，MCP Server 补充连接状态
- API Key 脱敏，Resource name 校验防路径穿越

**Non-Goals:**
- A2A 协议端点改造 (A2A 有自己的 JSON-RPC 信封规范)
- 数据库集成 (短期 Registry 仍为内存 HashMap，DB 为长期目标)
- ETag/If-Match 乐观并发控制 (P2)
- OpenAPI/Swagger 文档端点 (P2)
- 自定义 rejection extractor 统一 405/415/413 格式 (P2)

## Decisions

### D1: 裸返回 vs 信封
- **选择**: 移除 `ApiResponse<T>` 信封，成功响应裸返回
- **理由**: 列表双层嵌套 (`data.data`) 消除，前端解析更直接；错误靠 HTTP Status 判断，无需检查 `status` 字段
- **已考虑 alternative**: 保留信封 — 一致性强但嵌套冗余，且与 CIM 裸返回模式不一致

### D2: cursor + limit (keyset pagination)
- **选择**: cursor = base64(资源ID)，limit 控制页大小，无 offset
- **理由**: keyset 分页在新增/删除记录时不跳过/重复；DB 友好 (索引命中)；cursor 编码资源 ID 客户端 opaque
- **已考虑 alternative**: offset+limit — 传统但大表性能差，并发时跳过/重复；cursor+page_size — 语义等价但 page_size 命名冗余

### D3: cursor 编码 = base64(资源ID)
- **选择**: cursor 直接编码上一页末尾资源的 ID，base64 编码
- **理由**: 简单、opaque、服务端解码后可直接用于 WHERE id > last_id 查询
- **已考虑 alternative**: base64(JSON{offset,sort}) — 复杂且暴露内部结构；纯 offset — 不符合 keyset 分页语义

### D4: 错误响应 { code, message, result? }
- **选择**: 语义码 (not_found, validation_error)，result? 替代 details?
- **理由**: 语义码可读性优于数字码；result 命名比 details 更通用 (可承载验证错误、限流信息等)
- **已考虑 alternative**: 数字码 (CIM 的 "1010002") — 机器友好但人类不可读

### D5: DELETE 统一 204
- **选择**: 所有 DELETE 返回 204 No Content
- **理由**: RESTful 最标准，删除操作无需返回被删除资源
- **已考虑 alternative**: 200 + { deleted: true } — 前端友好但不一致

### D6: 只用 PUT，不用 PATCH
- **选择**: 移除所有 PATCH，统一 PUT
- **理由**: 减少方法数，简化前端调用；Skills PUT 仅修改 enabled (文档明确)；Settings PUT 全量替换 (read-modify-write)
- **已考虑 alternative**: PATCH 局部更新 — 语义更准确但增加复杂度，需定义 JSON Merge Patch (RFC 7396) 语义

### D7: Providers 只读
- **选择**: 仅保留 GET /providers 和 GET /providers/:name
- **理由**: Provider 配置来自 config.toml，运行时修改不持久化，原有写操作本就空实现或 403
- **已考虑 alternative**: 写回 config.toml — 实现复杂且影响启动配置一致性

### D8: MCP JSON-RPC 路径分离
- **选择**: `/api/v1/mcp/rpc` (JSON-RPC) + `/api/v1/mcp/servers/*` (REST)
- **理由**: 协议分离，路径语义清晰，避免同一端点混用两种协议
- **已考虑 alternative**: 保留 /mcp 混合 — 简单但语义模糊

### D9: TaskStatus 统一 A2A TaskState
- **选择**: 列表和详情都用 A2A TaskState 枚举值
- **理由**: list 已通过 A2A handler 获取，统一避免映射错误
- **已考虑 alternative**: 用 synthia-task 的 TaskStatus — 需额外映射层，且用户可见状态应与 A2A 一致

### D10: Sort 白名单 + 资源名校验
- **选择**: 每资源定义可排序字段白名单；资源名 `^[a-zA-Z0-9_-]{1,255}$`
- **理由**: 防注入 (sort 字段映射到查询；资源名拼路径)
- **已考虑 alternative**: 无校验 — 当前数据量小可侥幸，但安全隐患不可接受

### D11: API Key 脱敏
- **选择**: 保留前 4 + 后 3，中间 `***`
- **理由**: 前端需知道是否已设置 API Key，但不应暴露完整密钥
- **已考虑 alternative**: 仅返回 `api_key_set: bool` — 丢失部分信息 (用户无法辨认是哪个 key)

## Risks / Trade-offs

[Trade-off] Settings PUT 全量替换 — 前端必须先 GET 再 PUT，缺字段被清空 → 接受理由: read-modify-write 是 REST PUT 标准模式，文档明确

[Trade-off] Skills PUT 仅改 enabled — 非标准 PUT 语义 (部分更新) → 接受理由: skills 是文件系统驱动，只有 enabled 可通过 API 修改，文档明确约束

[Trade-off] 无 PATCH — 部分更新场景需全量 PUT → 接受理由: 减少 HTTP 方法数，简化实现和前端调用

[Risk] 前端 breaking change 全面适配 → Mitigation: 一次性迁移，无兼容期 (/api/* 最终移除)

[Risk] cursor 编码资源 ID，若 ID 格式变更则旧 cursor 失效 → Mitigation: 客户端遇到空列表自动回到首页

[Risk] Registry::list() 短期返回全量 Vec，handler 层切片 → Mitigation: 当前注册表数据量小 (<100)，内存可接受；长期接 DB 时改造 list_paginated

## Migration Plan

1. **新增公共类型** — synthia-core 新增 List<T>, PageQuery, Cursor 编解码
2. **改造 UserError** — 新增 IntoResponse impl，补充 InvalidCursor/InvalidSortField
3. **废弃 api/ 模块** — envelope.rs/pagination.rs 标记 deprecated
4. **逐资源改造 handler** — 返回类型从 ApiResponse<T> 改为裸返回
5. **路由迁移** — /api/* → /api/v1/*，MCP RPC 分离
6. **前端适配** — 响应解析、错误处理、分页、DELETE 全面更新
7. **测试** — 更新所有集成测试的断言

Rollback: 保留 /api/* 路由 30 天过渡期 (301 → /api/v1/*)

## Open Questions

None — 三轮审查 37 个决策点全部落定
