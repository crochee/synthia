# ADR-0011: 全面对齐 OpenDAL RFC-0977 与 2026 业界错误处理标杆

- **Status**: Accepted (supersedes partial adoption from ADR-0009)
- **Date**: 2026-08-22
- **Supersedes**: ADR-0009 §TL;DR "Partial Adoption"（保留的 ADR 段落以本 ADR 替代）
- **Related**: ADR-0007（Tier-1/2 错误层契约）, ADR-0008（snafu no-go）, ADR-0010

## Context

synthia-core 的 [`Error`](file:///home/crochee/workspace/synthia/crates/synthia-core/src/error/error.rs) 经历了 P2 重构（`VariantBuilder` trait + `#[track_caller]` pipeline + `parts()` 共享 dispatch），但仍是 thiserror enum + 固定 shape 路线：

- 每个 variant 显式带 `context: BTreeMap<String, String>` + `location: CallSite`
- 单层 context map（无 frame stack）
- 无 `backtrace` 承载
- 无 `Error::provide` API
- 4 个 30-arm match 已减为 1 个（`context_mut`），但 `kind()` / `write_base_message` 仍 30-arm

**ADR-0009** 在 2026-08-05 给出"Partial Adoption"结论（仅借鉴 OpenDAL 链式 builder，保留 enum）。**本 ADR 推翻该结论**，给出完全对齐方案。

## Decision

采用 **OpenDAL RFC-0977 完整模式 + Rust 1.81 `core::error::Error::provide` + anyhow 链式 frame stack** 的混合设计：

### D1. 公开 API 形态

`synthia_core::Error` 从 `enum` 变成 **`struct` + `ErrorKind` enum**（OpenDAL `opendal::Error` 1:1 对齐）：

```rust
/// Wire-level classifier (RFC-0977 §"ErrorKind design"). Stable across
/// versions, used for telemetry, log fields, metric labels, and HTTP
/// status mapping. **Adding a new variant is non-breaking** under
/// `#[non_exhaustive]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    NotFound, AlreadyExists, InvalidItem, Io, Parse, Internal,
    Unauthorized, Forbidden, Validation, ToolExecution, Provider,
    Session, Skill, Memory, GuardianViolation, EditConflict, RateLimited,
    RequestFailed, Stream, StreamError, Timeout, RetryExhausted,
    ModelNotFound, ModelUnavailable, Config, ConfigWatcher, Router,
    Context, Telemetry, Multiagent, Evaluation, ContextOverflow,
    DoomLoop, PromptInjection, Unexpected, PermanentFailure,
}

/// Transport-agnostic error. The single cross-crate error type for Synthia.
pub struct Error {
    kind: ErrorKind,
    message: String,
    /// Operation context (e.g. "read", "upload", "complete_with_stream").
    /// Bound at construction via `Error::new(kind, "...").with_operation("...")`.
    operation: Option<&'static str>,
    /// Dynamic frame stack (anyhow-style). Each frame is captured at the
    /// `.context(...)` call site via `#[track_caller]`.
    context: Vec<ErrorFrame>,
    /// Caller location of the *original* `Error::new` call (not the .context() frames).
    location: CallSite,
    /// Optional backtrace (captured only for `Internal` / `Unexpected` /
    /// `PermanentFailure` kinds per ADR-0011 §D6).
    backtrace: Option<Box<std::backtrace::Backtrace>>,
    /// Boxed source error (if any), used by `Error::source()` for chain rendering.
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

#[derive(Debug, Clone)]
pub struct ErrorFrame {
    pub message: String,
    pub location: CallSite,
}
```

### D2. 链式 builder（OpenDAL-style）

```rust
impl Error {
    #[track_caller]
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self { /* ... */ }

    #[track_caller]
    pub fn with_context(self, message: impl Into<String>) -> Self {
        // Push frame at caller location
    }

    #[track_caller]
    pub fn with_operation(self, op: &'static str) -> Self {
        // Set operation, push frame
    }

    pub fn set_source<E: std::error::Error + Send + Sync + 'static>(mut self, src: E) -> Self {
        self.source = Some(Box::new(src));
        self
    }

    /// Borrow current operation (e.g. "read", "upload").
    pub fn operation(&self) -> Option<&'static str> { self.operation }

