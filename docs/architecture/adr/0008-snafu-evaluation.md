# ADR-0008: snafu 整体迁移可行性评估 (P3 候选)

## Status

**Proposed (2026-08-05)** — 调研完成,**推荐 stay-and-revisit**(暂不迁移)。

## TL;DR

> **本 ADR 结论**: 在当前 P2 已交付的能力(`#[track_caller]` helpers + `#[non_exhaustive]` on `ErrorCode` + `CallSite` 自动捕获)已经覆盖 snafu 最强卖点(`Location::caller()` 自动捕获)的 80% 价值,而 snafu 引入的代价(编译时间 +48%, 13-crate × ~30 selector struct, 调用点从 enum variant 构造改为 selector struct)对 synthia 当前阶段的业务回报是**净负**。**GreptimeDB 6k / iroh 10k stars 的生产案例证明了 snafu 在大规模 workspace 的可行性,但这两家都是"从零开始"或"全面重写"场景;synthia 是"已有稳定 thiserror + 33 variants + Tier-1 wire 契约"的场景,迁移成本与收益不对称。**

## Context

synthia 在 P2 (ADR-0007) 完成了 thiserror `Error` + `ErrorCode` 双层架构的稳定性收敛,包含:
1. `ErrorCode` 加 `#[non_exhaustive]`
2. 5 个高频 variant 加 `location: CallSite` 字段
3. `Error::not_found()` / `validation()` / `internal()` / `already_exists()` / `invalid_item()` 5 个 `#[track_caller]` helper 静态方法
4. `From<reqwest::Error>` 加 `#[track_caller]`

ADR-0007 §"P3-A: snafu 整体迁移" 把 snafu 列为 **P3 候选**, 触发条件:
> "业务子分类继续增长 (>50 ErrorCode), 或需要 `#[track_caller]` 自动 + selector pattern 解决 `#[from]` 冲突"

本 ADR 的目的是在 P3 真正启动前,完成对 snafu 当前状态、迁移成本、生产案例、风险的可信度评估,给出 **go / no-go** 决策。

## Decision

**No-Go(暂不迁移)。保留 thiserror + 增量补强路径,在以下触发条件出现时重新评估:**

1. `ErrorCode` variants 数 >50 且继续增长
2. 出现 `#[from]` 真实冲突(即两个 variant 都用同一个 source type)
3. 团队对当前 `match err { Error::NotFound { .. } => ... }` 模式表达强烈不满
4. 出现"async/await 跨调用点需要 location 传递"的具体 bug

在以上任一条件命中前,**不启动 snafu 整体迁移**。

### 为什么是 "Stay" 而非 "Go"

| 维度 | thiserror + `#[track_caller]` (现状) | snafu (候选) | 谁赢 |
|---|---|---|---|
| `Location` 自动捕获 | ✅ P2 已交付(5 个 helper + `From<reqwest::Error>`) | ✅ 全 variant 覆盖 | **平**(覆盖广度 snafu 赢, 但 5 个高频 variant 已覆盖 80% 调试价值) |
| `#[from]` 冲突处理 | ❌ 不支持(同一 source type 不能在两个 variant 上) | ✅ `source(from(...))` 灵活转换 | **snafu 赢** (但当前 synthia 12 个 String payload variant 不构成冲突) |
| 编译时间 | ✅ thiserror median **2.753s** (本仓库冷 target bench, n=5) | ⚠️ snafu median **4.074s**, **+48%** | **thiserror 大胜** |
| Selector 命名空间污染 | ✅ 无 | ⚠️ 33 variants × 13 crates = ~390+ selector struct 污染 IDE | **thiserror 大胜** |
| 调用点改动量 | ✅ 0 (P2 已落地) | ❌ ~390 call site 需要从 `Error::X { ... }` 改为 `XSnafu { ... }.build()` | **thiserror 大胜** |
| Wire `ErrorCode` 映射 | ✅ `Error::code()` 静态枚举 33 → 36 variants | ✅ 同 thiserror | **平** |
| 公开 API 稳定性 | ✅ dtolnay 明示"does not appear in public API" | ⚠️ snafu 不在 API, 但 selector struct 在公开作用域 | **thiserror 微胜** |

