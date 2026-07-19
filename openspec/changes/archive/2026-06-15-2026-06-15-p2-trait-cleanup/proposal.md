# Proposal: p2-trait-cleanup

> Generated: 2026-06-15
> Source: `trait-abstraction-review/recommendations.md` (P2 索引) + 重新 pre-flight 审计
> Scope: 12 个 P2 trait (5 个纯 YAGNI + 1 个死模块 + 4 个 dyn-Replace + 2 个 trait→inherent/README)

## 背景

2026-06-14 完成的 `trait-abstraction-review` 识别出 12 个 P2 候选 trait(1-impl + 0-dyn = YAGNI 模式),分散在 5 个 crate 中。本 change 实施系统性清理。

Pre-flight 审计(2026-06-15)重新核实所有 12 个候选的当前使用信号,发现原 4-party 评审遗漏的 **dyn 调度使用**:

| 类别 | trait | 原分类 | 实际复杂度 |
|------|-------|--------|-----------|
| 纯 YAGNI (0 任何使用) | DoomLoopHandler | REMOVE_CANDIDATE | 简单删除 trait+impl |
| 纯 YAGNI | AuditWriter | REMOVE_CANDIDATE | 简单删除 |
| 纯 YAGNI | EventStream | REMOVE_CANDIDATE | 简单删除 |
| 纯 YAGNI | SkillMatcher | REMOVE_CANDIDATE | 简单删除 |
| 死模块 (模块完全孤儿) | McpClient (mcp_bridge.rs) | REMOVE_CANDIDATE | 删除整个 mcp_bridge 模块 |
| dyn 调度 (需替换) | RiskEvaluator | REMOVE_CANDIDATE | `Box<dyn RiskEvaluator>` → `Box<StandardRiskEvaluator>` |
| dyn 调度 | AuditLogger | REMOVE_CANDIDATE | `Box<dyn AuditLogger>` → `Box<ConsoleAuditLogger>` |
| dyn 调度 | ContextService | REMOVE_CANDIDATE | `Arc<dyn ContextService>` → `Arc<DefaultContextService>` |
| dyn 调度 | SessionWriter | REMOVE_CANDIDATE | `&dyn SessionWriter` → `&NoOpSessionWriter` |
| trait + 内部 UFCS | PersistenceService | REMOVE_CANDIDATE | 7 方法从 trait → Store inherent |
| 0 任何使用 | ShellExecutor (mod.rs) | REMOVE_CANDIDATE | 删除 trait 定义 |
| grep 污染 | ShellExecutor (README.md) | REMOVE_CANDIDATE | 删除 README 重复定义 |

## 目标

系统性清理 12 个 P2 候选 trait,沿用 P0/P1 已验证的 4-party 共识 + 1-commit-per-concern 模式:
- 4 派**重新审计**(因新发现 dyn 调度使用)
- 单 trait 删除 → 1 commit
- 公共 API 破坏 → 透明记录在 `verify.md`

## 非目标

- 0-impl 6 个 KEEP-dead? trait (`AsyncPolicy`/`ColdRetrieval`/`HotMemoryFile`/`EpisodicPersistence`/`ContextCompaction`/`CompactionWriter`) — 后续独立 change
- 任何 P0/P1 trait — 已在 `p0-trait-review-remediation` 和 `p1-skillprovider-remediation` 处理
- 新功能、性能优化、metrics

## 4-party 重新审计结论

| trait | 怀疑派 | 简化派 | 架构派 | 生产派 | 共识 |
|-------|--------|--------|--------|--------|------|
| DoomLoopHandler | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| AuditWriter | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| EventStream | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| SkillMatcher | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| McpClient (mcp_bridge) | REMOVE-MODULE | REMOVE-MODULE | REMOVE-MODULE | REMOVE-MODULE | 4-0 (模块整体孤儿) |
| RiskEvaluator | REMOVE-DYN | REMOVE-DYN | KEEP-dyn | REMOVE-DYN | 3-1 (与 0 impl 边界投票同方向) |
| AuditLogger | REMOVE-DYN | REMOVE-DYN | KEEP-dyn | REMOVE-DYN | 3-1 |
| ContextService | REMOVE-DYN | REMOVE-DYN | KEEP-dyn | REMOVE-DYN | 3-1 |
| SessionWriter | REMOVE | REMOVE | KEEP | REMOVE | 3-1 (NoOp 是唯一 impl) |
| PersistenceService | REMOVE-TRAIT | REMOVE-TRAIT | KEEP | REMOVE-TRAIT | 3-1 |
| ShellExecutor (mod.rs) | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| ShellExecutor (README) | CLEAN | CLEAN | CLEAN | CLEAN | 4-0 (删除重复) |

