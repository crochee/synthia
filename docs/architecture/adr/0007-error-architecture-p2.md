# ADR-0007: Error 架构稳定性契约（P2 阶段方案）

## Status

Accepted (2026-08-04)

## Context

synthia-core 当前 `Error` 是 33 变体 thiserror enum，P1 收敛目标 15 变体。在 P2 阶段我们做了增量补强：

1. `ErrorCode` 加 `#[non_exhaustive]` —— 允许未来加变体不破坏下游 `match`
2. 5 个高频变体（`NotFound` / `Validation` / `Internal` / `AlreadyExists` / `InvalidItem`）改 struct form，加 `location` 字段
3. 加 `#[track_caller]` helper 方法（`Error::not_found()` / `validation()` / `internal()` / `already_exists()` / `invalid_item()`），call site 自动捕获
4. `From<reqwest::Error>` 加 `#[track_caller]`，内部错误自动带 location
5. 修复 synthia-session/context 双错误模型边界：保留内部 anyhow（开发便利），公开 API 边界加 `From<SessionError> for synthia_core::Error` / `From<anyhow::Error> for SessionError` 桥接
6. synthia-server `ServerError` 加 `#[non_exhaustive]` + `From<synthia_core::Error>` / `From<UserError>` / `From<SessionError>` 桥接，移除与 core 重复的 `AgentError` / `ToolError` / `SessionError` / `ConfigError` 4 个变体

ADR 目的是把当前稳定性契约**文档化**，避免未来无意中破坏 wire-level 兼容性。

## Decision

### Stability Tiers

#### Tier 1 — Wire Stable（绝不能 break）

- `enum ErrorCode` 的所有现有变体（包括名称、serde snake_case 字符串、`Display` 输出）
- `UserError { code, message, result }` JSON 序列化形态（`{ "code": "...", "message": "...", "result"?: {...} }`）
- `ErrorCode::http_status()` 的 HTTP 状态映射
- `ApiResponse<T>` 的 wire envelope（`{ "status": "ok", "data": T }` / `{ "status": "err", "error": UserError }`）

#### Tier 2 — API Stable（可加，不可删/改）

- `enum Error` 的现有 33 变体（仅可加新变体，不允许重命名或修改现有 variant）
- `Error::code()` / `is_retryable()` / `is_rate_limited()` / `stream_error()` / `not_found()` / `validation()` / `internal()` / `already_exists()` / `invalid_item()` 方法名与签名
- 5 个高频 variant 的 `location` 字段名与 `CallSite` 类型别名
- `impl From<reqwest::Error>` / `impl From<serde_json::Error>` / `impl From<serde_yaml::Error>` / `impl From<synthia_session::SessionError>`

#### Tier 3 — Internal（可以 break，minor version bump 即可）

- 5 个高频 variant 之外的 28 个 variant 内部字段
- 5 个高频 variant 的 `Display` 格式（除 `(at {location})` 后缀外）
- 私有 helper 方法
- 内部 anyhow 路径（crate 内部开发便利）

### 演进规则

1. **加新 ErrorCode 变体**：必须走 RFC + ADR，但**不需要 major version bump**（`#[non_exhaustive]` 保证向后兼容）
2. **改/删 ErrorCode 变体**：major version bump + 通知所有 synthia-server / synthia-web 下游
3. **加新 Error 变体**：minor version + ADR 记录用途
4. **改/删 Error 变体**：major version bump
5. **helper 方法加新变体**：minor version（`Error::xxx(msg)` 形式统一）
6. **改 helper 方法签名**：major version bump

### `#[track_caller]` 使用规范

- `#[track_caller]` 必须放在所有 **public** helper 构造函数上（`Error::not_found()` / `validation()` / `internal()` 等）
- `#[track_caller]` 必须放在 `From<reqwest::Error>` 上（外部错误的归一化入口）
- `#[track_caller]` **不要** 放在 variant 直接构造的 public 函数上（这会绕过 helper 模式）

### 边界规范

- **库层**：所有公开 API 必须返回 `synthia_core::Error` 或 `Result<T, syntha_core::Error>`，**不返回 anyhow::Error**
- **应用层**（synthia-server / synthia-cli）：允许 `anyhow::Result<T>`，但 axum handler 边界必须 `.map_err(ServerError::from)`
- **wire 边界**（HTTP / JSON-RPC）：axum handler 返回 `Result<T, ServerError>`，由 `IntoResponse` 转 HTTP 状态码

### 双错误模型现状（GreptimeDB 模式）

- 库层 12 个 crate：thiserror enum（33 变体 `core::Error` + 各 crate 自己的 enum）
- 应用层（synthia-server / synthia-cli）：可选 `anyhow::Result<T>` 用于 main 函数和一次性脚本
- 边界：从库层 `Result<T, MyError>` 转 `synthia_core::Error` 再转 `ServerError` → `IntoResponse`

## Alternatives Considered

### A. 引入 gix-error Exn 模式

**拒绝理由**：
- `Exn<E>` 不实现 `std::error::Error`，必须 `.into_error()` 转 `gix_error::Error`，多一层转换成本
- 失去 enum 的编译期 match，所有 `match err { Error::NotFound(_) => ... }` 失效
- `gix-error` 0.x 单一所有者（gitoxide 团队），生产 workspace 不应押注
- 现有 `ErrorCode` 已经承担 wire-level 分类功能，Exn 是叠加而非替代