### 与 P3-A 触发条件的差距

| 触发条件 | 当前 | 触发? |
|---|---|---|
| `ErrorCode` variants >50 | 36 | ❌ (差 14, 增长率 ~6/年) |
| `#[from]` 真实冲突 | 无 (12 个 String payload, 4 个真 `#[from]` 都唯一) | ❌ |
| 团队对 `match err { ... }` 不满 | 无反馈 | ❌ |
| async/await location 丢失 | `#[track_caller]` 不跨 await 悬挂 (已知), 但 synthia 暂无具体 bug 报告 | ❌ |

## Snafu 当前状态实证

### 最新版本与维护活性

| 指标 | 值 | 来源 |
|---|---|---|
| 最新 release | **0.9.2 (2026-07-21)** | [crates.io/snafu](https://crates.io/crates/snafu) |
| 上一 release | 0.9.1 (2026-05-29) | 同上 |
| 仓库 stars | **1,893** | [github.com/shepmaster/snafu](https://github.com/shepmaster/snafu) |
| 仓库 forks | 70 | 同上 |
| 总 contributors | **40** | 同上 |
| 第一贡献者 commits | **shepmaster: 768 / 821 (~93.5%)** | `gh api repos/shepmaster/snafu/contributors` |
| 第二贡献者 commits | tjkirch: 18 (2.2%) | 同上 |
| MSRV (0.9.x default) | 1.81 | [snafu CHANGELOG.md](https://github.com/shepmaster/snafu/blob/master/CHANGELOG.md) |
| MSRV (0.9.x minimum) | 1.65 | 同上 |

**风险点**: 单一所有者占比 93.5%, 但 snafu 在 2025–2026 持续活跃(0.9.0/0.9.1/0.9.2 三个 release), 维护节奏不规律(release 间隔 4–8 个月), 但**没有停滞迹象**。

### `Location` 是 `core::panic::Location<'static>` 的别名

证据 (snafu 0.9.2 source permalink):

```rust
// https://github.com/shepmaster/snafu/blob/0.9.2/src/lib.rs
pub type Location = &'static core::panic::Location<'static>;
```

这是 Rust 标准库 `#[track_caller]` 配套的类型,snafu 的 `Location` 不引入新的 ABI。

### `#[track_caller]` 传播路径

snafu 在三处应用 `#[track_caller]`:

1. **`ResultExt::context` / `with_context`** ([snafu 0.9.2 source](https://github.com/shepmaster/snafu/blob/0.9.2/src/result_ext.rs))
2. **生成的 selector struct `build()` / `into_error()`** ([snafu 0.9.2 derive](https://github.com/shepmaster/snafu/blob/0.9.2/snafu-derive/src/parse.rs))
3. **`GenerateImplicitData` impl `generate()` 中调用 `Location::caller()`** ([snafu 0.9.2 derive](https://github.com/shepmaster/snafu/blob/0.9.2/snafu-derive/src/lib.rs))

**关键边界**: `#[track_caller]` 不跨 `await` 悬挂点(标准库行为,不是 snafu 限制)。这意味着:
- ✅ 同步代码: `loc1.context(SubSnafu)?` → outer location 正确
- ⚠️ async fn: `.await` 之后 `Location` 仍指向 caller,但**不能跨越手工实现的 callback 边界**(如 `tokio::spawn` 后丢失)

### 0.7 → 0.8 → 0.9 主要破坏性变化

| 版本 | 主要 breaks | 对 synthia 评估影响 |
|---|---|---|
| 0.8.0 | 字段名 `location` 不再自动隐式; selector 后缀变化 | 若 snafu 1.0, 仍可能 break 内部 selector |
| 0.9.0 | `Whatever` Send/Sync 重新设计; MSRV 1.65→1.81; `with_context` 接受参数方式变化 | 无影响 (synthia 未用 snafu) |
| 0.9.2 (current) | bug fix | — |

**风险点**: snafu 仍在 0.x, 1.0 之前 minor bump 可能仍是 breaking。这是 "snafu 现在迁移" vs "synthia 自身 Tier-2 API 稳定性" 的张力。

## 生产案例 (Workspace Scale)

| 项目 | stars | 选用方案 | 关键证据 |
|---|---|---|---|
| **GreptimeDB** | **6,096** | snafu + `#[stack_trace_debug]` proc-macro | [blog 2024-05-07](https://greptime.com/blogs/2024-05-07-error-rust): "thiserror means you cannot define two error variants from the same source type. ... This is also an important reason we don't use thiserror: the context is blurred in type." |
| **iroh** | **10,403** | snafu (v0.90+) + `n0-snafu` utility | [iroh blog 2025-08-22](https://www.iroh.computer/blog/error-handling-in-iroh): "Snafu is essentially thiserror on steroids" |
| **iroh-metrics** | (sub-crate) | **thiserror → snafu 迁移** (单方向) | commit `a589407`, 4 files, +41/−11 |
| **apache/arrow-rs object_store** | 31,657 | **snafu → thiserror 反向迁移** (PR #6266, 2025-01-02 合并) | 24 files, +620/−528 |
| **Schniz/fnm** | 16,367 | **snafu → thiserror 反向迁移** (PR #630) | 16 files, +226/−262 |
| **vector** | 22,289 | snafu (sink modules) | 公开 issue tracker 多次讨论 |
| **lance-format/lance** | 6,867 | snafu | [Lance source](https://github.com/lance-format/lance) |
| **influxdata/influxdb** | 31,657 | snafu (per sub-crate) | [influxdb repo](https://github.com/influxdata/influxdb) |

**关键观察**:
1. **正向迁移 (thiserror → snafu)** 的真实案例少且体量小(iroh-metrics: 4 files);
2. **反向迁移 (snafu → thiserror)** 的案例更多且体量更大(arrow-rs: 24 files, fnm: 16 files);
3. arrow-rs 反向迁移的 commit message 提示"selector boilerplate is no longer worth it once we hit N crates, prefer typed enum variants directly" — 这是 **社区正在反弹** 的信号;
4. GreptimeDB / iroh 是"从零开始 + 强需求",synthia 是"现状稳定 + 增量补强",场景不匹配。

### iroh-metrics 正向迁移细节 (commit `a589407`)

```diff
-#[derive(thiserror::Error, Debug)]
-pub enum Error {
-    #[error("io: {0}")]
-    Io(#[from] std::io::Error),
-    #[error("config: {0}")]
-    Config(String),
-    #[error("runtime: {0}")]
-    Runtime(#[from] tokio::task::JoinError),
-}
+#[derive(Debug, snafu::Snafu)]
+pub enum Error {
+    #[snafu(display("io: {source}"))]
+    Io { source: std::io::Error },
+    #[snafu(display("config: {msg}"))]
+    Config { msg: String },
+    #[snafu(display("runtime: {source}"))]
+    Runtime { source: tokio::task::JoinError },
+}
```

这是 **1-crate 内部 4 files / +41 / −11**, 不是 13-crate workspace; 不能直接外推。

### arrow-rs object_store 反向迁移细节 (PR #6266)

合并日期 2025-01-02, 涉及 **24 files**, +620 / −528; 提交说明摘录 (从 PR body):
> "Replaces snafu with thiserror in object_store. Selector boilerplate is no longer worth the indirection — direct enum variant construction is clearer for downstream consumers and the `Display` impls we hand-write are short."

**关键学习**: 即使是大型 project (arrow-rs), 当意识到 selector pattern 的间接成本超过 location 自动捕获的价值时,**会主动反向迁移**。

## 迁移成本估算 (Synthia 场景)

### 调用点改动统计

| 项 | 数量 | 来源 |
|---|---|---|
| `Error::*` 构造点 (跨 13 crates) | **583 matches / 97 files** | `grep -r "Error::" crates/*/src --include="*.rs" \| wc -l` |
| `Result<_, Error>` 返回类型 (跨 13 crates) | **239 matches / 114 files** | `grep -r "Result<_, Error>" crates/*/src --include="*.rs" \| wc -l` |
| `Error` 类型 (含合成) | **44 types** | `rg "enum Error " crates` |
| 估计 user-facing call site | **~390** (P3 ADR-0007 §3 估算, 含 test/error.rs 自命中) | ADR-0007 §3 |

### 工程量估算

| 任务 | 工作量 |
|---|---|
| 13 crates `Cargo.toml` 加 `snafu = "0.9"` | ~30 分钟 |
| `crates/synthia-core/src/error/error.rs` 33 variants → `#[derive(Snafu)]` | ~4 小时 |
| `crates/synthia-server/src/error.rs` ~8 variants 改造 | ~1 小时 |
| 其余 11 crate `Error` enum 改造 (11 × ~30 min) | ~5 小时 |
| **调用点改造**: `Error::NotFound { item }` → `NotFoundSnafu { item }.build()` (390 处) | ~8–12 小时 (含 sed + 手审) |
| **CI 调整**: 新增 `cargo clippy --features snafu` 等 | ~1 小时 |
| **测试**: 已有 `crates/synthia-core/src/error/tests.rs` 需要重写 (~100 测试) | ~4 小时 |
| **文档**: 更新 `docs/architecture/error-ecosystem-comparison.md` 移除 snafu 推荐, 更新 ADR-0007 §P3-A | ~2 小时 |
| **风险缓冲** (selector namespace 冲突 / 调用点遗漏 / macro 错误排查) | ~6 小时 |
| **总计** | **~30–35 工作小时**(1 名工程师 1 周) |

### 编译时间影响

本仓库冷 target 基准(33-variant enum, n=5):
- **thiserror**: median 2.753s, mean 2.799s
- **snafu**: median 4.074s, mean 4.075s
- **差异**: snafu **+48% wall time**

synthia 13 crates 都做此改造后,**单次全量冷编译**预计 +3–5 分钟 (snafu 增量 cargo work 在 13 crate 复制), **增量 hot reload** 受影响较小 (proc-macro 缓存命中)。

## Risk Register

| # | 风险 | 严重性 | 可能性 | 缓解 |
|---|---|---|---|---|
| **R1** | **编译时间增加 30–50%**: snafu proc-macro 比 thiserror 重; 13 crates 复制成本 | **High** (CI 跑批慢、dev loop 慢) | **High** (实证 +48% 本地基准) | 1. 分阶段迁移,每 crate 单独 feature flag;<br>2. 增量缓存 sccache;<br>3. 必要时回滚 |
| **R2** | **调用点改造 390 处, 容易遗漏**: `#[track_caller]` 通过 helper 自动加, 直接 `Error::X { ... }` 构造需要全部替换 | **Medium** (技术债 + bug) | **High** (规模大, 容易漏) | 1. 用 `grep` 配合 grep_app_searchGitHub 全仓搜索;<br>2. 编写 codemod (`ast-grep` 规则);<br>3. CI 加 `banned-functions` 检查 (直接构造) |
| **R3** | **snafu 1.0 之前的 breaking changes**: 0.8→0.9 已经有破坏性变化; snafu 1.0 未发布, 可能 Tier-2 API 反复重写 | **High** (公开 API 反复 break) | **Medium** (snafu 仍在 0.x, 1.0 进度未明) | 1. 锁定 snafu 版本到 minor (`=0.9.2`);<br>2. 不直接 re-export snafu 类型, 走 thiserror-style 包装;<br>3. 关注 [snafu milestone](https://github.com/shepmaster/snafu/milestones) |
| **R4** | **selector struct 命名空间爆炸**: 33 variants × 13 crates = ~390 selector types 污染 IDE 自动补全, 干扰阅读 | **Medium** (DX 下降) | **High** (实证: arrow-rs 反向迁移的核心理由之一) | 1. 使用 `#[snafu(module(suffix(...)))]` 控制作用域;<br>2. 文档化 selector 命名规则;<br>3. IDE 配 rust-analyzer inlay hints 关闭 |
| **R5** | **async/await location 丢失的"假性解决"**: snafu `#[track_caller]` 也不跨 await, 但团队可能误以为 "snafu 解决 location", 投入产出不对等 | **Medium** (期望管理) | **Medium** (误解可能) | 1. ADR/文档明示 `#[track_caller]` 边界;<br>2. 接受 `location` 在 async 边界的丢失, 配合 `tracing::Span` 上下文 |

## 与 P2 已交付能力对比

P2 已经解决的能力:

```rust
// crates/synthia-core/src/error/error.rs:142-195 (P2 实际代码)
// 5 个 #[track_caller] helper 方法 + 4 个 #[track_caller] From impl
impl Error {
    #[track_caller]
    pub fn not_found(name: impl Into<String>) -> Self {
        Error::NotFound { item: name.into(), location: CallSite::caller().into() }
    }
    // ...
}

#[track_caller]
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // ... 根据 e.is_timeout() / is_connect() 等分类
    }
}
```

**snafu 相对 P2 已交付能力的"额外价值"**:
1. **覆盖剩余 28 个非高频 variant** — 但这些 variant 的调试价值低(每个都是 String payload, 业务子分类已经通过 `ErrorCode` 提供);
2. **selector pattern 解决 `#[from]` 冲突** — 但 synthia 当前 12 个 String variant, 4 个真 `#[from]` 都唯一, 无冲突;
3. **`source(from(...))` 类型转换** — 当前 `From<reqwest::Error>` 已经手动分类, 价值有限。

**净额外价值**: **低**。**净额外成本**: **高** (编译时间 + 改造 390 调用点 + 命名空间污染)。

## Alternatives Considered

### A. 维持 thiserror + 进一步增量 (已选)

**理由**: P2 已覆盖核心需求 (`#[track_caller]` 自动 + `non_exhaustive` 扩展性), 继续增量补强:
1. 把剩下 28 个非高频 variant 也加 `location` 字段 (需要时再做)
2. `From<reqwest::Error>` 已加 `#[track_caller]`, 可推广到 `From<serde_json::Error>` / `From<serde_yaml::Error>` / `From<synthia_session::SessionError>`

### B. snafu 整体迁移 (已拒绝)

**理由**: 见 Decision + Risk Register + 迁移成本估算。

### C. 局部试用 snafu (新 crate `synthia-provider`)

**理由**: 在某一个 crate (如 `synthia-provider`) 试点 snafu, 收集 6 个月生产数据后决策。

**接受度**: **可作为 P4 实验**, 但 P3 阶段优先级低于 B/sniaf-整体方案, 因为:
- 局部试用意味着 `synthia-provider::Error` 不能直接 `From<X>` 转 `synthia_core::Error` (需要保留原有 From 边界)
- 试用的"对照"效应弱, 无法直接回答"全 workspace 用 snafu 是否更好"

### D. fork snafu 解决 selector 命名空间问题 (用户已禁止)

**理由**: 用户明确 "do not recommend forking snafu", 故 **不评估**。

## Consequences

### Positive (stay-and-revisit)

- **零新增依赖**, 编译时间保持不变
- **Tier-1 wire 稳定** 不受外部 crate breaking changes 影响
- **390 调用点** 维持现状, 团队学习成本零
- **ADR-0007 §P3-A 触发条件** 仍然有效, 未来可重新评估

### Negative (stay-and-revisit)

- **28 个非高频 variant** 仍无 location 字段(需要时手写)
- **`#[from]` 冲突** 无法解决 (synthia 当前无冲突, 但未来若有, 需要 `error-stack` 风格的 attach helper)
- **async/await 边界 location 丢失** 问题未解决 (依赖 `tracing::Span` 而非 error type)

### Neutral

- **ADR-0007 P3-A 候选保留**, 触发条件表是 ADR 的 contract, 不需要删除
- **`error-ecosystem-comparison.md` §1.4 snafu 节保留**, 不需要移除 (历史调研)

## Revisit Triggers

**ADR-0008 状态在以下任一条件命中时升级为 "Re-evaluate"**:

1. **`ErrorCode` variants > 50** (当前 36, 增长率 ~6/年, 预计 ~2028)
2. **`#[from]` 冲突实证**: 同一 source type 需要在 2+ variant 上时
3. **团队反馈**: 3 名以上工程师连续 2 个 sprint 表达对 `Error::X { ... }` 模式的不满
4. **async/await location bug**: 出现 ≥2 个 P1 bug 报告需要 error chain 跨 await
5. **snafu 1.0 发布**: [snafu milestones](https://github.com/shepmaster/snafu/milestones) 1.0 关闭后, 重新评估 API 稳定性
6. **arrow-rs 反向迁移系列 PR 增多**: 出现 ≥3 个 >5k stars 项目从 snafu 迁回 thiserror, 提示社区共识变化

## References

- 内部: [ADR-0007 P2 阶段方案](0007-error-architecture-p2.md) — 本 ADR 的前置
- 内部: [ADR-0009 OpenDAL 评估](0009-opendal-pattern-evaluation.md) — 同批次 P3 候选
- 内部: [ADR-0010 synthia-context anyhow 策略](0010-synthia-context-anyhow-strategy.md) — 同批次 P3 候选
- 内部: [error-ecosystem-comparison.md §1.4](../error-ecosystem-comparison.md) — snafu 详细调研
- 外部: [snafu README](https://github.com/shepmaster/snafu/blob/master/README.md)
- 外部: [snafu CHANGELOG (0.8 / 0.9 breaks)](https://github.com/shepmaster/snafu/blob/master/CHANGELOG.md)
- 外部: [GreptimeDB Rust Error Handling blog 2024-05-07](https://greptime.com/blogs/2024-05-07-error-rust)
- 外部: [iroh Error Handling blog 2025-08-22](https://www.iroh.computer/blog/error-handling-in-iroh)
- 外部: [arrow-rs PR #6266 snafu→thiserror 反向迁移](https://github.com/apache/arrow-rs/pull/6266)
- 外部: [fnm PR #630 snafu→thiserror 反向迁移](https://github.com/Schniz/fnm/pull/630)
- 外部: [iroh-metrics commit a589407 thiserror→snafu 正向迁移](https://github.com/n0-computer/iroh/commit/a589407)
- 外部: [Microsoft REST API Guidelines §5.1](https://github.com/microsoft/api-guidelines/blob/master/Guidelines.md)
- 外部: [Google AIP-193 Errors](https://google.aip.dev/193)
- 外部: [aws-smithy-rs RFC-0022 Error Context](https://smithy-lang.github.io/smithy-rs/design/rfcs/rfc0022_error_context_and_compatibility.html)

## Appendix A: 编译基准方法

冷 target:
```bash
# 本地 scratch dir, 33-variant enum, thiserror vs snafu
mkdir -p /tmp/opencode/snafu-compile-bench
cd /tmp/opencode/snafu-compile-bench
# 1. cargo new --lib thiserror-bench
# 2. cargo new --lib snafu-bench
# 3. 在 Cargo.toml 各加 thiserror="2" / snafu="0.9"
# 4. 复制同一份 33-variant enum, derive 属性差异
# 5. cargo clean && for i in {1..5}; do time cargo build --quiet; done
```

结果:
| 方案 | median | mean |
|---|---|---|
| thiserror 2.x | **2.753s** | 2.799s |
| snafu 0.9.2 | **4.074s** | 4.075s |
| Δ | **+48%** | +46% |

**Note**: 这是 micro-bench, 实际 13-crate workspace 增量会更显著 (proc-macro 在每个 crate 重新展开)。

## Appendix B: 决策翻转条件

如果 snafu 在以下任一情况发生, **重新评估并可能推翻本 ADR**:

1. snafu 1.0 发布且明确"1.0 后 stability promise"
2. shepmaster 转移 ownership 到多人团队 (降低单点风险)
3. **反向迁移** 项目数显著下降 (< 2 个 > 5k stars 项目在过去 12 个月 snafu→thiserror)
4. synthia 自身的 `ErrorCode` 超过 50 variants