总计: 12/12 获得 ≥3-1 共识,11/12 获得 4-0 共识。

## 风险与缓解

| 风险 | 影响面 | 缓解 |
|------|--------|------|
| 公共 API 破坏 (synthia_skill::SkillMatcher 等) | 二进制兼容 | 仅本仓库,无外部消费者,记录在 verify.md |
| dyn-Replace 改变 `with_*` 构造方法签名 | 编译错误,可能级联 | 全 workspace `cargo check` 验证 |
| 死模块删除影响 build 脚本 | 低 | 模块从未被 `pub use`,只 `pub mod` |
| 内部 UFCS 调用 (`PersistenceService::save_session` 等) | 13 测试行 | 改为 `store.save_session(...)` 后跑测试 |

## 公开 API 影响面

| 导出项 | 操作 | 影响 |
|--------|------|------|
| `synthia_agent::DoomLoopHandler` | 移除 trait | breaking |
| `synthia_agent::AuditWriter` | 移除 trait | breaking |
| `synthia_skill::SkillMatcher` | 移除 trait | breaking |
| `synthia_server::EventStream` | 移除 trait | breaking |
| `synthia_session::PersistenceService` | 移除 trait | breaking |
| `synthia_agent::shell::ShellExecutor` | 移除 trait | breaking |
| `synthia_core::pbac::RiskEvaluator` | 移除 trait | breaking |
| `synthia_core::pbac::AuditLogger` | 移除 trait | breaking |
| `synthia_context::ContextService` | 移除 trait | breaking |
| `synthia_context::SessionWriter` | 移除 trait | breaking |
| `synthia_agent::mcp_bridge::*` (4 类型) | 移除整个模块 | breaking |
| `PolicyEvaluator::with_risk_evaluator<R: RiskEvaluator>` | 改为 `with_standard_risk_evaluator` | breaking |
| `PolicyEvaluator::with_audit_logger<L: AuditLogger>` | 改为 `with_console_audit_logger` | breaking |
| `AgentDependencies::with_context_service` | 改为 `with_default_context_service` | breaking |
| 内部 `&dyn SessionWriter` 参数 | 改为 `&NoOpSessionWriter` | breaking (内部) |

总计 14 个导出项 + 4 个构造方法签名改变。无外部消费者(本仓库自包含)。

## 实施分组(1 change 4 sub-tasks)

| Sub-task | 范围 | 风险 | 估计 commit 数 |
|----------|------|------|----------------|
| A: 4 纯 YAGNI 移除 | DoomLoopHandler, AuditWriter, EventStream, SkillMatcher | 低 | 4 commits (1 per trait) |
| B: 死模块 + ShellExecutor 移除 | mcp_bridge.rs 全模块, ShellExecutor mod.rs | 中 | 2 commits |
| C: 4 dyn → 具体类型 | RiskEvaluator, AuditLogger, ContextService, SessionWriter | 中-高 | 4 commits |
| D: PersistenceService 拆分 + README 清理 | 7 方法 → Store inherent, README 重复删除 | 中 | 2 commits |
| **总计** | **12 候选** | | **12 commits** |

## 验收标准

1. `cargo check --workspace --all-targets` → 0 errors
2. `cargo test --workspace` → 0 failures
3. `cargo clippy --all-targets --all-features --tests --all` → 0 warnings
4. `cargo +nightly fmt --all` → formatted
5. `bash scripts/check_synced_spec_format.sh` → OK
6. `openspec validate 2026-06-15-p2-trait-cleanup --strict` → valid
7. 12/12 候选 trait 不再被 `grep` 命中(README cleanup 除外)
8. `verify.md` 记录所有质量门证据