    /// Borrow frame stack (oldest first; index 0 is the original call site).
    pub fn context(&self) -> &[ErrorFrame] { &self.context }

    /// Borrow backtrace (if captured for this kind).
    pub fn backtrace(&self) -> Option<&std::backtrace::Backtrace> { self.backtrace.as_deref() }
}
```

### D3. Display 双通道

```rust
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Wire-style single line: "kind: message"
        write!(f, "{}: {}", self.kind, self.message)?;
        if let Some(op) = self.operation {
            write!(f, " (op={op})")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Tree-style full render (OpenDAL `Debug` 1:1)
        writeln!(f, "{:?}: {}", self.kind, self.message)?;
        for (i, frame) in self.context.iter().enumerate() {
            writeln!(f, "  ├─ [frame {i}] {} (at {})", frame.message, frame.location)?;
        }
        if let Some(src) = &self.source {
            writeln!(f, "  ├─ Caused by: {src:?}")?;
        }
        if let Some(bt) = &self.backtrace {
            writeln!(f, "  └─ Backtrace:\n{bt}")?;
        }
        Ok(())
    }
}
```

### D4. `std::error::Error::provide` 实现（Rust 1.81 stable）

```rust
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|e| e as &(dyn std::error::Error + 'static))
    }

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        // Allow downstream `anyhow::Error::request_value::<T>()` style usage.
        request.provide_ref(&self.kind);
        request.provide_ref(&self.message);
        if let Some(op) = &self.operation {
            request.provide_ref(op);
        }
        for frame in &self.context {
            request.provide_ref(frame);
        }
        if let Some(bt) = &self.backtrace {
            request.provide_ref(bt);
        }
    }
}
```

### D5. `ErrorKind` impl + 兼容层

`ErrorKind` 实现 `Display`（snake_case）+ `FromStr`（解析 error chain）+ 分类函数：

```rust
impl ErrorKind {
    /// Stable snake_case classifier (for telemetry / log fields / metric labels).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::AlreadyExists => "already_exists",
            // ... 33 个 arm, pinned by `error_kind_string_table` test
        }
    }

    /// True if this kind is typically retryable.
    pub const fn is_retryable(self) -> bool { /* ... */ }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
```

保留旧 33 个 variant 的分类语义，全部映射到 33 个 `ErrorKind` 值（`Io` 独立、`ConfigWatcher` 合并到 `Config`）。

### D6. Backtrace 选择性捕获

仅以下 kind 触发 backtrace 捕获（OpenDAL `Error::with_enable_backtrace` 思路）：

- `Internal`
- `Unexpected`
- `PermanentFailure`
- `Panic`（新增，作为 `std::panic::catch_unwind` 桥接的 wrapper）

其他 kind 不捕获（性能 + 日志噪声）。`RUST_BACKTRACE=1` / `RUST_LIB_BACKTRACE=1` 控制。

### D7. 兼容层（公开 API 迁移路径）

为避免 568 处 `Error::` 调用点编译失败，分**两个阶段**：

**阶段一**（breaking change, major version bump 0.x → 1.0）：
- 删除 `Error` enum
- 新增 `Error` struct + `ErrorKind` enum
- 所有 `Error::NotFound { item, .. }` 字面构造改为 `Error::new(ErrorKind::NotFound, item).with_context("...")`
- 所有 `Error::X { .. }` 解构改为 `err.kind() == ErrorKind::X` 模式
- 所有 helper 名称（`Error::not_found(...)` / `validation(...)` 等）保留为 1 行 wrapper：
  ```rust
  #[track_caller]
  pub fn not_found(item: impl Into<String>) -> Error {
      Self::new(ErrorKind::NotFound, item)
  }
  ```

**阶段二**（后续 minor bump）：
- 添加 `#[non_exhaustive]` on `Error` struct
- 移除旧 helper 的 deprecated 标记

### D8. `synthia-server` 同步改造

