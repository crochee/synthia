# ADR-0012: 集中 `Clock` / `IdGen` 抽象（替换散落的 chrono/ulid 调用）

- **Status**: Accepted
- **Date**: 2026-09-12
- **Related**: AGENTS.md §3.7（架构原则）, `synthia-core/src/clock.rs`,
  `synthia-core/src/idgen.rs`

## Context

之前 Synthia 里两件看似无关的事都直接调用底层 crate API，导致
**测试与生产路径必须分别维护**：

1. **墙钟时间**：代码到处是 `chrono::Utc::now()` / `Utc::now()`，
   没法在测试里注入固定时间。
2. **唯一 ID**：几个组件各自构造 `ulid::Generator::new()`，且
   部分组件走 `uuid::Uuid::new_v4()`，**风格不统一**。

更严重的是 [`synthia_agent::agent::ReActAgent::with_clock`] 的
实现已经把这个模式做出来了，但用了
`Arc<dyn Fn() -> chrono::DateTime<chrono::Utc> + Send + Sync>` 的
裸闭包类型 — 每个新组件要复用就得重复抄一遍这一行签名。

```rust
// 之前（散落、不可复用、签名冗长）：
clock: Arc<dyn Fn() -> chrono::DateTime<chrono::Utc> + Send + Sync>,

// 之后（trait + 默认实现 + 共享新类型）：
clock: synthia_core::SharedClock,
```

## Decision

### D1. `synthia-core` 新增两个 trait + 三个具体实现

```rust
// crates/synthia-core/src/clock.rs
pub trait Clock: Send + Sync {
    fn now(&self) -> chrono::DateTime<chrono::Utc>;
}

#[derive(Debug, Default, Clone)]
pub struct SystemClock;          // 生产：chrono::Utc::now()

#[derive(Debug, Clone)]
pub struct FixedClock { now: DateTime<Utc> }  // 测试：钉死时间

#[derive(Clone)]
pub struct SharedClock(Arc<dyn Clock>);         // 共享新类型
```

```rust
// crates/synthia-core/src/idgen.rs
pub trait IdGen: Send + Sync {
    fn next_id(&self) -> String;
}

pub struct UlidGenerator;        // 生产：ulid crate
pub struct SequenceGenerator;     // 测试：id-0, id-1, …
pub struct SharedIdGen(Arc<dyn IdGen>);
```

### D2. 替换散落调用

- `synthia-agent::ReActAgent::with_clock` 改接 `SharedClock`，
  旧 `Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>` 形式作废。
- 新组件不再允许直接 `chrono::Utc::now()` / `ulid::Generator::new()`，
  必须经过 `SharedClock` / `SharedIdGen`。
- 现有的"caller-driven"模型（`synthia-scheduler`、`synthia-mcp` 的
  supervisor — 它们的 `tick(now)` / `tick(now)` 都让调用方传
  `now`）已经满足抽象；不强制改成 `SharedClock`，保持简洁。

### D3. prelude 暴露

`synthia::prelude` 增加：

```rust
pub use crate::core::{
    Clock, FixedClock, IdGen, SequenceGenerator,
    SharedClock, SharedIdGen, SystemClock, UlidGenerator,
    // 原有：AtomicCancelToken, CancelToken, Error, Result
};
```

调用方写 `use synthia::prelude::*;` 就能拿到全部时间/ ID 词汇，
不需要再 `use chrono` / `use ulid`。

### D4. 生产与测试的最少用法

```rust
// 生产
let clock = SharedClock::system();
let ids = SharedIdGen::ulid_prefixed("run");
// 整个 agent 用同一个 Arc<SharedClock>

// 测试
let clock = SharedClock::fixed_from_rfc3339("2026-01-01T12:00:00Z");
let ids = SharedIdGen::sequence(); // → "id-0", "id-1", …
```

## Consequences

### 正面
- **测试时序变成 1 行**：`SharedClock::fixed_at(t)` / `fixed_from_rfc3339(s)`。
- **生产与测试同源代码路径**：所有时间戳都走 `clock.now()`，
  不会出现"测试代码忘了 stub 某个 `Utc::now`"的隐性 bug。
- **ID 风格统一**：session/run/event 都用同一 `SharedIdGen`，
  日志里能直接按 ID 前缀过滤（`run-*` / `sess-*`）。
- **依赖收敛**：以前测试代码需要拉 `chrono` / `ulid` 才能
  注入 stub，现在 trait 已经由 `synthia-core` 提供，应用
  可以完全不直接依赖这两个 crate。
- **乐高可换**：用户可以接 NTP/PTP 时钟，或 UUIDv7/Snowflake
  ID 生成器，仅实现一个 trait。

### 负面
- **一个破坏性改动**：`ReActAgent::with_clock` 签名变了 —
  从 `Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>` 改为
  `SharedClock`。生产代码仓库没有调用方（`grep with_clock`
  无匹配），影响范围 = 0。
- **trait 对象多一次 Arc 解引用**：SharedClock 在每个
  `now()` 上多一次虚表调用。纳秒级开销，agent 主路径热点
  在 LLM IO 与 tool dispatch，完全不可测。

### 风险
- **遗漏改造点**：分散在 16 个 crate 的 `chrono::Utc::now()`
  调用，本次仅改了 `synthia-agent::re_act` 一处。剩余点要么
  (a) 已经是 caller-driven 模式（scheduler/mcp supervisor），
  (b) 在生产代码非时序敏感点（日志、debug 输出），不需要
  注入。完整审计在后续 PR。
- **ULID 时钟漂移**：ULID 自带时间戳，如果 `FixedClock` 测试
  不替换 ULID 生成器，timestamp 部分会暴露真实时间。
  `SequenceGenerator`（测试用 ID）天然规避了这一点；生产
  `UlidGenerator` 沿用系统时间，符合 ULID 规范。

## Alternatives Considered

### A. 继续使用裸闭包类型（不动）
**拒绝**：每个新组件都要复制签名；没有类型化方法（`fixed_at`
shorthand）；调用方代码 `Arc::new(|| Utc::now())` 噪声大。

### B. 把 chrono::Utc 抽到 typed-builder
**拒绝**：chrono 已经是 typed-builder（`Utc.with_ymd_and_hms`），
再包一层是过度抽象。

### C. 用 trait `Now` / `MakeId`（更短的 trait 名）
**拒绝**：`Clock` / `IdGen` 与 traitclaw / pi-ai 等社区项目对齐，
更利于外部读者理解。

## Implementation Plan

- [x] `synthia-core/src/clock.rs` 实现 `Clock` + `SystemClock` +
      `FixedClock` + `SharedClock`
- [x] `synthia-core/src/idgen.rs` 实现 `IdGen` + `UlidGenerator` +
      `SequenceGenerator` + `SharedIdGen`
- [x] `synthia/src/prelude.rs` 增加 prelude 暴露
- [x] `synthia/src/core.rs` 模块文档更新
- [x] `synthia-agent::ReActAgent::with_clock` 迁移到 `SharedClock`
- [x] 单元测试通过（synthia-core 13 + synthia-agent 291）
- [x] `cargo check -p synthia --all-features` 通过
- [ ] 后续 PR：全仓 grep `Utc::now` / `ulid::Generator::new`
      逐处审计替换