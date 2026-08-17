<!--
Raw capture of brainstorming output for restful-api-v1 change.

本档原样捕捉 brainstorming 的产出，包含三轮审查的完整决策链。
-->

# Synthia RESTful API v1 — Brainstorming Decision Log

## 背景

Synthia Management API 当前存在以下问题:
1. 使用 `ApiResponse<T>` 信封包裹所有响应，列表接口双层嵌套 (`data.data`)
2. 每个列表资源独立定义 `XxxListResponse`，无通用 `List<T>` 泛型
3. 无分页支持，列表返回全量
4. DELETE 响应不统一 (200/204 混用)
5. 缺少 PATCH 方法，settings 只能全量替换
6. 路由无版本前缀，不利于后续演进

参考设计:
- 列表查询: `cim-storage/src/model.rs` 的 `List<T>` 模式
- 错误响应: `cim-slo/src/errors.rs` 的 `WithBacktrace` + `IntoResponse` 模式
- API 设计: api-design skill (REST conventions)

## 决议链

### Q1: 信封策略 → 裸返回
**决策**: 移除 `ApiResponse<T>` 信封，成功响应直接返回资源对象。
- 列表: 直接返回 `List<T>`
- 详情: 直接返回 `T`
- 错误: HTTP Status + `{ code, message, result? }`

### Q2: 版本前缀 → /api/v1/
**决策**: 使用版本前缀 `/api/v1/*`

### Q3: 分页模式 → cursor + limit (keyset pagination)
**决策**: cursor = base64(资源 ID)，limit 控制页大小。
- 不使用 offset (无 offset 概念)
- 不使用 page_size (统一用 limit)
- cursor 编码上一页末尾资源的 ID，实现 keyset 分页

### Q4: 错误响应 → HTTP Status + { code, message, result? }
**决策**: 使用语义码 (非数字码)，result? 替代 details?
- 新增 `InvalidCursor` 和 `InvalidSortField` 错误码

### Q5: DELETE 统一 → 204 No Content
**决策**: 所有 DELETE 操作返回 204，无 Body

### Q6: PATCH → 不使用，只用 PUT
**决策**: 移除所有 PATCH，统一用 PUT 更新
- Skills PUT 仅修改 enabled (文档明确)
- Settings PUT 全量替换 (必须先 GET 再 PUT)

### Q7: Providers → 只读
**决策**: Providers 仅保留 GET，移除所有写操作 (POST/PUT/DELETE)
- Provider 配置通过 config.toml 管理，不支持运行时修改

### Q8: MCP JSON-RPC 路径分离 → /api/v1/mcp/rpc
**决策**: JSON-RPC 移到 `/mcp/rpc`，REST 留在 `/mcp/servers`

### Q9: TaskStatus → 统一用 A2A TaskState
**决策**: GET /api/v1/tasks 列表和详情统一使用 A2A TaskState 状态值

### Q10: Sort 白名单
**决策**: 每资源定义可排序字段白名单，非法字段返回 400

### Q11: API Key 脱敏
**决策**: 响应中 api_key 脱敏，保留前4+后3，中间 ***

### Q12: Resource name 校验
**决策**: `^[a-zA-Z0-9_-]{1,255}$`，防止路径穿越

### Q13: cursor + limit 边界行为
**决策**:
- limit=0 → 400
- limit>100 → 截断为 100
- cursor 解码失败 → 400
- cursor 指向已删除资源 → 空列表

### Q14: Jobs pause/resume 拆分
**决策**: 拆分为两个明确端点: POST /jobs/:key/pause + POST /jobs/:key/resume

### Q15: 补充 GET /api/v1/tasks/:id 详情
**决策**: 新增任务详情端点，返回完整 Task (含 history, artifacts)

### Q16: Task/Job 查询参数
**决策**: 定义 TaskPageQuery (status, context_id) 和 JobPageQuery (key, trigger_contains)

### Q17: MCP Server 连接状态
**决策**: McpServerInfo 补充 status (连接状态) 和 pid

