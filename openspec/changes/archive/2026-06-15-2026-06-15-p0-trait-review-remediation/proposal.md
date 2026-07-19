# Proposal: p0-trait-review-remediation

## Why

2026-06-15 trait-abstraction-review (已归档于
`openspec/changes/archive/2026-06-15-2026-06-15-trait-abstraction-review/`)
产出的 `recommendations.md` 列出 3 个 P0 项需要立即处理。本 change 集中
收尾这 3 个 P0 发现,把抽象审视的"种子索引"变成"实际清理",落实
项目 memory 的核心原则:

> "Architectural trait abstractions should be re-evaluated 6 months
> after bug fixes and code deduplication"

距上次大规模 bug fix + dedup (synthia-cli 死文件清理 11 个 / -2572
行) 已 6 个月,正是"先修 critical bug + duplicate code"窗口。

**3 个 P0 项的实际严重性 (本次 context exploration 验证)**:

| P0 项 | 严重性 (实查) | 状态 |
|-------|----------------|------|
| `Retryable` trait | 实际是 no-op wrapper (委托给 Error inherent),**非无限递归** (Rust 方法解析优先 inherent)。dead code 性质,需删除 | 高 |
| `McpClientFacade` 重复 | 实际是模块内语义重复 (`types.rs` vs `traits.rs`,签名不同),**非编译错误**。Rust 允许不同 module path 同名 trait。两个都 0 impl + 0 call site,需删除 | 中 |
| `SessionManager` 拆分 | 12 方法 + 与 `PersistenceService` 7 方法有重叠,违反 ISP。用户已选 C-1 方案 (拆 2 trait: SessionReader + SessionWriter) | 中-高 |

本 change 不重新做研究审视 (那已由 trait-abstraction-review 完成),而是
**实施**已识别的清理项,延续 6 月规律 (research → cleanup → research)。

## What Changes

**核心交付物** (按 sub-task 顺序, 各自独立 commit + review):

- **Sub-task A** — 删除 [retry.rs:6-14](file:///home/crochee/workspace/synthia/crates/synthia-provider/src/retry.rs#L6-L14) 的 `Retryable` trait + impl。`Error::is_retryable()` 已经是 inherent method,所有调用方自动改用 inherent 版本。**-9 行,0 行为变化**
- **Sub-task B** — 删除 [types.rs:95](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/types.rs#L95) 和 [traits.rs:16](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/traits.rs#L16) 的两个 `McpClientFacade` 重复定义。**-26 行,0 行为变化**
- **Sub-task C** — 把 [session.rs:110](file:///home/crochee/workspace/synthia/crates/synthia-session/src/session.rs#L110) 的 `SessionManager` (12 方法) 拆为 `SessionReader` (6 方法) + `SessionWriter` (5 方法)。impl (1 个,推测 `Store`) 同时实现 2 个 trait。所有 call site 同步更新 trait bound。

**架构决策** (基于 4-party 对抗):

| Sub-task | 怀疑派 | 架构派 | 生产派 | 简化派 | **决定** |
|----------|--------|--------|--------|--------|----------|
| A: Retryable | REMOVE | KEEP+警告 | REMOVE | REMOVE | **REMOVE** (3-1) |
| B: McpClientFacade ×2 | REMOVE both | 留 1 | REMOVE both | REMOVE both | **REMOVE both** (3-1) |
| C: SessionManager | 合 PersistenceService | 拆 2 | 拆 2 | 合 PersistenceService | **拆 2 (C-1)** (用户已选,4-party 倾向 2-2) |

**不做**:
- ❌ 不创建新 trait (除了 Sub-task C 的 2 个衍生)
- ❌ 不修改 trait 公共 API 之外的代码
- ❌ 不动 `archive/2026-06-15-2026-06-15-trait-abstraction-review/` (只读)
- ❌ 不重新审视其他 trait (那是 trait-abstraction-review 的工作)

## Capabilities

### New Capabilities

- `p0-trait-review-remediation`: 收尾 trait-abstraction-review P0 列表的 3 个最高优先级发现,提供清晰的 trait 边界

### Modified Capabilities

无 (本 change 不改现有 capability;只清理 trait 实现细节)

## Impact

- **代码**:
  - `crates/synthia-provider/src/retry.rs` -9 行 (删除 trait + impl)
  - `crates/synthia-mcp/src/types.rs` -12 行 (删除 McpClientFacade)
  - `crates/synthia-mcp/src/traits.rs` -15 行 (删除 McpClientFacade)
  - `crates/synthia-session/src/session.rs` 重构 (拆 1 trait → 2 trait, 总行数 +10)
  - 估算 net: **~ -25 行** + 1 impl 改 2 impl
- **OpenSpec**:
  - 新增 `openspec/changes/2026-06-15-p0-trait-review-remediation/` 目录
  - 6 份文档 (proposal/design/tasks/verify/brainstorm + 1 spec)
- **测试**: 全部 sub-task 完成后 `cargo test --workspace` 必须 0 regression
- **依赖**: 无新增 crate
- **风险**: 低-中 (3 个独立 sub-task 各自风险隔离)
  - A: 极低 (0 调用方)
  - B: 极低 (0 调用方,模块路径冲突已隔离)
  - C: 中 (拆 trait 涉及 call site 更新,但 1 impl + 推测少量调用方)
- **回滚**: 3 sub-task 各自独立 commit,任一可单独 revert

## 验证 (粗估)

- `cargo check --workspace`: 0 errors
- `cargo test --workspace`: 0 regressions (call site 更新到位)
- `cargo clippy --all-targets --all-features --tests --all`: 0 warnings
- `cargo +nightly fmt --all`: 无 diff
- `openspec validate 2026-06-15-p0-trait-review-remediation --strict`: valid
