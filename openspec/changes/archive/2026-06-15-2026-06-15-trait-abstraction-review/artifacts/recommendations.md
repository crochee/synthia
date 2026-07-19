# Recommendations: trait-abstraction-review

> Generated: 2026-06-14
> Source: `trait-inventory-classified.md` (56 traits) + 15 deep reviews

## Summary

| Category | Count | % of total |
|----------|-------|------------|
| KEEP | 29 | 52% |
| KEEP-dead? | 8 | 14% |
| REVIEW | 3 | 5% |
| REMOVE_CANDIDATE | 16 | 29% |
| **Total** | **56** | **100%** |

## Deep review summary (15 candidates, all 4-party reviewed)

| # | trait | review verdict | 4-party consensus |
|---|-------|----------------|-------------------|
| 01 | `Job` | KEEP | 4-0 (active 9-dyn plugin) |
| 02 | `Policy` | KEEP | 4-0 (PBAC core) |
| 03 | `SteeringChannel` | KEEP | 4-0 (active 14-dyn, P7 依赖) |
| 04 | `AuditWriter` | REMOVE_CANDIDATE | 4-0 (1 impl + 0 dyn = YAGNI) |
| 05 | `EventStream` | REMOVE_CANDIDATE | 4-0 (1 impl + 0 dyn) |
| 06 | `Retryable` | REMOVE_CANDIDATE | 4-0 (已 2026-06-15 验证: no-op wrapper, 非递归) |
| 07 | `PersistenceService` | KEEP | 3-1 (建议拆分为 3 trait) |
| 08 | `DoomLoopHandler` | REMOVE_CANDIDATE | 2-2 (倾向 REMOVE) |
| 09 | `ShellExecutor` | KEEP | 3-1 (安全边界) |
| 10 | `SkillMatcher` | REMOVE_CANDIDATE | 3-1 (YAGNI) |
| 11 | `ShellExecutor` (README) | REMOVE_CANDIDATE | 4-0 (污染 grep 信号) |
| 12 | `SkillProvider` | REVIEW | 4-0 (P1 拆分, 10 方法违反 ISP) |
| 13 | `SessionManager` | REVIEW | 4-0 (P0 拆分, 12 方法 + 与 PersistenceService 重叠) |
| 14 | `SessionWriter` | REMOVE_CANDIDATE | 3-1 (1 真实 impl 仅 NoOp) |
| 15 | `SkillActivator` | KEEP | 4-0 (活跃 2-dyn 依赖注入) |

## KEEP 典型代表

- `ModelProvider` (13 impl, 6 方法, 43 dyn) — LLM 抽象核心
- `Tool` (43 impl, 7 方法, 10 dyn) — Agent 工具系统
- `AgentHook` (13 impl, 8 方法, 4 dyn) — Hook 系统
- `PromptSection` (16 impl, 3 方法, 3 dyn) — 上下文注入
- `CompactionProvider` (4 impl, 1 方法, 16 dyn) — 上下文压缩
- `ModelRouter` (6 impl, 3 方法, 12 dyn) — 模型路由
- `Trigger` (4 impl, 2 方法, 11 dyn) — Job 触发
- `Job` (1 impl, 3 方法, 9 dyn) — 任务调度
- `SteeringChannel` (1 impl, 4 方法, 14 dyn) — 实时 steering
- `SkillActivator` (1 impl, 1 方法, 2 dyn) — 任务派发
- `Policy` (1 impl, 4 方法, 3 dyn) — PBAC 核心
- `ShellExecutor` (1 impl, 2 方法, 0 dyn) — 安全边界

## REVIEW (需拆分) — 3 个

- `SkillProvider` (10 方法违反 ISP) — P1 拆为 3 trait (Reader/Writer/VectorIndex)
- `SessionManager` (12 方法 + 与 PersistenceService 重叠) — P0 拆/合
- `PersistenceService` (7 方法, 强烈建议拆 3 trait) — 与 SessionManager 同步处理

## REMOVE_CANDIDATE — 16 个

| 类别 | trait | 备注 |
|------|-------|------|
| 1-impl + 0-dyn 纯预留 | `AuditWriter` | 后端扩展点但无切换需求 |
| 1-impl + 0-dyn 纯预留 | `EventStream` | SSE/WS 切换但 WS 未启动 |
| 1-impl + 0-dyn 纯预留 | `Retryable` | **已 2026-06-15 处理**: 确认 no-op wrapper, 已删除 (Sub-task A) |
| 1-impl + 0-dyn 纯预留 | `DoomLoopHandler` | 边界投票, 倾向移除 |
| 1-impl + 0-dyn 纯预留 | `SkillMatcher` | BM25 是合理默认 |
| 1-impl + 0-dyn NoOp 主导 | `SessionWriter` | NoOp 是唯一 impl |
| README 重复 | `ShellExecutor` (README.md) | 清理 grep 污染 |
| 其他 1-impl + ≤2 call | `McpClient`, `RiskEvaluator`, `AuditLogger`, `ContextService`, `PersistenceService` (争议), `ShellExecutor` (mod.rs) | 见分类 |

