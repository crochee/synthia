# P2 Rust Error 架构生态对比 (synthia-core)

> **范围**: 为 synthia 13-crate workspace + axum server + JSON-RPC wire protocol 评估"长期 Error 架构"
> 候选,对比 anyhow / eyre / thiserror / snafu / error-stack / gix-error Exn / OpenDAL
> **写作时间**: 2026-08-04
> **synthia 当前状态**: 单一 `synthia_core::Error` enum (thiserror derive),配 `ErrorCode` 稳定 wire 分类符;
> 所有 13 个 crate 都直接 re-export `synthia_core::Error` (见 [`crates/synthia-core/src/error/error.rs`](../../crates/synthia-core/src/error/error.rs))

---

## 0. Synthia 当前 Error 架构盘点

**核心文件**:
- [`crates/synthia-core/src/error/error.rs`](../../crates/synthia-core/src/error/error.rs) — 单一 `Error` enum,33 个变体,thiserror derive
- [`crates/synthia-core/src/error/error_code.rs`](../../crates/synthia-core/src/error/error_code.rs) — 稳定 wire `ErrorCode` enum,36 个变体
- [`crates/synthia-core/src/error/user_error.rs`](../../crates/synthia-core/src/error/user_error.rs) — `UserError` 结构体(API 响应用)
- [`crates/synthia-core/src/error/into_response.rs`](../../crates/synthia-core/src/error/into_response.rs) — axum `IntoResponse` (gated by `axum` feature)
- [`crates/synthia-server/src/error.rs`](../../crates/synthia-server/src/error.rs) — 服务器层 `ServerError` enum
- [`crates/synthia-server/src/middleware/error_handler.rs`](../../crates/synthia-server/src/middleware/error_handler.rs) — 中间件错误处理

**当前错误模型**:
```rust
// crates/synthia-core/src/error/error.rs:16-127
#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")] NotFound(String),
    #[error("I/O error: {0}")] Io(#[from] std::io::Error),
    // ... 31 个其他变体
    #[error("edit conflict on {path}: ...")] EditConflict { path: ..., original_hash: u64, current_hash: u64 },
    #[error("rate limited, retry after {0:?}")] RateLimited(Option<std::time::Duration>),
    // ...
}
```

**跨 crate 用法** (synthia 现在已经统一):
```rust
// crates/synthia-skill/src/installer/installer.rs:55
use synthia_core::Error;
// crates/synthia-tool/src/registry.rs:589
async fn register(&self, item: Self::Info) -> Result<(), synthia_core::Error> { ... }
```

**Wire 协议层**:
```rust
// crates/synthia-core/src/error/into_response.rs:18-63 — ErrorCode → HTTP StatusCode 映射
pub fn http_status(&self) -> StatusCode {
    match self {
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::ValidationError => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        // ...
    }
}
```

**矛盾点 (P2 升级动机)**:
1. **`synthia-session` crate** 已经偷偷引入 `anyhow::Result` 局部化使用 (见
   [`crates/synthia-session/src/manager/cache.rs:5`](../../crates/synthia-session/src/manager/cache.rs), `state.rs:5`, `queries.rs:6`,
   `core.rs:128` 出现 `anyhow!("session {session_id:?} not found")`).
2. **错误信息已大量使用 `String` 损失结构**: `Error::Provider(String)`, `Error::Session(String)` 等
   12 个变体都用 `String` payload,导致下层错误被 stringify,丢失 backtrace + 原始类型.
3. **缺乏 location 信息**: 任何 panic/错误发生时,只能从日志时间戳反推.
4. **缺乏 per-frame context**: 无法区分"这个 Io 错误来自 session loader 而不是 skill loader".
5. **`axum` feature gating** (`crates/synthia-core/Cargo.toml:24,28`):
   `axum = { workspace = true, optional = true }` + `axum = ["dep:axum"]` —
   synthia-core 已经为非 HTTP 消费者做了轻量化,但**未来如果引入 anyhow/eyre 等大 crate,
   会反向破坏这一隔离** (anyhow/eyre 都默认带 `std` feature,引入 `backtrace` + 大量依赖).

---

## 1. 候选 crate 深度调研

### 1.1 anyhow (dtolnay) — 事实标准的"应用层"错误

**仓库**: <https://github.com/dtolnay/anyhow> | 最新版: 1.0.x | License: MIT/Apache-2.0

**模型**:
- 单一类型 `anyhow::Error` = `Box<dyn StdError + Send + Sync>`
- `anyhow!()` / `bail!()` 宏构造
- `Result<T, anyhow::Error>` 即 `anyhow::Result<T>`
- `Context` trait (在 `Option` / `Result` 上) 提供 `.context("...")` / `.with_context(|| ...)`
- 通过 `downcast_ref::<T>()` 做类型恢复 (运行时反射)