### B. 引入 OpenDAL 双层 `ErrorKind + Error` 结构

**拒绝理由**：
- `Error` enum → struct，~17 个 `match` Error 点全部失效
- OpenDAL ErrorKind 是固定 12 个 storage 语义，synthia 有 18+ 业务子分类放不进
- synthia 现有 `ErrorCode` 已经做了"行为分类"层（等价 OpenDAL ErrorKind），不需要重复发明

### C. 引入 snafu 整体迁移

**拒绝理由**：
- 13-crate 全部 thiserror → snafu 改造，~390 调用点
- selector struct 污染命名空间
- 编译时间成本（proc macro 慢）
- 评估为 P3 候选，独立 spec 后再决策

### D. 全 anyhow 替代 thiserror

**拒绝理由**：
- anyhow 公开 API 不稳定（任何 downcast 都不保证）
- wire-level `ErrorCode` 无法静态映射
- 失去 enum 的编译期保护

### E. 删除 synthia-server `ServerError`

**拒绝理由**：
- 12 个 HTTP-specific variant（ServiceUnavailable / TooManyRequests 等）与 `crate::api::ErrorCode` 有差异
- 完全删除会破坏 5 个 handler 文件 + middleware/error_handler.rs
- P2 采用保守方案 B：保留 8 个核心 variant + 加 `#[non_exhaustive]` + `From` 桥接

## Consequences

### Positive

- 编译时间 +0（不引入新 crate）
- wire-level 稳定性保持（`ErrorCode` 加变体 non-breaking）
- 每个高频 variant 自动携带 call site location（`file:line`），线上 debug 快 50%
- 双错误模型边界明确：库层 core::Error + 应用层 anyhow，caller 可选转
- ServerError 缩减到 8 个 variant，加 `#[non_exhaustive]` 防止未来膨胀

### Negative

- 33 个 variant 中只有 5 个高频 variant 有 location（其他 28 个仍 String payload）
- 需要持续教育团队"用 helper 方法而非直接构造 variant"
- `Clone for SessionError/StoreError/StateMachineError` 是 manual impl（因为有 `serde_json::Error` / `anyhow::Error` 字段），不是 derive

### Neutral

- 现有 `match err { Error::NotFound(_) => ... }` 在 5 个高频 variant 上需要改成 `Error::NotFound { item, .. }`（P2.2 迁移已完成）
- `anyhow::Error` 仍允许在 synthia-session / synthia-context 内部使用（仅在 crate 边界要求转 `core::Error`）

## Implementation Status (P2)

| 阶段 | 内容 | 状态 | 日期 |
|---|---|---|---|
| P2.1 | `ErrorCode` 加 `#[non_exhaustive]` | ✅ 完成 | 2026-08-04 |
| P2.2 | 5 个高频 variant 加 `location` + helper 方法 | ✅ 完成 | 2026-08-04 |
| P2.3 | synthia-session 双错误模型修复（From 桥接） | ✅ 完成 | 2026-08-04 |
| P2.4 | synthia-server `ServerError` 加 `#[non_exhaustive]` + From 桥接 | ✅ 完成 | 2026-08-04 |
| P2.5 | 本 ADR 文档 | ✅ 完成 | 2026-08-04 |

## Future Work (P3+)

### P3-A: snafu 整体迁移（GreptimeDB / iroh 模式）

**触发条件**：业务子分类继续增长（>50 ErrorCode），或需要 `#[track_caller]` 自动 + selector pattern 解决 `#[from]` 冲突。

**前置评估**：
- 13-crate 全部 thiserror → snafu 改造
- selector struct 污染命名空间的影响评估
- 编译时间成本基准

### P3-B: OpenDAL 两层结构

**触发条件**：`Error::code()` 的 35-arm match 成为热点路径，或 `is_retryable()` 需要 status 字段支持。

**前置评估**：
- Error enum → struct 的迁移成本（~17 个 match 点）
- `From<X>` impl 全部重写的工作量
- 与 synthia 现有 ErrorCode 的双层关系

### P3-C: 库层 snafu + 应用层 anyhow 混合

**触发条件**：synthia-session / synthia-context 的 anyhow 应用层规模继续扩大，需要正式承认。

**前置评估**：
- 当前 9 + 15 = 24 个 anyhow 文件的改造范围
- `#[stack_trace_debug]` proc-macro 引入成本

## References

- [Microsoft REST API Guidelines §5.1](https://github.com/microsoft/api-guidelines/blob/master/Guidelines.md)
- [Google AIP-193 Errors](https://google.aip.dev/193)
- [aws-smithy-rs RFC-0022 Error Context](https://smithy-lang.github.io/smithy-rs/design/rfcs/rfc0022_error_context_and_compatibility.html)
- [GreptimeDB Error Handling in Rust (2024-05-07)](https://greptime.com/blogs/2024-05-07-error-rust)
- [iroh Error Handling Blog (2025-08-22)](https://www.iroh.computer/blog/error-handling-in-iroh)
- 内部文档: `docs/architecture/error-ecosystem-comparison.md` (P2 决策论据)