# Brainstorm: p0-trait-review-remediation

> 4 派对抗原始记录 (怀疑派 / 架构派 / 生产派 / 简化派)
> 编排: 2026-06-15 21:30 (Asia/Shanghai)
> 议题: trait-abstraction-review P0 列表的 3 个最高优先级发现,如何收尾

## 第 0 步: 议题澄清 (Socratic 分解)

**Q1: 3 个 P0 项是"同时做"还是"分批做"?**

- 怀疑派: 分批, 一个 commit 一个语义, 便于 bisect / revert
- 架构派: 同步, 1 个 change 1 个 spec, 整体看待
- 生产派: 同步 (3 个都是小改, 1 个 change 更清晰)
- 简化派: 分批 (YAGNI, 不需要"统一提交"的仪式感)

**用户决策**: 1 个 change, 3 个 sub-task (产派 + 架构派胜)

**Q2: Sub-task C 的 SessionManager 怎么拆?**

详见 design.md §4, 用户决策 C-1 (拆 2 trait) 取代 4-party 共识 C-3 (合并) / C-1 (拆 2) 的 2-2 tiebreaker。

**Q3: McpClientFacade 删哪个?**

- 怀疑派: 都删, 0 用户的 trait = 垃圾
- 架构派: 留 1 个 (在 traits.rs, 用 `ToolDefinition`/`ToolOutput` 更现代)
- 生产派: 都删, 减少 API surface
- 简化派: 都删, YAGNI

**共识**: 3-1 都删 (留空方法不实现是 worse than 删)

## 第 1 派: 怀疑派 (Skeptic)

**核心立场**: "no user = no trait" — 任何 0 impl + 0 call site + 0 dyn
的 trait 都应删除, 除非有明确未来扩展点。

**对 Sub-task A**: ✅ 同意删除。`Retryable` 的 impl 调用 inherent
method 是 dead wrapper, 删除是清理而非破坏。**已验明无递归风险**。

**对 Sub-task B**: ✅ 同意都删。两个 0 用户的 trait 是双倍 dead code,
不是单选, 是双删。Rust 允许不同 module path 同名 = 更危险, 因为
reviewer 容易误以为其中一个是 canonical 版本。

**对 Sub-task C**: ⚠️ 反对拆, 主张**合并到 PersistenceService**。
- 拆 2 trait 增加 trait 数量, 违反 KISS
- `SessionReader` 的 5 个方法 + `PersistenceService` 的 4 个读方法
  是同一组操作的两种 API 风格
- **怀疑派的合并方案**: 删 `SessionManager`, 把它的 12 个方法选择性
  合并进 `PersistenceService`, 最终 trait 9-11 方法

**vs 简化派的差异**: 怀疑派允许"对 trait 排序", 简化派主张"先删再说"。
本次立场: 怀疑派偏保守 (合并), 简化派偏激进 (拆 + 各自精简)。

## 第 2 派: 架构派 (Architect)

**核心立场**: "trait 数量应反映 capability 边界" — 一个 trait 一个清晰
职责, 读和写是不同职责。

**对 Sub-task A**: ✅ 同意删除。`Retryable` 的错误已经在 `Error` 类型上,
没有自己的 capability 边界, 抽 trait 是 over-engineering。

**对 Sub-task B**: ⚠️ 主张**留 1 个**。删 `types.rs` 版本, 保留
`traits.rs` 版本 (更现代, 用 `ToolDefinition`/`ToolOutput` 类型)。
理由: 未来 `McpClient` 需要 facade, 提前占位避免再设计。

**对 Sub-task C**: ✅ 同意拆 2 trait (Reader/Writer)。
- 12 方法 trait 是 ISP 违反, 任何 consumer 都只用到一部分
- 拆 2 trait 让泛型参数更精确 (`R: SessionReader` vs
  `R: SessionReader + SessionWriter`)
- 不合 PersistenceService 的理由: `PersistenceService` 是更底层
  (按 `&str` session_id 操作), `SessionReader` 是按 `SessionConfig`
  操作, 接口粒度不同

**vs 怀疑派的差异**: 架构派接受"未来需要"占位, 怀疑派要求"现在有用户"。

## 第 3 派: 生产派 (Production)

**核心立场**: "0 行为变化 + 0 风险 = 0 阻力" — 改动必须不能引入
回归, 每个 sub-task 独立可回滚。

**对 Sub-task A**: ✅ 同意删除。inherent method 已是 canonical, 删 trait
不会改变任何调用语义。`cargo test` 必过。