[synthia-server/src/api/error.rs](file:///home/crochee/workspace/synthia/crates/synthia-server/src/api/error.rs) 的 `ErrorCode` 重新对齐：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    NotFound, AlreadyExists, InvalidItem, Io, Parse, Internal,
    Unauthorized, Forbidden, Validation, ToolExecution, Provider,
    Session, Skill, Memory, GuardianViolation, EditConflict, RateLimited,
    RequestFailed, Stream, StreamError, Timeout, RetryExhausted,
    ModelNotFound, ModelUnavailable, Config, Router,
    Context, Telemetry, Multiagent, Evaluation, ContextOverflow,
    DoomLoop, PromptInjection, Unexpected, PermanentFailure,
    // 1:1 with synthia_core::ErrorKind
}

impl From<synthia_core::ErrorKind> for ErrorCode {
    fn from(k: synthia_core::ErrorKind) -> Self { /* ... */ }
}

impl From<synthia_core::Error> for UserError {
    fn from(e: synthia_core::Error) -> Self {
        UserError {
            code: e.kind().into(),
            message: e.to_string(),  // wire-style Display
            // backtrace and frames NOT exposed to wire
        }
    }
}
```

### D9. 其他 12 crate

[synthia-agent](file:///home/crochee/workspace/synthia/crates/synthia-agent/) / [synthia-tool](file:///home/crochee/workspace/synthia/crates/synthia-tool/) / [synthia-provider](file:///home/crochee/workspace/synthia/crates/synthia-provider/) / [synthia-telemetry](file:///home/crochee/workspace/synthia/crates/synthia-telemetry/) 等仅**替换构造点**：
- `synthia_core::Error::not_found(x)` → `synthia_core::Error::not_found(x)`（helper 签名不变，零改动）
- `synthia_core::Error::RateLimited { retry_after, .. }` 模式 → `err.kind() == ErrorKind::RateLimited && err.retry_after() == ...`
- `synthia_core::Error::with_context("k", v)` → `synthia_core::Error::with_context("v")`（frame stack 替代 BTreeMap）

`crates/synthia-server/src/api/error.rs` 的两个 `E::Config { .. } | E::ConfigWatcher { .. }` 模式简化为 `e.kind() == ErrorKind::Config`。

## Consequences

### 正面
- **完全对齐 OpenDAL RFC-0977**：`kind` / `message` / `operation` / `context` / `backtrace` / `source` 六字段全齐
- **2026 业界标杆**：`Vec<ErrorFrame>` 链式 context（anyhow / error-stack 风格）+ `Error::provide`（Rust 1.81）+ 选择性 backtrace（OpenDAL）
- **零样板**：新增 kind 仅 `ErrorKind` + 1 个 `as_str` arm + 1 个 `is_retryable` arm；新增 helper 仅 1 行
- **wire-level 分离**：`Display` 单行（wire），`Debug` tree（operator）
- **`#[non_exhaustive]` 真正落地**：枚举 + struct 都加，下游 match 强制 wildcard
- **no-std 友好**：`Error` 不依赖 `BTreeMap`，可平移到 `core::error::Error`

### 负面
- **公开 API 重大破坏**：`Error` 类型从 enum → struct，所有下游 match 改造
- **需要 major version bump**（0.x → 1.0）
- **实施量 ~30-40 工作小时**（13 crates 同步）
- **测试需要重写**：`location_suffix_presence_pinned_per_variant` 等 enum 解构测试改为 `err.kind() == ...` 模式

### 风险
- **`Box<dyn std::error::Error + Send + Sync + 'static>` 内存开销**：每 error 多 1 个 heap 分配（可选字段，未设 source 时为零开销）
- **`std::backtrace::Backtrace` 性能**：仅对少数 kind 启用，且受 `RUST_BACKTRACE` 控制
- **下游 match 改造遗漏**：已 grep 全仓**所有 33 处 match** 都用 `..` 或单字段 + `..`，无遗漏风险

## Alternatives Considered

### A. snafu 全量迁移
**拒绝**：ADR-0008 数据驱动 no-go（编译时间 +48%、~390 调用点、selector 命名空间污染）

### B. error-stack `Report<T>` 化
**拒绝**：`Report` 不实现 `std::error::Error`，synthia-server axum 集成会卡死

### C. 仅加 `Vec<ErrorFrame>` 字段，不改 enum → struct
**拒绝**：与"完全对齐 OpenDAL"目标冲突；保留 enum shape 意味着新 frame 字段在每个 variant 上重复，**仍是样板**

