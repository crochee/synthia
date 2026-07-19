# Design: p2-trait-cleanup

> 2026-06-15 — 12 P2 trait 清理的设计决策

## Sub-task A: 4 纯 YAGNI 移除

**DoomLoopHandler** (`crates/synthia-agent/src/doom_loop_handler.rs:71`)

- 删除: `pub trait DoomLoopHandler: Send + Sync { ... }` (整个 trait + impl + struct `DefaultDoomLoopHandler` 一起)
- 文件剩余: 仅保留 `DoomLoopConfig` 和 `doom_loop_detected` 函数 (无 trait 依赖)
- 验证: `grep -rn 'DoomLoopHandler' crates/ --include='*.rs'` → 0 命中 (除 `lib.rs` 的 `pub mod doom_loop_handler;`)

**AuditWriter** (`crates/synthia-agent/src/audit.rs:17`)

- 删除: trait `AuditWriter`, impl for `FileAuditWriter`
- 保留: `FileAuditWriter` struct + 内部方法 (改为 inherent) + 测试
- 验证: `grep -rn 'AuditWriter' crates/ --include='*.rs'` → 仅文件内 self-reference

**EventStream** (`crates/synthia-server/src/event_stream.rs:64`)

- 删除: trait `EventStream`, impl for `SseEventStream`
- 保留: `SseEventStream` struct (方法改为 inherent) + `EventBroadcaster` (无 trait 依赖)
- `lib.rs` 移除 `pub use event_stream::{EventStream, ...}`

**SkillMatcher** (`crates/synthia-skill/src/matcher.rs:9`)

- 删除: trait `SkillMatcher`, impl for `BM25Matcher`
- 保留: `BM25Matcher` struct (方法改为 inherent)
- `lib.rs` 移除 `pub use matcher::SkillMatcher`

## Sub-task B: 死模块 + ShellExecutor 移除

**McpClient + 整个 mcp_bridge 模块** (`crates/synthia-agent/src/mcp_bridge.rs`)

- **关键发现**: `pub mod mcp_bridge` 在 `synthia-agent/src/lib.rs:26`,**但 `grep -rn 'mcp_bridge' crates/` 0 外部引用**
- 整个模块孤儿 (227 行): `McpClient` trait + `McpTool` struct + `McpBridgeClient` struct + `McpBridge` struct + 3 测试
- 操作: 删除整个 `mcp_bridge.rs` 文件 + 从 `lib.rs` 移除 `pub mod mcp_bridge;`
- `McpBridgeClient::call_tool` 当前返回 `"not implemented"` — 进一步确认死代码

**ShellExecutor (mod.rs)** (`crates/synthia-agent/src/shell/mod.rs:84`)

- 删除: `pub trait ShellExecutor`
- 保留: `LocalShellExecutor` struct (方法改为 inherent) + `pub use local::LocalShellExecutor`
- README 重复定义属 Sub-task D 范围

## Sub-task C: 4 dyn → 具体类型

**RiskEvaluator** (`crates/synthia-core/src/pbac/evaluation.rs:225`)

- 删除: trait `RiskEvaluator`, impl for `StandardRiskEvaluator`
- `PolicyEvaluator` struct:
  - 字段: `risk_evaluator: Option<Box<dyn RiskEvaluator>>` → `Option<Box<StandardRiskEvaluator>>`
- `PolicyEvaluator::with_risk_evaluator<R: RiskEvaluator + 'static>` → `with_standard_risk_evaluator(StandardRiskEvaluator)` (inherent)
- `with_audit_logger` 同步处理

**AuditLogger** (`crates/synthia-core/src/pbac/evaluation.rs:229`)

- 删除: trait `AuditLogger`, impl for `ConsoleAuditLogger`
- `PolicyEvaluator` struct: 字段 `audit_logger: Option<Box<dyn AuditLogger>>` → `Option<Box<ConsoleAuditLogger>>`
- 内部使用点 `if let Some(ref logger) = self.audit_logger { logger.log_indeterminate(...) }` → `logger.log_indeterminate(...)` 直接调用

**ContextService** (`crates/synthia-context/src/service.rs:85`)

- 删除: trait `ContextService`, impl for `DefaultContextService`
- `DefaultContextService` 保留,方法改为 inherent
- `AgentDependencies`:
  - 字段: `context_service: Option<Arc<dyn ContextService>>` → `Option<Arc<DefaultContextService>>`
  - `with_context_service` 改为 `with_default_context_service(Arc<DefaultContextService>)`

**SessionWriter** (`crates/synthia-context/src/session_writer.rs:6`)

- 删除: trait `SessionWriter`, impl for `NoOpSessionWriter`
- `&dyn SessionWriter` 参数 → `&NoOpSessionWriter`

## Sub-task D: PersistenceService 拆分 + README 清理

**PersistenceService** (`crates/synthia-session/src/service.rs:20`)

- 7 方法 trait → `Store` 上的 inherent 方法
- 影响: 13 个内部 UFCS 调用 (`PersistenceService::save_session(&store, ...)` → `store.save_session(...)`)
- 公共 API 破坏: `synthia_session::PersistenceService` 不再可导入
- `tests/reexport_policy.rs` 需更新 (`use synthia_session::PersistenceService` 删)

**ShellExecutor README 清理** (`crates/synthia-agent/src/shell/README.md:37`)

- 删除 README 中的 `pub trait ShellExecutor: Send + Sync { ... }` 重复定义
- 保留 README 中其他地方对 trait 的引用(改为指向 inherent 方法或纯说明)

## 与 P0/P1 决策对齐

- 同样的"1 trait per commit"模式(P0 P1 每个 trait 1 commit)
- 同样的 4-party 共识要求 (3-1 minimum, 4-0 preferred)
- 同样的"trait 删除后 internal UFCS → inherent"路径
- 同样的"公共 API 破坏透明记录"原则

## 风险评估

| Sub-task | 风险等级 | 主要风险 |
|----------|----------|----------|
| A | 低 | 几乎纯删除,0 编译错误风险 |
| B | 中 | mcp_bridge 模块删除需验证无内部引用 |
| C | 中-高 | 4 个公共方法签名改变,可能级联编译错误 |
| D | 中 | PersistenceService 13 行 UFCS 替换 |

## 不做的事

- 不引入新抽象
- 不添加 metrics / observability
- 不修复任何未相关 bug
- 不优化性能
- 不拆分 SessionManager (P0 已删) / SkillProvider (P1 已删) / 其他已处理 trait