**对 Sub-task B**: ✅ 同意都删。0 impl + 0 调用 = 删掉必不破坏任何
生产路径。生产派比架构派更激进, 因为生产派"零回归"原则不允许
"为了未来占位" 留 dead code。

**对 Sub-task C**: ✅ 同意拆 2 trait (C-1)。生产派**不**选 C-3 (合并),
因为合并会改动 `PersistenceService` 公共 API, 涉及更多下游 consumer。
拆 2 trait 是局部改动, 只动 `SessionManager` 引用方, 不影响
`PersistenceService` 的用户。

**关键论据**: `cargo check --workspace` 在 commit 后立即反馈, 若
发现 call site 漏改, 立刻可定位 (rustc error E0277: trait bound
not satisfied), 不用走 e2e 测试。

## 第 4 派: 简化派 (Simplifier)

**核心立场**: "YAGNI 极致" — 不留 1 个备用 trait, 不留"未来可能用"
的设计, 实际不需要时全部删干净。

**对 Sub-task A**: ✅ 同意删除。与 3 派一致。

**对 Sub-task B**: ✅ 同意都删。与怀疑派 + 生产派一致 (3-1)。

**对 Sub-task C**: ⚠️ 比怀疑派更激进 — 主张**直接删 `SessionManager`
trait, 不用 trait bound, 改用具体类型 `Store`**。
- 1 impl + trait = over-abstraction
- trait bound 增加函数签名复杂度, 但 `Store` 已是具体类型, 用它
  更直接
- **简化派的最简化方案**: 删 `SessionManager`, 改所有 call site
  使用 `&Store` 具体类型, 不再用泛型参数

**vs 怀疑派的差异**: 怀疑派接受"保留 1 个 trait" (合并到
PersistenceService), 简化派主张"具体类型优先, trait 退场"。

## 第 5 步: 4-party tiebreaker 与用户决策

| 议题 | 怀疑 | 架构 | 生产 | 简化 | tie | **用户** |
|------|------|------|------|------|-----|----------|
| A: Retryable | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 | **REMOVE** |
| B: McpClientFacade | 都删 | 留 1 | 都删 | 都删 | 3-1 | **都删** |
| C: SessionManager | 合 Persistence | 拆 2 | 拆 2 | 删 trait 用具体 | 2-2 | **拆 2 (C-1)** |

**用户决策的考虑**:
- 4 派对 A 一致, 直接走
- 4 派对 B 倾向都删 (3-1), 走多数
- 4 派对 C 2-2 tie, 用户手动 tiebreak 选 C-1:
  - 理由 1: 简化派的"删 trait 用具体类型"会改变 `Store` 作为
    内部实现的契约, 涉及更多 call site
  - 理由 2: 怀疑派的"合 PersistenceService"会改 `PersistenceService`
    公共 API, 扩大 blast radius
  - 理由 3: 架构派+生产派的"拆 2 trait"是局部改动, 风险最低
  - 理由 4: 与 6 月规律"研究→清理"一致, 拆 trait 是研究产出的具体落实

## 第 6 步: 风险评估共识

| Sub-task | 风险等级 | 主要风险 | 缓解 |
|----------|----------|----------|------|
| A | 极低 | 0 调用方 | 0 缓解需要 |
| B | 极低 | 0 调用方 + 模块隔离 | 0 缓解需要 |
| C | 中 | call site 漏改 | `cargo check --workspace` 立即反馈, 失败可定位 |

**共识**: 4 派一致同意"先做 A (低风险热身), 再做 B (低风险熟练),
最后做 C (中风险慎做)"。3 sub-task 顺序排列既是风险递增, 也是
工作量递增 (A: 5 行, B: 27 行, C: 30+ 行 call site 改动)。

## 第 7 步: 不做的事共识 (4-0)

- ❌ 不重命名 `SessionReader`/`SessionWriter` (避免无意义 churn)
- ❌ 不重新审视其他 13 个 REMOVE_CANDIDATE (留给下个 P0 batch)
- ❌ 不动 `archive/2026-06-15-trait-abstraction-review/` 内容
- ❌ 不创建"为了未来"的新 trait
- ❌ 不改 trait 之外的清理 (如 `SessionConfig` 重命名等)

## 第 8 步: 实施节奏共识

- 每个 sub-task 独立 commit, commit message 包含 sub-task 标识
  (A/B/C), 便于 bisect
- 每个 sub-task 完成后 `cargo check` + `cargo test` 立即验证
- C 完成后追加 `cargo clippy` + `cargo +nightly fmt --all`
- 全 sub-task 完成后归档 (openspec archive)