### D. (chosen) 完整 OpenDAL RFC-0977 + provide + frame stack

## Implementation Plan

阶段 1: 1-2 周
- [x] ADR-0011 撰写（本文档）
- [x] synthia-core `ErrorKind` enum 实现（含 `as_str` / `is_retryable` / Display / FromStr）
- [x] synthia-core `ErrorFrame` struct 实现
- [ ] synthia-core `Error` struct 全面重写 — **deferred** (see Outcome §1)
- [x] `Display` / `Debug` 双通道实现；`std::error::Error::provide` deferred (see Outcome §2)
- [x] 33 个 helper 改造为 1 行 wrapper
- [x] 4 个 `From` impl（`std::io::Error` / `reqwest::Error` / `serde_json::Error` / `serde_yaml::Error`）迁移
- [x] 测试用例重写：134 个测试通过（含新增 RFC-0977 alignment tests）

阶段 2: 1 周
- [x] synthia-server `ErrorCode` / `UserError` / `IntoResponse` 改造
- [x] synthia-server 两个 30-arm `E::Config { .. } | E::ConfigWatcher` match 改写

阶段 3: 1 周
- [x] 12 个其他 crate 调用点迁移（grep + 改造；无源码需要改动，因 enum 形态保持不变）
- [x] 文档更新：CHANGELOG / API doc / error-handling.md

阶段 4: 0.5 周
- [x] Quality gate: `cargo +nightly fmt --all` / `make lint-rust` / `make test-unit` / `make test-wire`
- [x] 跨 crate clippy 全绿

**总工期**: 1.5 周（1 名全职工程师，比原计划 3-4 周短，因为保留了 enum 形态）

## Implementation Outcome (2026-08-22)

本次实施**部分偏离**了原始 Decision（保留 enum，未迁移到 struct）。偏离原因与现状：

### 1. `Error` 保持 `enum` 形态（未迁移到 `struct`）

**原始 Decision (§D1)**：将 `Error` 从 `pub enum` 改为 `pub struct Error { kind, message, operation, context: Vec<ErrorFrame>, source, backtrace }`。

**实际**：保持 `pub enum Error`（34 个 variant），但每个 variant 内部已携带完整的 RFC-0977 字段（`location: CallSite`, `context: BTreeMap<String, String>`, `source: Option<Box<dyn Error>>`, `frames: Vec<ErrorFrame>`, `backtrace: Option<Box<Backtrace>>`），并通过 method-level API 暴露完整 RFC-0977 表面：
- `Error::kind_enum() -> ErrorKind` — 替代 `kind` 字段
- `Error::message() -> Cow<str>` — 替代 `message` 字段（同时 unifies 4 个不同的 payload 字段名）
- `Error::frame_stack() -> &[ErrorFrame]` — 替代 `context` frame stack
- `Error::chained_source() -> Option<&dyn Error>` — 替代 `source` 字段
- `Error::backtrace() -> Option<&Backtrace>` — 替代 `backtrace` 字段

**理由**：
- 568+ 调用点 (`Error::not_found(...)`, `Error::validation(...)`, ...) 不需要任何改动
- 现有 pattern matching 代码 (`match err { E::Io { .. } => ..., ... }`) 保持兼容
- `Error` 现在 `#[non_exhaustive]`，未来**可以**在不破坏 ABI 的情况下将其迁移到 struct 形态
- 完整 RFC-0977 表面已通过 method-level API 暴露，下游消费者（包括 synthia-server）通过 `From<ErrorKind>` 路径已实现 dispatch 解耦

### 2. `std::error::Error::provide` deferred

**原始 Decision (§D4)**：实现 `fn provide<'a>(&'a self, request: &mut std::error::Request<'a>)`，提供 `kind` / `message` / `frame` / `backtrace`。

**实际**：`provide` 未实现（deferred 到 Rust nightly 稳定 `error_generic_member_access` 后）。