### Q18: Provider 完整字段
**决策**: ProviderInfo 补充 base_url, context_window, max_output_tokens, supports_* 等

### Q19: Registry trait 长期改造
**决策**: 新增 `list_paginated(cursor, limit, sort, filter)` 方法，短期在 handler 层切片

### Q20: 响应头精简
**决策**: 只保留 traceparent + Content-Type

### Q21: MCP Server 级联删除
**决策**: 删除 MCP server 时级联 unregister 其 tools

### Q22: 版本迁移策略
**决策**: 长期移除 /api/*，全面使用 /api/v1/*

### Q23: Memory search 复用 PageQuery
**决策**: memory/search 的 limit 和 cursor 统一走 PageQuery

## 核心类型最终设计

### List\<T\>
```rust
pub struct List<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,  // base64(末尾资源ID), 有更多=Some, 末尾=None
    pub total: Option<u64>,           // 大表省略
}
```

### PageQuery
```rust
pub struct PageQuery {
    pub cursor: Option<String>,  // base64(资源ID)
    pub limit: Option<u64>,     // 默认20, 最大100
    pub sort: Option<String>,   // "-" 前缀=DESC, 白名单约束
}
```

### Cursor 编解码
```rust
fn encode_cursor(id: &str) -> String { base64(id) }
fn decode_cursor(cursor: &str) -> Result<String, UserError> { base64_decode(cursor) }
```

## 端点总览

### Public (无认证)
- GET /health → { status, version }
- GET /.well-known/agent-card.json → AgentCard

### A2A Protocol (认证)
- /a2a/* → A2A JSON-RPC + REST

### Management (认证, /api/v1)
| Method | Path | Response |
|--------|------|----------|
| GET | /models | Models (裸, 无分页) |
| GET | /tasks | List\<TaskSummary\> |
| GET | /tasks/:id | TaskDetail (裸) |
| GET | /providers | List\<ProviderInfo\> |
| GET | /providers/:name | ProviderInfo (裸) |
| GET | /skills | List\<SkillInfo\> |
| GET | /skills/:name | SkillDetail (裸) |
| POST | /skills | 201 SkillDetail |
| PUT | /skills/:name | 200 SkillEnabled |
| DELETE | /skills/:name | 204 |
| POST | /skills/reload | 200 { reloaded, count } |
| GET | /tools | List\<ToolInfo\> |
| GET | /tools/:name | ToolDetail (裸) |
| POST | /tools | 201 ToolDetail |
| DELETE | /tools/:name | 204 |
| GET | /commands | List\<CommandInfo\> |
| GET | /commands/:name | CommandInfo (裸) |
| DELETE | /commands/:name | 204 |
| GET | /jobs | List\<JobInfo\> |
| POST | /jobs | 201 { key, status } |
| DELETE | /jobs/:key | 204 |
| POST | /jobs/:key/execute | 200 { key, status } |
| POST | /jobs/:key/pause | 200 { key, status } |
| POST | /jobs/:key/resume | 200 { key, status } |
| POST | /mcp/rpc | JsonRpcResponse |
| GET | /mcp/servers | List\<McpServerInfo\> |
| GET | /mcp/servers/:id | McpServerInfo (裸) |
| POST | /mcp/servers | 201 { registered, name } |
| POST | /mcp/servers/:id/discover | 200 { server, tools } |
| DELETE | /mcp/servers/:id | 204 |
| GET | /memory/search | List\<MemoryResult\> |
| GET | /settings | Settings (裸, api_key脱敏) |
| PUT | /settings | 200 Settings |
| GET | /approvals | List\<ApprovalInfo\> |
| POST | /approvals/:id/resolve | 200 { resolved } |

### WebSocket
- WS /ws/approvals?token=xxx → ApprovalEvent 实时流

## 废弃清单

- `src/api/envelope.rs` → 裸返回替代
- `src/api/pagination.rs` → List\<T\> + PageQuery 替代
- `ApiResponse<T>` → Management API 不再使用 (A2A 内部可保留)
- /api/* → /api/v1/*
- /api/mcp (JSON-RPC) → /api/v1/mcp/rpc