## KEEP-dead? — 8 个 (需进一步调查)

| trait | file:line | 说明 |
|-------|-----------|------|
| `AsyncPolicy` | `crates/synthia-core/src/pbac/policy.rs:353` | Policy 子 trait, 0 impl, 可能未来用 |
| `ColdRetrieval` | `crates/synthia-memory/src/traits.rs:24` | 冷存储检索, 0 impl |
| `HotMemoryFile` | `crates/synthia-memory/src/traits.rs:36` | 热存储文件, 0 impl |
| `EpisodicPersistence` | `crates/synthia-memory/src/traits.rs:53` | 持久化, 0 impl |
| `ContextCompaction` | `crates/synthia-memory/src/traits.rs:69` | 上下文压缩, 0 impl |
| `CompactionWriter` | `crates/synthia-context/src/traits.rs:19` | 写入, 0 impl |
| `McpClientFacade` (types.rs) | `crates/synthia-mcp/src/types.rs:95` | 重复定义 #1 (已 2026-06-15 删除) |
| `McpClientFacade` (traits.rs) | `crates/synthia-mcp/src/traits.rs:16` | 重复定义 #2 (已 2026-06-15 删除) |

**已验证 (2026-06-15)**: `McpClientFacade` 在两个 `pub mod` (types + traits) 中各自定义为 `pub trait`,**并非编译错误** — Rust 允许不同 module path 下的同名 trait (`synthia_mcp::types::McpClientFacade` 与 `synthia_mcp::traits::McpClientFacade` 是不同路径)。实际是**语义重复** (签名不同, 都 0 impl + 0 call site)。两 trait 已 2026-06-15 删除 (Sub-task B)。

## Future refactor candidates (种子索引)

| Priority | Trait | Reason | Deep review |
|----------|-------|--------|-------------|
| **P0** | `Retryable` 移除 | 已 2026-06-15 处理 (no-op wrapper) | [06-Retryable.md](deep-reviews/06-Retryable.md) |
| **P0** | `McpClientFacade` 重复 | 已 2026-06-15 处理 (非编译错误, 语义重复) | (无 deep review, 直接识别) |
| **P0** | `SessionManager` 拆分 | 12 方法 + 与 PersistenceService 重叠 (Sub-task C 实施) | [13-SessionManager.md](deep-reviews/13-SessionManager.md) |
| **P1** | `SkillProvider` 拆分 | 10 方法违反 ISP | [12-SkillProvider.md](deep-reviews/12-SkillProvider.md) |
| **P1** | `PersistenceService` 拆分 | 7 方法 + 2 泛型,建议拆 3 | [07-PersistenceService.md](deep-reviews/07-PersistenceService.md) |
| **P2** | `AuditWriter` 移除 | 1 impl + 0 dyn = YAGNI | [04-AuditWriter.md](deep-reviews/04-AuditWriter.md) |
| **P2** | `EventStream` 移除 | 同上 | [05-EventStream.md](deep-reviews/05-EventStream.md) |
| **P2** | `DoomLoopHandler` 移除 | 1 impl + 0 dyn | [08-DoomLoopHandler.md](deep-reviews/08-DoomLoopHandler.md) |
| **P2** | `SkillMatcher` 移除 | 1 impl + 0 dyn | [10-SkillMatcher.md](deep-reviews/10-SkillMatcher.md) |
| **P2** | `SessionWriter` 移除 | 1 真实 impl 仅 NoOp | [14-SessionWriter.md](deep-reviews/14-SessionWriter.md) |
| **P2** | `ShellExecutor` README 清理 | 污染 grep | [11-ShellExecutor-README.md](deep-reviews/11-ShellExecutor-README.md) |
| **P2** | `KEEP-dead?` 8 个 trait 调查 | 0 impl + 0 dyn | (无 deep review, 需具体调查) |

## Out-of-scope (本次 research 不实施任何重构)

本 change 是 **research-only**,**不修改** `src/` 任何业务代码。所有 P0/P1/P2 候选是**未来 OpenSpec change 的种子**。