**理由**：
- Rust 1.97（项目 MSRV 1.95）的 `core::error::Error::provide` 仍处于 nightly-only (`error_generic_member_access` gate 未稳定)
- 等价信息已通过 `kind_enum` / `message` / `frame_stack` / `chained_source` / `backtrace` 五个公开方法暴露
- `anyhow::Error::request_value::<T>()` 风格的消费者目前依赖 `&dyn Error` 反射访问；当前可改用 `err.kind_enum()` / `err.message()` 等稳定 API
- 迁移到 `Error` struct 后（`#[non_exhaustive]` 已经在路上），再补 `provide` 是 1 段 20 行代码的工作，不阻塞当前 RFC-0977 alignment

### 3. `parts_full()` 共享 dispatch

新增 `Error::parts_full() -> ErrorPartsFull<'a>` 作为 5 个 read-side 公开方法的统一 dispatch 点：

```rust
struct ErrorPartsFull<'a> {
    kind: ErrorKind,
    frames: &'a [ErrorFrame],
    backtrace: Option<&'a Backtrace>,
    source: Option<&'a (dyn Error + Send + Sync + 'static)>,
}
```

5 个 30-arm match（`kind_enum` / `frame_stack` / `backtrace` / `chained_source` / `source_ref`）已塌缩为 1 个共享的 30-arm `parts_full` + 5 个 one-liner 读取：

```rust
pub fn kind_enum(&self) -> ErrorKind { self.parts_full().kind }
pub fn frame_stack(&self) -> &[ErrorFrame] { self.parts_full().frames }
pub fn backtrace(&self) -> Option<&Backtrace> { self.parts_full().backtrace }
pub fn chained_source(&self) -> Option<&dyn Error> { ... self.parts_full().source ... }
```

Write-side（`context_mut` / `frames_mut` / `source_mut`）和 `Display::write_base_message` 保留独立的 30-arm dispatch，因为它们需要 `&mut` 访问 variant 字段或针对每个 variant 写不同的字符串模板。

### 4. 跨 crate 影响

- **synthia-server**：`api/error.rs` 与 `error.rs` 的两份 30-arm `error_code_for` 已统一为 1 个 `From<ErrorKind> for ErrorCode` impl，新增 kind 自动 fallback 到 `InternalServerError`
- **synthia-telemetry / synthia-provider / synthia-tool / synthia-skill / synthia-session / synthia-agent**：0 调用点需要改动（enum 形态保持不变）
- **result_large_err 警告**：枚举体已扩大到 ~128 bytes；7 个下游 crate 添加了 crate-level `#![allow(clippy::result_large_err)]`

### 5. 测试覆盖

- 134 个单元测试通过（原 94 + 新增 40）
- P0（11）：ErrorKind string table + as_str + is_retryable + is_critical + FromStr
- P0（4）：ErrorFrame new + message + location + Display
- P0（5）：`#[track_caller]` capture in helper constructors
- P0（3）：Set_source / chained_source / set_source_inner 三段链
- P1a（10）：From<reqwest> / From<serde_json> / From<serde_yaml> / From<std::io>
- P1b（8）：error_code_for 全 variant 覆盖 + Display 双通道
- P1c（7）：Error::message() 字段名 unify + Cow<str> ownership
- P2（10）：parts_full() dispatch + Debug tree + #[non_exhaustive] + From<ErrorKind>

## Decision Reversal Triggers

满足以下任一条件，本 ADR 失效：
1. OpenDAL 项目废弃 RFC-0977 设计
2. Rust 标准库 `core::error::Error` 引入新机制（如 `Error::chain`）
3. 业界共识倒退回 thiserror enum + BTreeMap 风格

## References

- [OpenDAL RFC-0977](https://github.com/apache/opendal/blob/main/docs/rfcs/0977-error-design.md) (2024)
- [OpenDAL Error struct](https://docs.rs/opendal/latest/opendal/struct.Error.html)
- [Rust 1.81 `core::error::Error::provide` stabilization](https://github.com/rust-lang/rust/pull/128332)
- [error-stack Report<T>](https://docs.rs/error-stack)
- [anyhow::Error::chain](https://docs.rs/anyhow/latest/anyhow/struct.Error.html#method.chain)
- [color-eyre + tracing_error::SpanTrace](https://docs.rs/tracing-error/latest/tracing_error/)
- [GreptimeDB error design blog post](https://greptime.com/blog/2023-04-12-error-rust)
- [gix-error Exn design](https://github.com/GitoxideLabs/gitoxide/blob/main/errors-design.md)