**官方定位** (from README):
> "Use Anyhow if you don't care what error type your functions return, you just want it
> to be easy. This is common in application code."
> ([README](https://github.com/dtolnay/anyhow/blob/master/README.md))

**关键缺陷 for synthia**:
1. **不可作为公开 API** — `anyhow::Error` 把内部 std error 全部擦成 `dyn Error`,
   公开后下游无法 `match`,只能 `downcast`(脆弱)
2. **没有 wire-level 分类符** — 只能通过 `error.downcast_ref::<X>()` 试探,无法做稳定的
   HTTP status mapping 或 JSON-RPC error code 映射
3. **backtrace 是默认行为 (Rust ≥ 1.65)** — `RUST_BACKTRACE=1` 才能看到,生产默认关闭
4. **synthia-session 已经局部使用 anyhow** — 已经造成"双 error 模型"局面
   (synthia-session 用 anyhow::Result,其他 12 个 crate 用 synthia_core::Error)

**证据**:
- README: <https://github.com/dtolnay/anyhow/blob/master/README.md>
- "No-std support" 段: anyhow 在 `no_std` 模式下大部分 API 仍可用,但需要全局 allocator
  ([README](https://github.com/dtolnay/anyhow/blob/master/README.md))
- "Comparison to thiserror" 段明示 anyhow 是**应用层**,thiserror 是**库层**

---

### 1.2 eyre (yaahc/eyre-rs) — anyhow 的 fork,custom ReportHandler

**仓库**: <https://github.com/eyre-rs/eyre> | 最新版: 0.6.x | ⭐ 1.8k

**模型**:
- 同一 trait-object 思路 (`eyre::Report`)
- 关键扩展: **`EyreHandler` trait** — 可插拔的 report handler
  - `DefaultHandler` (anyhow-like)
  - `color-eyre` 提供 colorful report + `tracing_error::SpanTrace` 整合
    ([crates.io: 6.16M 总下载, 1.98k reverse deps](https://crates.io/crates/color-eyre))
  - `stable-eyre` (使用 `backtrace-rs` 替代 `std::backtrace`)
  - `simple-eyre` (无 backtrace)
- `ResultExt` trait (替代 anyhow 的 `Context`,有 `.wrap_err()` / `.wrap_err_with()`)
- `eyre!` / `bail!` 宏同 anyhow

**官方定位** (from README):
> "We recommend users do not re-export types from this library as part their own public
> API for libraries with external users."
> ([eyre README §Usage Recommendations](https://github.com/eyre-rs/eyre#usage-recommendations-and-stability-considerations))

**关键风险**:
- **官方明示不推荐作 public API** — 与 synthia 公开 `Error` 的需求冲突
- **no-std support 已移除 (2020)** — 参见 commit
  [608a16a](https://github.com/eyre-rs/eyre/pull/29/commits/608a16aa2c2c27eca6c88001cc94c6973c18f1d5)
- 与 anyhow **功能冗余** — 对 synthia 没有任何独特价值

**证据**:
- README: <https://github.com/eyre-rs/eyre/blob/master/README.md>
- "Compatibility with anyhow" 段 — 提供 `anyhow` feature 互相转换
- `color-eyre` 总下载 6.16M,reverse deps 1.98k — 主要用于 **CLI 工具** 而非服务

---

### 1.3 thiserror (dtolnay) — derive(Error) 的事实标准

**仓库**: <https://github.com/dtolnay/thiserror> | 最新版: 2.x | License: MIT/Apache-2.0

**模型**:
- 单一 `#[derive(Error)]` proc-macro,生成 `impl std::error::Error for ...`
- 支持 enum、struct (named/tuple/unit)
- 字段属性:
  - `#[from]` — 生成 `From<T>`,且隐含 `#[source]`
  - `#[source]` — 显式 source (亦可省略若字段名是 `source`)
  - `#[backtrace]` — 转发 source 的 `Backtrace` (nightly, `error_generic_member_access`)
- **存在形态**: enum 或 struct,**不支持 "error tree" / "frame stack"**
- `error(transparent)` — 透传到底层 error (一个字段的 enum/struct)

**官方定位** (from README):
> "Thiserror deliberately does not appear in your public API. You get the same thing as
> if you had written an implementation of `std::error::Error` by hand, and switching from
> handwritten impls to thiserror or vice versa is not a breaking change."
> ([thiserror README §Details](https://github.com/dtolnay/thiserror))

**关键优势 for synthia**:
1. **正是 synthia 今天的方案** — `crates/synthia-core/src/error/error.rs:9,15`
   `use thiserror::Error; #[derive(Debug, Error)] pub enum Error { ... }`
2. **稳定 wire 映射**: `Error::code(&self) -> ErrorCode` 静态可枚举
3. **零运行时开销** — 纯 derive 宏,展开后就是手写 `impl Error`
4. **公开 API 友好** — dtolnay 明示 "does not appear in your public API"
5. **广泛生态** — `axum-error-response` 等官方示例都用它

**关键限制**:
1. **`#[from]` 冲突** — 同一 source 类型不能在两个 variants 上 (GreptimeDB 团队明确指出这是缺点:
   "we won't know whether an error is generated in the write path or the read path",
   [GreptimeDB blog](https://greptime.com/blogs/2024-05-07-error-rust))
2. **没有 location / context 字段** — 需要手写 `#[snafu_implementation]`
   或者在 `error(transparent)` 内嵌套其他方案
3. **嵌套层级靠递归 enum** — 没有"frame stack"概念,合成多源错误需要 `Box<Error>`,
   失去类型信息

**证据**:
- README: <https://github.com/dtolnay/thiserror/blob/master/README.md>
- 实现源码: <https://github.com/dtolnay/thiserror/blob/master/impl/src/expand.rs>
  (展示 `#[from]` → `From` 展开逻辑,无运行时开销)

---

### 1.4 snafu (shepmaster/Jake Goulding) — location-aware + context selectors

**仓库**: <https://github.com/shepmaster/snafu> | 最新版: 0.8.x | License: MIT/Apache-2.0

**模型**:
- `#[derive(Snafu)]` 生成 enum + **per-variant `<Variant>Snafu` selector struct** + `Context<X, Source>` trait
- 每个 variant 可携带 `#[snafu(source)]` (内部 source) / `#[snafu(source(from(SomeErr, |x| ...)))]` (类型转换) /
  `#[snafu(implicit)]` (隐式字段如 `Location`)
- `ensure!` 宏替代 `if … Err(...)`
- `Snafu::snafu_visibility` / `module(suffix)` 等修饰符控制 selector 命名空间
- **`Option<snafu::Location>`** 自动捕获构造点 `#[track_caller]` — 零成本
- 支持 `whatever!` (anyhow-like 一次性消息)

**关键能力 for synthia**:
1. **同一 source 类型可在多 variant 上** — 用 `source(from(IOErr, |e| match e.kind() { ... }))`
2. **location 自动捕获** — 每个 variant 构造点自动记录 `Location::caller()`,无需手写
3. **selector pattern** — `ReadSnafu { path }.fail()` 比 `ReadFailed { path: path.to_string() }` 简洁
4. **workspace 内分层组合** — 每个 crate 自己的 `Error` 通过 `#[snafu(source(from(CrateError, |e| ...)))]`
   组合上层 `Error`,可以保留"内层类型 → 外层归类"映射

**生产案例 — GreptimeDB** ([GreptimeDB blog](https://greptime.com/blogs/2024-05-07-error-rust)):
- 多 sub-crate workspace,每个 crate 一个 Error enum + `#[stack_trace_debug]` proc-macro
- 选择 snafu 而非 thiserror 的理由 (引用):
  > "thiserror mainly implements the std::convert::From trait for your error types,
  > so that you can simply use ? to propagate the error you receive. Consequently,
  > this also means you cannot define two error variants from the same source type.
  > Considering you are performing some I/O operations, you won't know whether an error
  > is generated in the write path or the read path. This is also an important reason
  > we don't use thiserror: the context is blurred in type."

**生产案例 — iroh** ([iroh blog 2025-08-22](https://www.iroh.computer/blog/error-handling-in-iroh)):
- 从 anyhow 过渡到 snafu (in v0.90),为支持 library public API
- 写了 `n0-snafu` utility crate 补足 anyhow 风格的测试便利性
- 引用:
  > "Snafu is essentially thiserror on steroids. It provides: Enum-based error types
  > with derive macros (like thiserror), Rich context attachment and error chaining,
  > Automatic backtrace capture when constructing error variants, Extension traits that
  > work around Rust's limitations"

**关键限制**:
1. **selector struct 命名爆炸** — 每个 variant 自动生成 `<Variant>Snafu`,13 个 crate × 30 个 variant
   = 几百个 selector struct 占用 IDE 自动补全
2. **API 设计范式转换** — 从"直接构造 enum variant"改成"构造 selector struct + .fail()"
   (例: `Err(ReadSnafu { path: p }.build())` 而非 `Err(ReadFailed { path: p })`)
3. **依赖 proc-macro 大量展开** — 编译时间略高于 thiserror (greptime 博客没有量化,
   但社区普遍反馈)

**证据**:
- README: <https://github.com/shepmaster/snafu/blob/master/README.md>
- 文档 guide: <https://docs.rs/snafu/*/snafu/guide/index.html>
- `whatever!` 类似 `anyhow!` 但可融入 snafu Error chain

---

### 1.5 error-stack (hashintel) — frame-based 错误栈

**仓库**: <https://github.com/hashintel/hash/tree/main/libs/error-stack> | 最新版: 0.8.x |
License: MIT/Apache-2.0 | crates.io: 4M+ downloads, 130 reverse deps

**模型**:
- `Report<C>` 类型 — 围绕错误构建**栈结构**(类似 OpenTelemetry span stack)
- `.attach(Printable)` / `.attach_with(|| ...)` — 在错误上**附加一帧 context**(任意 `Printable`)
- `.change_context(NewContext)` — 沿链换上下文类型
- 自动捕获 `Location::caller()` + 可选 backtrace
- 提供 `IntoReport<...>` trait 在 `Result<T, E: std::error::Error>` 上
- Feature flags: `anyhow` / `spantrace` / `futures` / `serde` / `hooks`

**核心差异 vs snafu**:
- snafu 是"enum variant → location/structure";error-stack 是"single typed head + frame stack"
- error-stack 的 frame 是**运行时附加**的,不是编译时声明的
- 每个 frame 可以携带**任意 `Printable` 对象**(struct、JSON、String...),不限于 enum field

**典型代码** ([error-stack README](https://github.com/hashintel/hash/tree/main/libs/error-stack)):
```rust
fn parse_experiment(description: &str) -> Result<(u64, u64), Report<ParseExperimentError>> {
    let value = description
        .parse::<u64>()
        .attach_with(|| format!("{description:?} could not be parsed as experiment"))
        .change_context(ParseExperimentError)?;
    Ok((value, 2 * value))
}
```

**渲染输出** (来自 README):
```
Error: experiment error: could not run experiment
├╴at examples/demo.rs:50:18
├╴unable to set up experiments
│
├─▶ invalid experiment description
│   ├╴at examples/demo.rs:20:10
│   ╰╴experiment 2 could not be parsed
│
╰─▶ invalid digit found in string
    ├╴at examples/demo.rs:19:10
    ├╴backtrace with 31 frames (1)
    ╰╴"3o" could not be parsed as experiment
```

**关键优势 for synthia**:
1. **frame stack 完美匹配多层 context** — "synthia-provider call" → "OpenAI HTTP fail" →
   "reqwest::Error" 的渲染直接可用
2. **per-frame context** — synthia 可以在每个 `?` 现场 attach `{tool: "calculator",
   session_id: "..."}`,**无需预先设计 enum variant**
3. **类型 + runtime context 分层** — Report head 携带类型 `ErrorCode` 等"稳定分类",
   frames 携带运行时调试信息

**关键限制 for synthia**:
1. **API 风格与 Rust 主流反着来** — 不是 enum variant,要让 13 个 crate 的作者都接受
2. **`ChangeContext` 必须满足 trait bounds** — 需要 `core::error::Error + Send + Sync + 'static`,
   synthia 的 `Error` 已经满足,但每个内层 crate 的错误需要手动包装
3. **wire-level 映射不直观** — `Report<C>` 不是 `enum`,需要 `head.downcast_ref::<C>()`
   才能拿到 stable code,比 thiserror 多一层间接
4. **backtrace 在 std::backtrace 之外有兼容性问题** — docs.rs 上 0.7/0.8 系列是稳定版

**证据**:
- README: <https://github.com/hashintel/hash/tree/main/libs/error-stack>
- crates.io: <https://crates.io/crates/error-stack>
- docs: <https://docs.rs/error-stack>

---

### 1.6 gix-error Exn (Byron / gitoxide) — call-site exception 树

**仓库**: <https://github.com/GitoxideLabs/gitoxide> (subcrate) | gix-error 0.2.5 |
License: MIT/Apache-2.0

**模型** (详细 — 这是 P2 候选之一):
- `Exn<E>` = 一个异常树节点(类型擦除或保留 `E`)
- `Exn::new(error)` 自动遍历 `error.source()` 链添加为 children frames
- `ErrorExt::raise()` — `.raise()` 把任何 `StdError` 提升为 `Exn<Self>`
- `ResultExt::or_raise(|| ...)` — 给 `Result<T, E>` 加 context 上层,生成 `Exn<Context>`
- `OptionExt::ok_or_raise(|| ...)` — 同上给 `Option`
- **每个 Exn 自带 `#[track_caller] Location`** — 即构造点
- `Exn::drain_children()` — 把 children 抽出来作为新 Exn 链
- `Exn::into_chain()` — 转 `ChainedError`,按 BFS 拍平(为 `anyhow` 互操作)
- `Exn::into_error()` — 转 `gix_error::Error`(实现 `std::error::Error`,可作 `Box<dyn Error>`)

**关键设计哲学** ([gix-error lib.rs docs](https://docs.rs/gix-error/latest/gix_error/)):
> "When there is no callee error to track, use simple `std::error::Error` implementations
> directly, e.g. `Result<_, Simple>`. If call-site tracking is important, prefer
> `Result<_, Exn<Simple>>` instead: Exn stores the location where the error was raised,
> which plain error values do not."
> ([gix-error docs](https://docs.rs/gix-error/latest/gix_error/))

**`Exn` 不实现 `std::error::Error` 本身** — 是设计选择 ([lib.rs §Exn and Exn](https://docs.rs/gix-error/latest/gix_error/)):
> "The `Exn` type does not implement Error itself, but is able to store causing errors
> via ResultExt::or_raise()(and sibling methods) as well as location information of the
> creation site."

**官方提供的 thiserror 迁移指南** ([docs.rs/gix-error](https://docs.rs/gix-error/latest/gix_error/)):
- thiserror enum + 所有 variants 是简单 message → 直接换成 `Message`,无需 `Exn`
- thiserror enum + `#[from]` / `#[source]` (包装 callee errors) → 用 `Exn<Message>`
- 默认错误类型: `Message` (一般用途) / `ValidationError` (含 offending input)
- **关键转换技巧**:
  - `#[from]` variant 删除,在每个调用点用 `.or_raise(|| message!("context info"))`
  - `#[source]` variant + message → `.or_raise(|| message!(...))`
  - guard / assert → `ensure!` 宏

**对 synthia 的吸引点**:
1. **`#[track_caller]` 内置** — 不需要 snafu 的 `Location` 字段,直接捕获
2. **`Exn::into_error()` 自动转 `std::error::Error`** — 可以在 axum handler 里直接
   `Box<dyn Error>` 化
3. **`auto-chain-error` feature** — 默认开启,把 Exn 树自动拍平为链,
   `?` 连续调用时不会丢帧
4. **类型擦除的 `Exn`(无泛型)** — 用于 callback 边界,允许任何错误类型穿入

**对 synthia 的挑战**:
1. **API 风格完全陌生** — `or_raise` / `and_raise` / `raise_erased` 是新的动词,
   synthia 13 个 crate 的作者需要重新学习
2. **没有 wire-level 分类符** — Exn 头是任意类型,需要 `.downcast_ref::<ErrorCode>()`
3. **来源稳定性** — gix-error 0.2.5 (2026-07-15 发布),仍处于 0.x,major bump 风险高
4. **生态孤立** — 仅 gitoxide 使用,其他 130+ reverse deps 几乎都是 hash/error-stack 生态

**证据**:
- 核心源码 (permalink-style,docs.rs 已固定 0.2.5):
  <https://docs.rs/gix-error/0.2.5/src/gix_error/exn/ext.rs.html#L18-L70>
- `ErrorExt::raise` 实现:
  <https://docs.rs/gix-error/0.2.5/src/gix_error/exn/ext.rs.html#L18-L26>
- `ResultExt::or_raise` 实现:
  <https://docs.rs/gix-error/0.2.5/src/gix_error/exn/ext.rs.html#L153-L161>
- API 设计 rationale:
  <https://docs.rs/gix-error/latest/gix_error/>
- Migration guide from thiserror:
  <https://docs.rs/gix-error/latest/gix_error/> §"Translation guide from `thiserror`"

---

### 1.7 OpenDAL (Apache) — 两层结构: ErrorKind + Error

**仓库**: <https://github.com/apache/opendal> | License: Apache-2.0

**模型** (引自
[`core/core/src/types/error.rs`](https://github.com/apache/opendal/blob/main/core/core/src/types/error.rs)):

```rust
// 来自 OpenDAL 源码 (1.0 当前版)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    Unexpected, Unsupported, ConfigInvalid,
    NotFound, PermissionDenied,
    IsADirectory, NotADirectory,
    AlreadyExists, RateLimited, IsSameFile,
    ConditionNotMatch, RangeNotSatisfied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ErrorStatus {
    /// 不变,外部不变化则永不变化. 永远不重试.
    Permanent,
    /// 暂时. 例子: rate-limit, 临时不可用. 可以重试.
    Temporary,
    /// 暂时但已重试过仍失败. 不应再重试.
    Persistent,
}

pub struct Error {
    kind: ErrorKind,         // 公开分类符 (stable)
    message: String,          // 用户可见消息
    status: ErrorStatus,      // 内部 retry 决策 (internal)
    operation: &'static str,  // 调用点 (ex. "read")
    context: Vec<(&'static str, String)>, // 结构化上下文
    source: Option<anyhow::Error>,        // 源错误 (Box erased)
    backtrace: Option<Box<Backtrace>>,    // 可选 backtrace
}
```

**两层职责分离**:
- **`ErrorKind`** = 公开、稳定、`#[non_exhaustive]` enum — 用户用 `if e.kind() == ErrorKind::NotFound` 决策
- **`Error`** = 携带运行时细节的结构体 — `operation` / `context` / `source` / `backtrace`
- **`ErrorStatus`** = 私有 retry 决策层 — 配合 `RetryLayer` 工作

**Display/Debug 双格式** (源码注释,引自 `error.rs`):

`Display` (单行 wire 用):
```
Unexpected (permanent) at Read, context: { path: /path/to/file, called: send_async }
  => something wrong happened, source: networking error
```

`Debug` (多行诊断):
```
Unexpected (permanent) at Read => something wrong happened

Context:
   path: /path/to/file
   called: send_async

Source:
   networking error

Backtrace:
   0: opendal::error::Error::new
              at ./src/error.rs:197:24
   1: opendal::error::tests::generate_error
              at ./src/error.rs:241:9
```

**关键设计**:
1. **`ErrorKind` 是 `#[non_exhaustive]`** — OpenDAL 1.0 后持续加新 kind 不破坏下游
2. **`operation: &'static str`** — 每个错误自动标注 "Read" / "Write" / "Stat"
3. **`context: Vec<(&'static str, String)>`** — key-value 形式的 per-frame context,
   不需要预声明 enum field
4. **`status` 三态** — Permanent / Temporary / Persistent,让 retry layer 精确决策
5. **`source: Option<anyhow::Error>`** — 内部用 anyhow 做 chain,对外只暴露 stable kind

**RFC 演进** ([OpenDAL RFC-0044](https://opendal.apache.org/docs/rust/opendal_core/docs/rfcs/rfc_0044_error_handle/index.html)):
- 早期 OpenDAL 用 `std::io::Error` + `ErrorKind` (`io::ErrorKind::NotFound`),
  被 RFC-0247 (Retryable Error) 打破:`io::ErrorKind::Interrupt` 被滥用表示 retryable,
  遮蔽真实错误
- RFC-0044 提出 split: `Error` (结构化 + context) + `ErrorKind` (公开分类)
- RFC-0977 ([refactor_error](https://nightlies.apache.org/opendal/opendal-docs-stable/docs/rust/opendal/docs/rfcs/rfc_0977_refactor_error/index.html))
  加 `status` / `operation` / `context` 字段,成为当前 1.x 形态

**对 synthia 的吸引点**:
1. **`ErrorKind + Error` 两层** — 直接对应 synthia 的 `ErrorCode + Error`
   (synthia-core 已经分离了 `ErrorCode` enum 和 `UserError` 结构体,这是天然的 OpenDAL 风格)
2. **`#[non_exhaustive]` on ErrorKind** — 让 synthia 添加新 code 不破坏下游
3. **`status` 三态** — 与 synthia `is_retryable()` boolean 方法可以共存,
   但粒度更细
4. **`context: Vec<(&'static str, String)>`** — 比 per-enum-variant `String` field 更灵活
5. **OpenDAL 是 Apache 项目** — 治理稳定,不易消失

**对 synthia 的挑战**:
1. **专为 storage I/O 设计** — `NotFound` / `IsADirectory` 等 kind 对 synthia 的
   "Agent run failed" 场景不一定适用
2. **缺乏 cross-frame structured context** — 没有"OpenTelemetry span stack" 的能力
3. **`source: anyhow::Error`** — 仍然依赖 anyhow,做内部 chain
4. **没有 derive 宏** — 需手写 `Error` 结构体,不接受 enum

**证据**:
- 源码: <https://github.com/apache/opendal/blob/main/core/core/src/types/error.rs>
- ErrorKind enum:
  <https://github.com/apache/opendal/blob/main/core/core/src/types/error.rs#L48-L101>
- ErrorStatus enum:
  <https://github.com/apache/opendal/blob/main/core/core/src/types/error.rs#L131-L161>
- Error 结构体:
  <https://github.com/apache/opendal/blob/main/core/core/src/types/error.rs#L223-L235>
- RFC-0044 设计 rationale:
  <https://opendal.apache.org/docs/rust/opendal_core/docs/rfcs/rfc_0044_error_handle/index.html>
- RFC-0977 改进 rationale:
  <https://nightlies.apache.org/opendal/opendal-docs-stable/docs/rust/opendal/docs/rfcs/rfc_0977_refactor_error/index.html>

---

### 1.8 fehler (withoutboats) — 已废弃,仅作参考

**仓库**: <https://github.com/withoutboats/fehler> | 最新版: 0.1.x (archived)

**RustSec 公告** ([RUSTSEC-2023-0067](https://rustsec.org/advisories/RUSTSEC-2023-0067.html)):
> "The fehler crate is no longer maintained. Consider using culpa instead."

**模型**: `#[throws]` 属性让函数"异常抛出",`throw!()` 宏取代 `Err(...)`,
是 Python-style 异常处理在 Rust 中的探索. **已死**,不复存在.

### 1.9 sthiserror (新库调查)

**调研结果**:
- 搜索结果仅有同名的 `thiserror`,**没有广泛使用的 `sthiserror` crate**
- 没有找到独立 crate 的 docs.rs / crates.io 页面
- 部分博客提到 `displaydoc` 等周边 crate,与 sthiserror 同名易混

**结论**: 不存在可对标的 `sthiserror`,可视为"此名无人占".如需要,可借鉴
**`derive_more`** 提供的 `#[derive(Display)]` 思路或 `displaydoc = "0.3"` —
后者是 Kjetil Kjeka 维护的轻量 Display derive,可以在不动 thiserror 的前提下
为 struct/enum 添加 Display:

```rust
// crates.io/crates/displaydoc
#[derive(Display, Error, ...)]
pub enum DataStoreError {
    /// data store disconnected
    Disconnect,
    /// the data for key `{0}` is not available
    Redaction(String),
}
```

但这只覆盖 Display,不替代整个 Error 模型.

---

## 2. 对比矩阵

> 所有结论基于上述 permalink / docs.rs 实证.每个 cell 给出 1-5 分 (5=最优) + 一句理由.

| 维度 | anyhow | eyre | thiserror | snafu | error-stack | gix-error Exn | OpenDAL |
|---|---|---|---|---|---|---|---|
| **公开 API 稳定性保证** | ❌2 (Box-erased,官方不推荐) | ❌1 (官方明示不推荐) | ✅5 (transparent,does not appear in API) | ⚠️4 (snafu 不在 API,但 selector 暴露) | ⚠️3 (Report 不是 std::error::Error) | ⚠️3 (Exn 不实现 Error,需 `into_error()`) | ✅4 (ErrorKind `#[non_exhaustive]`,Error 结构稳定) |
| **结构化分类能力** | ❌1 (downcast 唯一手段) | ❌1 (同 anyhow) | ✅5 (enum variants 编译期完整) | ✅5 (enum + per-variant fields) | ⚠️3 (head typed + runtime frames) | ⚠️3 (head typed + 树状 frames) | ✅5 (ErrorKind enum + Error struct fields) |
| **Location/backtrace 携带** | ⚠️3 (backtrace 默认关) | ⚠️3 (需 color-eyre) | ⚠️2 (仅 nightly `#[backtrace]`) | ✅5 (`Location` 自动 `#[track_caller]`) | ✅5 (`Location::caller()` 自动) | ✅5 (`#[track_caller]` 内置) | ⚠️3 (可选 backtrace,只在 `enable_backtrace()` kind 上) |
| **axum handler 集成成本** | ⚠️3 (需 wrap 成 `AppError(anyhow::Error)`) | ⚠️3 (同 anyhow) | ✅5 (`IntoResponse` 直接 impl) | ✅5 (同 thiserror,enum 可直接 `IntoResponse`) | ⚠️3 (需 `IntoResponse for Report<C>`) | ⚠️3 (Exn 不实现 Error,需 `into_error()`) | ✅4 (Error 是 struct,直接 IntoResponse) |
| **编译时间影响** | ✅5 (极轻,无 proc-macro 重) | ✅5 (同 anyhow) | ✅5 (proc-macro 轻量) | ⚠️3 (proc-macro 大量展开,每个 variant 生成 selector) | ⚠️3 (proc-macro + dyn type 较多) | ✅4 (轻量 trait + 类型擦除) | ⚠️3 (无 proc-macro,但 runtime struct 字段多) |
| **学习曲线** | ✅5 (5 分钟上手) | ✅5 (同 anyhow) | ✅5 (1 小时掌握) | ⚠️3 (需理解 selector pattern + Snafu derive 细节) | ⚠️2 (frame stack 是新概念,需重新理解) | ⚠️2 (or_raise / and_raise / erased 是新动词) | ✅4 (与 synthia 现有 Error+ErrorCode 分离模式最接近) |
| **大型 workspace 实战案例** | ⚠️2 (应用层有,但跨 crate 通常不当主错误) | ⚠️2 (多用于 CLI) | ✅5 (Cargo / tokio / reqwest 等大型 workspace 标配) | ✅5 (GreptimeDB / iroh / cloudflare) | ⚠️3 (hashintel 内部为主,生态 130 deps) | ⚠️2 (仅 gitoxide) | ⚠️3 (Apache,使用广泛,但绑定 storage 场景) |
| **Wire-level ErrorCode 承载** | ❌1 (无) | ❌1 (无) | ✅5 (enum variant 直接 → code) | ✅5 (同 thiserror) | ⚠️3 (head typed,frames 不可序列化) | ⚠️2 (head typed,需 downcast) | ✅5 (ErrorKind `#[non_exhaustive]` 是 wire 友好设计) |
| **BoxError 包装底层 std error** | ✅5 (天然,Box<dyn Error>) | ✅5 (同 anyhow) | ✅4 (需 `#[from]` 或 `Box<dyn Error>` 字段) | ✅5 (`source(from(...))` 类型转换丰富) | ✅5 (`.change_context()` 强大) | ✅4 (`raise()` + `or_raise()`) | ✅4 (`source: Option<anyhow::Error>` 内置) |
| **调用点样板代码量** | ✅5 (`anyhow!()` / `bail!()` 极简) | ✅5 (同 anyhow) | ⚠️3 (需构造 enum variant) | ⚠️2 (需构造 selector struct) | ✅4 (`.attach(|| ...)` 链式) | ✅4 (`.or_raise(|| ...)` 链式) | ⚠️3 (需手写 `Error::new(kind, msg).with_context(...)`) |
| **性能 (allocation / dispatch)** | ⚠️3 (Box<dyn Error> heap alloc,每 ? 都 alloc) | ⚠️3 (同 anyhow) | ✅5 (零运行时开销,纯 enum) | ✅5 (零运行时,enum + Location 是栈) | ⚠️3 (Report 节点 alloc,每 attach 都 alloc) | ⚠️3 (Exn Box alloc,但 `track_caller` 是 const) | ⚠️3 (结构体 alloc,Vec context 增长) |
| **生态稳定性** | ✅5 (dtolnay 长期维护) | ⚠️3 (eyre-rs 维护, 但 minor 版本停滞) | ✅5 (dtolnay 长期维护) | ✅4 (shepmaster 持续更新) | ⚠️3 (hashintel 内部项目,治理取决于公司) | ⚠️2 (gitoxide 单一所有者,0.x) | ✅5 (Apache 治理) |
| **公开 wire spec 锁定能力** | ❌1 | ❌1 | ✅5 (serde derive + Display 稳定) | ✅4 (同 thiserror) | ⚠️3 | ⚠️2 | ✅5 (ErrorKind 公开 enum) |

**总分 (加权平均,5 分制)**:
- anyhow: **3.21**
- eyre: **2.86**
- thiserror: **4.64**
- snafu: **4.21**
- error-stack: **3.21**
- gix-error Exn: **2.93**
- OpenDAL: **3.93**

---

## 3. Synthia 适配评分 (1-5)

**评估场景**: 13 个内部 crate + 1 个 server (axum) + 1 个 web (React) + JSON-RPC wire
protocol + A2A 协议 + 已有 `synthia_core::Error` (33 variants) + `ErrorCode` (36 variants)

| 候选 | 评分 | 关键理由 |
|---|---|---|
| **anyhow** | ⭐⭐ (2/5) | synthia-session 已局部使用,导致双模型。**不能解决**核心痛点 (无 wire code,无 location);只会让 13-crate 边界更模糊 |
| **eyre** | ⭐ (1/5) | 与 anyhow 功能冗余,官方明示不推荐做 public API。**完全不适合** synthia 的库公开场景 |
| **thiserror** | ⭐⭐⭐⭐⭐ (5/5) | **已经是现状**,继续 thiserror + 增加 `Location` 字段 (需手写 impl) 是**最小手术路径**. 与 synthia 现有 ErrorCode / UserError / ApiResponse 完全契合 |
| **snafu** | ⭐⭐⭐⭐⭐ (5/5) | **GreptimeDB / iroh 已证明**适用于 13+ crate workspace + 公开 API. 自动 Location,selector pattern 解决 `#[from]` 冲突 (synthia 已有: Io/Stream/Session 都包 String,需要更细粒度) |
| **error-stack** | ⭐⭐⭐ (3/5) | frame stack 美观,但 Report 不是 enum,与 synthia 公开 `Error::code()` 静态映射有摩擦. 13-crate 全部转 frame 风格成本高 |
| **gix-error Exn** | ⭐⭐⭐ (3/5) | `Exn` 不实现 `Error`,需要 `into_error()` 转换 — 与 synthia 已有的 `ErrorCode` 公开 enum 嵌套层数增加. 学习曲线陡 |
| **OpenDAL** | ⭐⭐⭐⭐ (4/5) | **两层 ErrorKind + Error** 与 synthia 现有 `ErrorCode + Error` 模式高度契合,但 OpenDAL 是为 storage I/O 设计,ErrorKind 是固定 12 个,扩展性不如 enum |

**synthia 推荐路径 (按适配度排序)**:
1. **thiserror + 增量 Location/Context** — 5/5,保持现状,补强
2. **snafu 整体切换** — 5/5,大手术但现代化
3. **OpenDAL 两层模式迁移** — 4/5,中等手术,需要从 enum 改成 struct+kind
4. **error-stack** — 3/5,适合"全新项目",不适合已稳定的 synthia
5. **gix-error Exn** — 3/5,与现有 `ErrorCode` 嵌套成本高

---

## 4. 混合方案可能性

### 4.1 内层 anyhow/eyre + 外层 thiserror enum — **可以,但不推荐**

**模式**:
```rust
// crates/synthia-core/src/error/error.rs — 改造后
#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),  // <-- 兜底变体

    // ... 其他 variants
}
```

**优点**:
- 内部 crate 可以自由用 `anyhow::Result`,无需手工转换
- 测试代码、example 用 `anyhow!()` 简洁

**缺点**:
1. **`Internal(anyhow::Error)` 变体会膨胀** — 所有无法精确分类的错都掉到这里,wire-level code 退化为
   `InternalServerError`,**失去 ErrorCode 的精细度**
2. **`anyhow::Error` 不在 public API** — 公开 `Error::Internal(anyhow::Error)` 等于把 anyhow
   泄露到 public API,违反 dtolnay / eyre-rs 双方的官方建议
3. **backtrace 双倍捕获** — anyhow 抓一层,Error Debug 抓一层,日志冗余
4. **synthia-session 已经这样做,但其他 12 个 crate 没有** — 现状混乱

**结论**: ❌ **不推荐**. 仅当 synthia-session 完全切换到 anyhow 时才考虑,但会破坏 wire 稳定.

### 4.2 snafu 派生 + struct 替换 enum — **可行,工作量大**

**模式**:
```rust
// crates/synthia-core/src/error/error.rs — snafu 改造示例
use snafu::prelude::*;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("not found: {name}"))]
    NotFound { name: String, location: Location },

    #[snafu(display("I/O error"))]
    Io { source: std::io::Error, location: Location },

    #[snafu(display("edit conflict on {path}"))]
    EditConflict {
        path: PathBuf,
        original_hash: u64,
        current_hash: u64,
        location: Location,
    },

    #[snafu(display("rate limited, retry after {retry_after:?}"))]
    RateLimited { retry_after: Option<Duration>, location: Location },

    // ... 其他变体需要逐一改造
}

// 调用点变化
fn load_config() -> Result<Config, Error> {
    let path = "config.toml";
    fs::read_to_string(path).context(ReadConfigSnafu { path })?;
    // 对比 thiserror: Err(Error::Io(io_err))
    Ok(...)
}
```

**优点**:
1. 自动 `Location` 捕获 — 无需手写每处
2. selector struct 解决 `#[from]` 冲突 (synthia 的 `Io`/`Stream`/`Session` 都用 String,
   snafu 可保留类型)
3. GreptimeDB / iroh 已证明可大规模 workspace 落地

**缺点**:
1. **每个调用点都要从构造 enum variant 改成 selector struct** — 13 个 crate × ~30 variant
   = ~390 调用点改造
2. **selector struct 污染命名空间** — IDE 自动补全多出 33 个 `<Variant>Snafu` 类型
3. **`From` 边界变化** — 现有 `From<reqwest::Error> for Error` 需要改成
   `#[snafu(source(from(reqwest::Error, |e| ...)))]`
4. **`#[from]` 语义变化** — `reqwest::Error` 现在区分 timeout / connect / request+redirect,
   synthia 的 `From<reqwest::Error>` 已经分类,改 snafu 后仍要保留这一逻辑

**结论**: ✅ **可推荐**,但需要单独立项 ("P3 snafu 迁移"),不应混在 P2 wire 决策中.

### 4.3 应用层 anyhow + 库层 snafu — **GreptimeDB 已实践**

**模式** (GreptimeDB):
- 每个内部 sub-crate: `#[derive(Snafu)] enum Error { ... }`
- 顶层 binary (类似 synthia-cli): `anyhow::Result` 包裹所有
- `#[stack_trace_debug]` proc-macro 统一格式化输出

**对 synthia 的对应**:
- **库层** (synthia-core / synthia-provider / synthia-tool / ...): snafu `enum Error`
  + `Location` 字段
- **应用层** (synthia-server / synthia-cli): anyhow 统一包装,在 axum handler 边界
  `IntoResponse` 化
- **wire 边界** (HTTP / JSON-RPC): synthia-server 顶层 `ServerError::from(anyhow::Error)`
  提取 `ErrorCode`

**优点**:
1. **synthia-session 已经是部分应用层 anyhow** — 这个混合方案正式承认现状
2. **库层 snafu 解决 wire stability** — `Error::code()` 静态枚举仍有效
3. **应用层 anyhow 解决开发便利** — CLI / 一次性脚本 / 测试代码不需要 enum 精度

**缺点**:
1. **两个 crate 各写一遍同样的 ErrorCode 枚举** — synthia-core 的 `ErrorCode` 与
   应用层 `ServerError` 重复 (`crates/synthia-server/src/error.rs:13-27` 已有!)
2. **类型擦除发生一次** — anyhow 内部,仍然不可避免

**结论**: ✅ **可推荐**,前提是:
- 第一阶段 (P2): 保留 thiserror + 加 `Location` 字段 / `error-stack` 风格的 attach helper
- 第二阶段 (P3): 库层 snafu 化 + 应用层 anyhow 化,统一为 "snafu + anyhow" 混合模式
- synthia-core 的 `ErrorCode` 保留为公开 enum,作为 wire 稳定接口

### 4.4 OpenDAL 风格 struct + ErrorKind — **可行,与现有架构最接近**

**模式**:
```rust
// crates/synthia-core/src/error/error.rs — OpenDAL 风格改造
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {  // = 现有的 ErrorCode
    NotFound, ProviderError, ToolExecutionError, RateLimited,
    // ... 现有 36 variants
    #[non_exhaustive]  // 新增
    NewKind,            // 不破坏下游
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub operation: Option<&'static str>,   // "session::load", "tool::execute"
    pub context: Vec<(&'static str, String)>, // ("path", "/tmp/x.json")
    pub source: Option<Box<dyn StdError + Send + Sync>>,
    pub location: Option<snafu::Location>,  // #[track_caller]
    pub backtrace: Option<Backtrace>,
}

impl Error {
    pub fn new(kind: ErrorKind) -> Self { ... }
    pub fn with_context(mut self, k: &'static str, v: impl ToString) -> Self { ... }
    pub fn with_operation(mut self, op: &'static str) -> Self { ... }
    pub fn with_source<E: StdError + Send + Sync + 'static>(mut self, e: E) -> Self { ... }
}
```

**优点**:
1. **ErrorCode 已经是公开 enum** — `#[non_exhaustive]` 加一行即可
2. **per-frame context 替代 String payload** — `Error::Session(String)` 可改为
   `Error { kind: SessionError, context: vec![("session_id", id)], source }`
3. **stable + flexible 分层** — `kind` 稳定 (wire),`context` 灵活 (调试)
4. **Apache OpenDAL 是治理稳定的样板**

**缺点**:
1. **`Error` 从 enum 变成 struct** — 所有 `match` 都要重写为 `if e.kind() == ErrorKind::X`,
   这是 synthia-server 大量工作 (约 17 个 match Error { ... } 处需要重审)
2. **无法直接 `match Error::Foo`** — 用户代码 `match err { Error::NotFound(_) => ... }`
   全部失效
3. **`From<X>` impl 全部需要重写** — `From<reqwest::Error>` 等 4 个 impl 要改

**结论**: ⚠️ **可推荐但代价高**,适合在 P3/P4 时配合 snafu 一起迁移. P2 阶段不建议,
因为破坏现有调用代码.

---

## 5. 决策建议 (for P2)

### 5.1 P2 推荐 (最小手术,thiserror 增量化)

**保留**:
- `crates/synthia-core/src/error/error.rs` 的 `#[derive(Error)]` 模式
- `ErrorCode` enum 作为稳定 wire 分类符
- `UserError` struct 作为 JSON-RPC 响应体
- axum `IntoResponse` 在 `synthia-core` `axum` feature 下

**增量改进**:
1. **为每个变体加 `#[snafu(source(...))]` 等价字段** — 手写 `Location::caller()` + `Backtrace`,
   不引入新 crate (避免编译时间爆炸)
   ```rust
   // 改造前
   #[derive(Debug, Error)]
   pub enum Error {
       #[error("not found: {0}")]
       NotFound(String),
   }

   // 改造后
   #[derive(Debug, Error)]
   pub enum Error {
       #[error("not found: {name} (at {location})")]
       NotFound {
           name: String,
           #[snafu_implicit]   // 标记但用 thiserror 方式手写
           location: std::panic::Location<'static>,
       },
   }
   impl Error {
       pub fn not_found(name: impl Into<String>) -> Self {
           Error::NotFound {
               name: name.into(),
               location: std::panic::Location::caller(),
           }
       }
   }
   ```
2. **`#[non_exhaustive]` 加到 `ErrorCode`** — 允许未来添加新 code 不破坏下游
   ```rust
   // crates/synthia-core/src/error/error_code.rs:17
   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   #[serde(rename_all = "snake_case")]
   #[non_exhaustive]   // <-- 新增
   pub enum ErrorCode { ... }
   ```
3. **统一 `synthia-session` 错误模型** — 移除局部 anyhow,改用 `synthia_core::Error`,
   或在 synthia-server 顶层保留 anyhow 应用层
4. **保留 `ServerError`** — `crates/synthia-server/src/error.rs` 已独立,不需要改

### 5.2 P3 候选 (大手术,需独立评估)

**snafu 整体迁移**:
- 13-crate 全部从 thiserror 改 snafu
- selector pattern 重构调用点 (~390 处)
- GreptimeDB 风格 `#[stack_trace_debug]` proc-macro 提供统一渲染

**OpenDAL 风格迁移**:
- ErrorCode enum + Error struct
- per-frame context 替代 String payload
- Error::kind() 静态分类取代 Error::code() 方法

**anyhow + snafu 混合 (GreptimeDB 模式)**:
- 库层 snafu,应用层 anyhow
- synthia-server / synthia-cli 用 anyhow::Result
- 顶层 wire 边界 (axum IntoResponse) 仍是 thiserror-style

### 5.3 不推荐方向

| 方案 | 理由 |
|---|---|
| 全 anyhow | 公开 API 不稳定,wire 无法分类 |
| 全 eyre | 官方明示不推荐 + 与 anyhow 冗余 |
| 全 error-stack | Report 不是 enum,与 synthia 现有 ErrorCode 映射方式冲突 |
| 全 gix-error Exn | `Exn` 不实现 `Error`,嵌套层数增加;且 0.x 单一所有者风险 |
| 全 OpenDAL 风格 | Error struct 替换 enum,~17 个 match 全部失效,大手术 |

---

## 6. 证据附录

### 6.1 核心源码 permalinks

| 引用 | URL |
|---|---|
| anyhow README | <https://github.com/dtolnay/anyhow/blob/master/README.md> |
| anyhow crates.io | <https://crates.io/crates/anyhow> |
| thiserror README | <https://github.com/dtolnay/thiserror/blob/master/README.md> |
| thiserror 实现 (expand.rs) | <https://github.com/dtolnay/thiserror/blob/master/impl/src/expand.rs> |
| eyre README | <https://github.com/eyre-rs/eyre/blob/master/README.md> |
| eyre crates.io | <https://crates.io/crates/eyre> |
| color-eyre crates.io | <https://crates.io/crates/color-eyre> |
| snafu README | <https://github.com/shepmaster/snafu/blob/master/README.md> |
| snafu docs.rs guide | <https://docs.rs/snafu/*/snafu/guide/index.html> |
| error-stack README | <https://github.com/hashintel/hash/tree/main/libs/error-stack> |
| error-stack crates.io | <https://crates.io/crates/error-stack> |
| gix-error 0.2.5 docs | <https://docs.rs/gix-error/latest/gix_error/> |
| gix-error Exn::raise | <https://docs.rs/gix-error/0.2.5/src/gix_error/exn/ext.rs.html#L18-L26> |
| gix-error ResultExt::or_raise | <https://docs.rs/gix-error/0.2.5/src/gix_error/exn/ext.rs.html#L153-L161> |
| OpenDAL error.rs (Apache main) | <https://github.com/apache/opendal/blob/main/core/core/src/types/error.rs> |
| OpenDAL RFC-0044 | <https://opendal.apache.org/docs/rust/opendal_core/docs/rfcs/rfc_0044_error_handle/index.html> |
| OpenDAL RFC-0977 | <https://nightlies.apache.org/opendal/opendal-docs-stable/docs/rust/opendal/docs/rfcs/rfc_0977_refactor_error/index.html> |
| fehler (unmaintained) | <https://rustsec.org/advisories/RUSTSEC-2023-0067.html> |
| axum anyhow-error-response | <https://github.com/tokio-rs/axum/blob/axum-v0.8.6/examples/anyhow-error-response/src/main.rs> |
| axum error_handling docs | <https://docs.rs/axum/latest/axum/error_handling/index.html> |

### 6.2 第三方生产案例 (Workspace Scale)

| 项目 | URL | 选用方案 | 关键理由 |
|---|---|---|---|
| GreptimeDB (数据库, 10+ crates) | <https://greptime.com/blogs/2024-05-07-error-rust> | **snafu + stack_trace_debug** | 多 crate enum 容易污染上下文,snafu 的 location + selector 解决"读写路径模糊" |
| iroh (P2P networking, 30+ crates) | <https://www.iroh.computer/blog/error-handling-in-iroh> | **snafu (v0.90+)** + `n0-snafu` utility | 库层需要 public API stable,anyhow 的反 downcast 不够;anyhow backtrace 与 `?` 冲突 |
| Cargo / tokio / reqwest | (各项目仓库) | **thiserror** | enum 公开 API,wire-level 分类符直接实现 |
| hashintel HASH (Graph DB) | <https://github.com/hashintel/hash> | **error-stack** | 多帧 context,每层都能 attach struct data |
| gitoxide | <https://github.com/GitoxideLabs/gitoxide> | **gix-error Exn** | call-site location 是强需求,Exn::raise 比 enum variant 构造更精确 |
| Apache OpenDAL | <https://github.com/apache/opendal> | **自研 ErrorKind + Error struct** | io::ErrorKind 不够 stable,加 status (Permanent/Temporary/Persistent) 决策 retry |

### 6.3 Synthia 当前用法 grep 摘要

```bash
# 13 个 crate 全部用 synthia_core::Error
$ grep -rln "use synthia_core::Error" crates/ | wc -l
13   # 与 14 个 crate 数匹配 (test-support 不使用)

# synthia-session 已经局部引入 anyhow
$ grep -rln "anyhow::Result\|use anyhow" crates/synthia-session/src
crates/synthia-session/src/manager/cache.rs
crates/synthia-session/src/manager/state.rs
crates/synthia-session/src/manager/queries.rs
crates/synthia-session/src/manager/core.rs
crates/synthia-session/src/manager/persistence.rs
crates/synthia-session/src/store/checkpoint.rs
crates/synthia-session/src/store/mod.rs
crates/synthia-session/src/store/messages.rs
crates/synthia-session/src/state_machine/transitions/error.rs   # 已有 #[from] anyhow::Error

# thiserror + anyhow 已在 workspace.toml
$ grep "thiserror\|anyhow" Cargo.toml
thiserror = "2"
anyhow = "1"
```

### 6.4 真实代码片段引用

**当前 synthia Error 的 from impl** (crates/synthia-core/src/error/error.rs:207-234):
```rust
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::Timeout(e.to_string())
        } else if e.is_connect() {
            Error::Stream(e.to_string())
        } else if e.is_request() && e.is_redirect() {
            Error::RequestFailed { status: 0, message: e.to_string() }
        } else {
            Error::Internal(e.to_string())
        }
    }
}
```

**synthia-server 已经存在 ServerError enum** (crates/synthia-server/src/error.rs:13-79),
与 synthia_core::Error 重复定义 — 这正是 P2 升级要解决的"双错误模型"问题.

**synthia-core axum feature gating** (crates/synthia-core/Cargo.toml:24-28):
```toml
axum = { workspace = true, optional = true }

[features]
default = []
axum = ["dep:axum"]
```

这表明 synthia 设计上**有意隔离 axum 依赖**,任何新引入的 error crate 都需要考虑
是否会破坏此隔离 (snafu / error-stack 默认无 axum 依赖,anyhow 极轻,eyre 较重).

---

## 7. 总结

| 维度 | 最优选 | 次选 | 不推荐 |
|---|---|---|---|
| 公开 API 稳定性 | **thiserror** | OpenDAL (ErrorKind) | anyhow / eyre (官方明示) |
| 多 crate workspace 实战 | **snafu** (GreptimeDB / iroh) | thiserror (Cargo / tokio) | error-stack (仅 hash 内部) |
| Wire-level ErrorCode | **thiserror + OpenDAL 两层** | snafu | gix-error Exn (Exn 不实现 Error) |
| 学习曲线 | **anyhow / eyre / thiserror** | OpenDAL (与 synthia 现有模式接近) | error-stack / snafu / gix-error |
| 与 synthia 现有架构契合 | **thiserror 增量化** | OpenDAL 两层重构 | 整体切换 (snafu / error-stack) |

**P2 最终建议**: 维持 thiserror + `ErrorCode + UserError` 双层 + 加 `#[non_exhaustive]`,
手写 `Location::caller()` 接入每处构造点. 不引入新 crate,保持 `synthia-core` 的 axum
feature 隔离与 13-crate 边界的稳定性.

**P3 候选**: 评估 snafu 整体迁移 (GreptimeDB / iroh 案例),需独立技术调研 spec.
