# ADR-0009: OpenDAL `ErrorKind + Error` 双层模式评估

> **范围**: 在 P2 已落地 (`ErrorCode + Error` 双层) 基础上, 重新评估是否进一步形式化为
> OpenDAL 的 `ErrorKind enum + Error struct (含 kind 字段)` 结构, 即 `match err.kind() { ... }`
> 替代 `match err { Error::NotFound(_) => ... }`.
> **决策文档**: 不写实现, 只给数据驱动的判断.
> **前置**: ADR-0007 已在 §Alternatives.B 中拒绝此方案为 P3 触发条件; 本 ADR 是重新评估.

---

## TL;DR — Verdict

**Partial Adoption** — **不采用 OpenDAL 完整 `Error struct + kind()` 模式**,
但**借鉴其 `with_context` / `with_operation` / `set_source` 链式 builder** 作为 P3 增量补丁.

数据支撑:
- OpenDAL `ErrorKind` 仅 12 变体 (storage-only); synthia `ErrorCode` 有 36 变体 (业务子分类),
  **直接套用会让 18+ 个业务 code 失去空间**.
- OpenDAL `Error` 是 struct, 调用点是 `if e.kind() == ErrorKind::X` (boolean 比较) + `Err(e) if e.kind() == X` (match guard);
  synthia 已有 **568 处 `Error::` 调用 + 30 处 `is_retryable/is_rate_limited` 调用** — enum → struct 是破坏性迁移.
- OpenDAL 自承"无 `#[track_caller]`" (见证据 #1 §4), 反而靠 `with_operation("...")` 手动注入;
  synthia 已为 5 个高频 variant 加 `#[track_caller]` helper (P2.2), **机制上反而更精细**.
- **真正的杀手差异**: OpenDAL 的 `context: Vec<(&'static str, String)>` 字段是
  per-frame 动态 KV, 替代了 `Error::Session(String)` 等 12 个 String-payload variant —
  **这是值得借鉴的部分**, 但**不需要通过 enum → struct 来获取**, 可以做 thiserror 增量增强.

**Killer difference vs current design (1 句话)**: OpenDAL 用"struct + 链式 builder"
把"操作名 + 上下文 KV + 源错误 + backtrace"做成可叠加层, 而不是塞进 enum 的固定字段 —
但这与"enum 还是 struct"正交, **可以独立借鉴**.

---

## 1. OpenDAL `Error` 结构定义 (Q1 答案)

### 1.1 `Error` struct 字段

[来源: `apache/opendal/core/core/src/types/error.rs#L228-L238`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L228-L238)

```rust
pub struct Error {
    kind: ErrorKind,                            // 公开分类符
    message: String,                            // 用户可见消息
    status: ErrorStatus,                        // Permanent / Temporary / Persistent
    operation: &'static str,                    // "Read" / "Write" / "Stat"
    context: Vec<(&'static str, String)>,       // 动态 KV 上下文
    source: Option<anyhow::Error>,              // 源错误 (Box erased)
    backtrace: Option<Box<Backtrace>>,          // 可选 backtrace (仅 kind 决定)
}
```

字段约束: `kind`, `message`, `source`, `backtrace` 由构造方法设; `status` 默认 `Permanent`,
可通过 `set_temporary()` / `set_permanent()` / `set_persistent()` 改; `operation` 通过
`with_operation(...)` 改; `context` 通过 `with_context(k, v)` 追加 (Vec 增长).

### 1.2 `kind()` accessor

[来源: `core/core/src/types/error.rs#L431-L433`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L431-L433)

```rust
pub fn kind(&self) -> ErrorKind { self.kind }
```

**Copy 返回** — `ErrorKind` 是 `#[derive(Clone, Copy)]`, 所以 `kind()` 零成本.
返回后**布尔比较** `if e.kind() == ErrorKind::NotFound`, 没有 variant 解构.

### 1.3 builder 方法 (无 `From` derive)

[来源: `core/core/src/types/error.rs#L319-L428`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L319-L428)

| 方法 | 行为 |
|------|------|
| `Error::new(kind, msg)` | 构造, backtrace 仅 `enable_backtrace()` 返回 true 时捕获 |
| `with_operation(op)` | 设 operation; 若已有非空 operation, 旧的移到 `context` 的 `("called", op)` |
| `with_context(k, v)` | push `Vec` 一条 KV |
| `set_source(src)` | 设 source, debug_assert 防止覆盖 |
| `set_permanent()` / `set_temporary()` / `set_persistent()` | 改 status |
| `with_permanent(b)` / `with_temporary(b)` / `with_persistent(b)` | 条件性改 status |
| `kind()` / `is_permanent()` / `is_temporary()` / `is_persistent()` / `message()` / `backtrace()` | accessor |

**关键观察**: OpenDAL **完全没有 `From<X> for Error` 自动 derive** —
所有外部错误到 `Error` 的转换都是手工 builder 调用
([例子: `core/core/src/raw/std_io_util.rs#L29-L49`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/std_io_util.rs#L29-L49)).

```rust
pub fn new_std_io_error(err: io::Error) -> Error {
    let (kind, retryable) = match err.kind() {
        NotFound => (ErrorKind::NotFound, false),
        // ...
        _ => (ErrorKind::Unexpected, true),
    };
    let mut err = Error::new(kind, err.kind().to_string()).set_source(err);
    if retryable { err = err.set_temporary(); }
    err
}
```

### 1.4 Display / Debug 实现

[来源: `core/core/src/types/error.rs#L240-L311`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L240-L311)

- **`Display`** (单行, wire 友好):
  `Unexpected (permanent) at Read, context: { path: /path/to/file, called: send_async } => something wrong happened, source: networking error`
- **`Debug`** (多行, 调试用): 分隔 `Context:` / `Source:` / `Backtrace:` 三段
- **`{:#?}`** (alternate, 自动化测试友好): 纯结构体 dump

### 1.5 `std::error::Error` impl

[来源: `core/core/src/types/error.rs#L313-L317`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L313-L317)

```rust
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|v| v.as_ref())
    }
}
```

**不暴露 backtrace 通过 `Error::provide`** — 注释明示需 nightly
(`error_generic_member_access`), 给了一段 newtype pattern 让用户自己加.

### 1.6 `From<Error> for io::Error`

[来源: `core/core/src/types/error.rs#L501-L511`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L501-L511)

```rust
impl From<Error> for io::Error {
    fn from(err: Error) -> Self {
        let kind = match err.kind() {
            ErrorKind::NotFound => io::ErrorKind::NotFound,
            ErrorKind::PermissionDenied => io::ErrorKind::PermissionDenied,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, err)
    }
}
```

**关键洞察**: 即便 OpenDAL 内部是 struct, 仍需手写 `From<Error>` 才能向上转 io::Error;
**没有 proc-macro 帮你生成**.

---

## 2. OpenDAL `ErrorKind` 变体 (Q2 答案)

### 2.1 12 个变体

[来源: `core/core/src/types/error.rs#L49-L89`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L49-L89)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    Unexpected,                    // 兜底
    Unsupported,                   // 服务不支持该操作
    ConfigInvalid,                 // config 错
    NotFound,                      // path 不存在
    PermissionDenied,
    IsADirectory,                   // path 是目录
    NotADirectory,                 // path 不是目录
    AlreadyExists,
    RateLimited,
    IsSameFile,                    // 路径相同
    ConditionNotMatch,             // 条件不满足 (HTTP 412/304)
    RangeNotSatisfied,             // range 不满足 (HTTP 416)
}
```

12 变体. **全部是 storage I/O 语义**, 没有"业务子分类"概念.

### 2.2 组织方式

- **单一扁平 enum** (无嵌套分类符, 无子 enum)
- **`#[non_exhaustive]`** — RFC-0977 之后的演进保证, 加新变体不破坏下游 `match`
- **`#[derive(Clone, Copy, PartialEq, Eq, Hash)]`** — 可以做 hash key / set member / 零成本比较

### 2.3 Wire-stable 吗?

**是的**, 通过双重机制:
1. `#[non_exhaustive]` — **编译期** 阻止下游穷举, 必须有 `_` arm
2. RFC-0977 的设计动机明示: "Users will have similar usage as before:
   `if e.kind() == ErrorKind::ObjectNotFound`" — **kind 名是公共 API**,
   一旦发布不能重命名
3. Display impl 的 `From<ErrorKind> for &'static str` 给了稳定字符串
   ([来源: `core/core/src/types/error.rs#L112-L129`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L112-L129))

**与 synthia `ErrorCode` 完全同构**: 36 变体 + `#[non_exhaustive]` + `Display` 稳定字符串 +
serde snake_case ([synthia 现状: `crates/synthia-core/src/error/error_code.rs:23-L26`](https://github.com/crochee/synthia/blob/main/crates/synthia-core/src/error/error_code.rs#L23-L26)).

---

## 3. OpenDAL 调用点模式 (Q3 答案)

### 3.1 主导模式: 布尔比较 + match guard

OpenDAL **从不** `match err { ... }` — 因为 `Error` 是 struct, 不能 match variant.
调用模式有 3 种:

#### 模式 A: `if e.kind() == ErrorKind::X`

[来源: `core/core/src/blocking/operator.rs` 文档示例 (官方推荐)](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/blocking/operator.rs)

```rust
if e.kind() == ErrorKind::NotFound {
    println!("entry not exist")
}
```

#### 模式 B: `match err.kind() { Kind::X => ..., _ => ... }`

[来源: `core/core/src/raw/std_io_util.rs#L32-L40`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/std_io_util.rs#L32-L40)

```rust
let (kind, retryable) = match err.kind() {
    NotFound => (ErrorKind::NotFound, false),
    PermissionDenied => (ErrorKind::PermissionDenied, false),
    AlreadyExists => (ErrorKind::AlreadyExists, false),
    _ => (ErrorKind::Unexpected, true),
};
```

注意: 这里 `err` 是 `io::Error` (右侧是 `std::io::ErrorKind`), 但**同样的 `match X.kind()` 模式
适用于 OpenDAL Error**.

#### 模式 C: `Err(e) if e.kind() == ErrorKind::X => ...`

[来源: `core/core/src/raw/oio/list/flat_list.rs`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/oio/list/flat_list.rs)

```rust
Err(e) if e.kind() == ErrorKind::PermissionDenied => { ... }
Err(e) if e.kind() == ErrorKind::NotFound => { ... }
```

[来源: `core/core/src/raw/oio/copy/api.rs` 测试断言](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/oio/copy/api.rs)

```rust
assert_eq!(err.kind(), ErrorKind::Unexpected);
assert!(err.is_persistent());
```

### 3.2 反例: OpenDAL 自身 `match err.kind() { ... }` 用得不多

我统计了 `core/core/src/` 下的所有 `err.kind()` 调用, **绝大多数是布尔比较或单分支断言**,
**只有 `std_io_util.rs` 一处用了完整 match** (且是 `io::Error` → `opendal::Error` 的转换处).
也就是说, **OpenDAL 自己也承认 match-on-kind 比 match-on-variant 啰嗦**.

### 3.3 与 synthia 当前 match 模式对比

synthia 现状 (enum match):
```rust
match err {
    Error::Validation { message, .. } => { /* 解构 message */ }
    Error::NotFound { item, .. } => { /* 解构 item */ }
    other => panic!("expected Error::Validation, got {other:?}"),
}
```
([来源: `crates/synthia-core/src/registry.rs`](https://github.com/crochee/synthia/blob/main/crates/synthia-core/src/registry.rs),
[来源: `crates/synthia-agent/src/error/tests.rs`](https://github.com/crochee/synthia/blob/main/crates/synthia-agent/src/error/tests.rs))

**等价的 OpenDAL 风格** (struct + kind compare + context KV):
```rust
if err.kind() == ErrorKind::ValidationError {
    // 但 message 在哪里?? context[("message", _)] ?? 需要再次 String match
}
```
**结果是**: OpenDAL 风格**无法直接解构字段**, 字段访问要么在 struct 上 (`err.message`),
要么从 context Vec 中按 key 找 (字符串 match, 弱类型).

---

## 4. Migration Cost (Q4 答案)

### 4.1 synthia 当前 Error 工作量基线

| 指标 | 数值 | 证据 |
|------|------|------|
| `Error` enum 变体数 | **33** | `crates/synthia-core/src/error/error.rs:27-L138` |
| `ErrorCode` 变体数 | **36** | `crates/synthia-core/src/error/error_code.rs:27-L63` |
| 5 个高频 variant 已加 `location` | NotFound/AlreadyExists/InvalidItem/Validation/Internal | `crates/synthia-core/src/error/error.rs:29, 32, 35, 44, 53` |
| `#[track_caller]` helper 方法数 | 5 (`.not_found()` / `.validation()` / `.internal()` / `.already_exists()` / `.invalid_item()`) | `crates/synthia-core/src/error/error.rs:144, 153, 162, 171, 180` |
| `From<X> for Error` impl 数 | 6 (reqwest / serde_json / serde_yaml / SessionError / StoreError / StateMachineError) | grep `impl From<.+> for Error\|impl From<.+> for synthia_core::Error` |
| `Error::` 引用数 (整个 workspace) | **568** | `rg -t rust "Error::" crates/ \| awk` |
| `Error::NotFound/Validation/Internal/...` 引用数 | 33 (主要在 tests + From impls) | grep `Error::NotFound\|Error::Validation\|Error::Internal\|Error::AlreadyExists\|Error::InvalidItem` |
| `match self` / `match err` / `match error` 数 | **68** (跨 47 文件) | grep `match err\|match self\|match error` |
| `is_retryable()` / `is_rate_limited()` 调用数 | 30 | grep `is_retryable\(\)\|is_rate_limited\(\)` |
| `Error::code()` 调用数 | 10 (主要在 server `map_core_error_code`) | grep `Error::code\(\)\|\.code\(\)` |
| `ServerError` enum 变体数 | 8 + `#[non_exhaustive]` | `crates/synthia-server/src/error.rs:19-L28` |

### 4.2 完整 OpenDAL 迁移成本 (估算)

假设我们要 1:1 复制 OpenDAL 模式 (Error enum → struct):

| 任务 | LOC 估算 | 工作量估算 (人天) |
|------|---------|-------------------|
| 改写 `Error` enum → struct | ~120 LOC 净增 (builder + impl + Display/Debug) | 0.5 |
| 33 个 variant → `kind()` 字段 + `message: String` + `context: Vec<...>` | ~80 LOC | 1.0 |
| 重写 6 个 `From<X>` impl 为手工 builder | ~120 LOC (无 proc-macro) | 0.5 |
| 改写 `Error::code()` (35-arm match) → `Error::kind()` 返回 `ErrorCode` | -50 LOC (变简单) | 0.5 |
| 改写 5 个 `#[track_caller]` helper → 5 个 `with_*` builder | ~50 LOC | 0.2 |
| **改写 33 处变体解构 match** (`Error::NotFound { item, .. }` → 不能解构, 走 context 或 message) | 33 处 × 5 LOC = ~165 LOC | **2.0** |
| 改写 synthia-server `map_core_error_code` (35-arm match) | -20 LOC (变简单) | 0.2 |
| 改写 synthia-session From impls (3 个) | ~30 LOC | 0.5 |
| **下游 crate 所有 `match Error::Foo { ... }` 重写** (假设 19 处来自 match 计数, 实际只看 Error:: 类型) | 19 处 × 5 LOC = ~95 LOC | **1.5** |
| 测试改动 (Error variants 引用 + 断言) | ~200 LOC | 1.0 |
| 文档改动 (rustdoc + ADR) | ~100 LOC | 0.5 |
| **合计** | **~890 LOC 净增/改** | **~8.0 人天** |

### 4.3 Partial Adoption 成本 (如果只搬 builder)

只搬 `with_context(k, v)` / `with_operation(op)` / `set_source(err)` 这套 builder,
**不改 enum → struct**:

| 任务 | LOC 估算 | 工作量估算 |
|------|---------|------------|
| 在 `synthia_core::Error` 上加 3 个 builder 方法 | ~30 LOC | 0.2 |
| 文档 + 1 个 example | ~30 LOC | 0.1 |
| **合计** | **~60 LOC** | **~0.3 人天** |

**结论**: Partial adoption 是 27x 便宜, 且**保留 enum 全部能力** (解构 + match + exhaustiveness).

---

## 5. OpenDAL 组合性 (Q5 答案)

### 5.1 `with_operation` 链式

[来源: `core/core/src/raw/http_util/error.rs#L25-L29`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/http_util/error.rs#L25-L29)

```rust
pub fn new_request_build_error(err: http::Error) -> Error {
    Error::new(ErrorKind::Unexpected, "building http request")
        .with_operation("http::Request::build")
        .set_source(err)
}
```

**关键设计**: `with_operation` 若检测到旧 operation 非空, **自动把旧的塞进 context
的 `("called", op)` 字段** ([来源: `core/core/src/types/error.rs#L349-L356`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L349-L356)):

```rust
pub fn with_operation(mut self, operation: impl Into<&'static str>) -> Self {
    if !self.operation.is_empty() {
        self.context.push(("called", self.operation.to_string()));
    }
    self.operation = operation.into();
    self
}
```

### 5.2 `with_context` 多帧堆叠

[来源: `core/core/src/raw/http_util/error.rs#L55-L67`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/http_util/error.rs#L55-L67)

```rust
pub fn with_error_response_context(mut err: Error, mut parts: Parts) -> Error {
    if let Some(uri) = parts.extensions.get::<Uri>() {
        err = err.with_context("uri", uri.to_string());
    }
    parts.headers.remove("Set-Cookie");  // 去敏
    parts.headers.remove("WWW-Authenticate");
    parts.headers.remove("Proxy-Authenticate");
    err = err.with_context("response", format!("{parts:?}"));
    err
}
```

**调用链模式**: `parse` → 失败 → `with_operation("BytesContentRange::from_str")` →
`with_context("value", value)` → `set_source(e)`
([来源: `core/core/src/raw/http_util/bytes_content_range.rs`](https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/http_util/bytes_content_range.rs))

### 5.3 嵌套错误: `source: Option<anyhow::Error>`

OpenDAL 用 `anyhow::Error` (Box-erased dyn Error) 存 source. 这与 synthia 的 `synthia_core::Error`
**内部存 `String` 损失结构** 形成对比 — 但 OpenDAL 是为了避免循环依赖, 用了 anyhow.

合成多源错误 (例如 HTTP request failed + URI + body):

```rust
let err = Error::new(ErrorKind::Unexpected, "sending request")
    .with_operation("http_util::send")
    .set_source(reqwest_err)
    .with_context("uri", uri.to_string())
    .with_context("body", body_summary);
```

### 5.4 与 snafu/anyhow 的对比

| 能力 | OpenDAL Error | snafu | anyhow |
|------|---------------|-------|--------|
| `with_context(k, v)` per-frame | ✅ builder method | ✅ `context()` + selector | ✅ `.context()` |
| source chain (typed) | ❌ (anyhow 擦除) | ✅ (`#[snafu(source)]` 类型保留) | ❌ (anyhow::Error) |
| 自动 `#[track_caller]` | ❌ (无) | ✅ (默认开启) | ⚠️ (backtrace 默认关) |
| 状态字段 (Permanent/Temporary/Persistent) | ✅ 三态 enum | ❌ (用 enum variant 替代) | ❌ |
| 公开 enum kind | ✅ ErrorKind | ❌ (无独立 kind enum) | ❌ |

**synthia 当前能拿到的**: Partial adoption 拿掉 builder 即可获得 #1 和 #4,
**且不破坏 #2/#3 现有能力** (synthia 用 thiserror `#[from]` + `#[track_caller]` helper).

---

## 6. Synthia 当前 vs OpenDAL 双层结构 对比表

| 维度 | synthia 当前 (P2 已落地) | OpenDAL 双层 | 差异成本 |
|------|--------------------------|--------------|----------|
| **wire 分类符** | `ErrorCode` enum, 36 变体, `#[non_exhaustive]`, serde snake_case, Display 稳定 | `ErrorKind` enum, 12 变体, `#[non_exhaustive]`, Display 稳定 | synthia **更宽**, 但两者同构 |
| **结构化错误载体** | `Error` enum, 33 变体, thiserror derive | `Error` struct, 无 derive | synthia enum **支持 variant 解构**, struct 不支持 |
| **call site 捕获** | `#[track_caller]` helper (5 个高频) + `CallSite` 类型别名 | **无 `#[track_caller]`**, 用 `with_operation("...")` 手动注入 | synthia **更精细** (编译期捕获) |
| **match 模式** | `match err { Error::NotFound { item, .. } => ... }` (解构字段) | `if e.kind() == ErrorKind::X` 或 `match err.kind()` (仅 kind, 字段需 `.message` 或 `context[]`) | enum 解构 **强类型** > struct 弱类型 |
| **exhaustiveness 检查** | ✅ (enum match 自动穷举) | ⚠️ (struct + kind() 布尔比较, 无穷举) | synthia **编译期更安全** |
| **From<X> impl** | `#[derive(From)]` 自动生成 (thiserror) | ❌ 全手写 builder | synthia **代码量更少** |
| **per-frame context** | ❌ 无 (固定字段) | ✅ `context: Vec<(&'static str, String)>` | OpenDAL **更灵活** |
| **operation 标签** | ❌ 无 (依赖 call site 推断) | ✅ `operation: &'static str` 字段 + `with_operation` | OpenDAL **更显式** |
| **status (retry 决策)** | `is_retryable()` boolean 方法 (覆盖 4 个 variant) | ✅ `ErrorStatus` 三态 enum (Permanent / Temporary / Persistent) | OpenDAL **粒度更细** |
| **backtrace** | ❌ 无 (依赖 caller location) | ✅ 可选 `Backtrace`, 但仅 `Unexpected` kind 启用 | OpenDAL **更接近生产实践** |
| **source chain** | `RetryExhausted { last_error: Box<Self> }` (typed) | `source: Option<anyhow::Error>` (type-erased) | synthia **强类型**, OpenDAL **多源混合** |
| **Display 格式** | per-variant `#[error("...")]` (single-line) | 自定义 `Display` impl + Debug 双格式 | 各有所长 |
| **IntoResponse 集成** | `IntoResponse for synthia_core::Error` (axum feature gated) | `From<Error> for io::Error` | 两者**对称** (都需 axum feature gating) |
| **公开 API 稳定性** | Tier 1 wire + Tier 2 variant (ADR-0007) | Tier 1 kind + Error struct 字段 | 对等 |
| **公开 enum kind 数量** | 36 | 12 | synthia **业务子分类更细** |
| **From impl 数量** | 6 (自动 + 手写) | 1 (From<Error> for io::Error) | synthia **覆盖面更广** |
| **学习曲线 (Rust 程序员)** | 1 小时 | 30 分钟 (更接近 std 习惯) | OpenDAL 略胜 |

### 6.1 关键不等价点 (Critical Asymmetries)

1. **`ErrorKind` 数量不可比**: OpenDAL 是 storage (12), synthia 是业务 (36).
   **直接套用会让 24 个业务 code 失去归属** (synthia 的 ProviderError / SessionError /
   ToolExecutionError / EvaluationError 等, 在 OpenDAL 12 变体中找不到对应).
2. **enum vs struct 是不可逆决策**: 一旦迁到 struct, 再想回到 enum 解构需要二次大手术.
3. **`#[track_caller]` 是 P2.2 红利**: OpenDAL 没有, synthia 有 — **synthia 在这个维度反超**.

---

## 7. Migration Cost (若完整采用)

### 7.1 工作量汇总 (Q4 复述)

| 任务 | LOC | 人天 |
|------|-----|------|
| Error enum → struct | +120 | 0.5 |
| 33 variant → kind+message+context | +80 | 1.0 |
| 重写 6 个 From impl | +120 | 0.5 |
| `Error::code()` 改 `kind()` 返回 `ErrorCode` | -50 | 0.5 |
| helper 方法改 builder | +50 | 0.2 |
| **变体解构 match 改写 (33 处)** | +165 | **2.0** |
| ServerError mapping | -20 | 0.2 |
| synthia-session From impls | +30 | 0.5 |
| **下游 crate match Error (19 处)** | +95 | **1.5** |
| 测试改动 | +200 | 1.0 |
| 文档 | +100 | 0.5 |
| **合计** | **~890 LOC** | **~8.0 人天** |

### 7.2 Partial Adoption 工作量 (仅借鉴 builder)

| 任务 | LOC | 人天 |
|------|-----|------|
| 加 `with_context(k, v)` builder | +15 | 0.1 |
| 加 `with_operation(op)` builder | +10 | 0.1 |
| 加 `set_source(err)` builder | +5 | 0.05 |
| 文档 + example | +30 | 0.1 |
| **合计** | **~60 LOC** | **~0.3 人天** |

**ROI 差异**: Partial adoption 是 27x 便宜.

### 7.3 破坏性矩阵 (若完整迁移)

| 调用模式 | 现有代码 | 迁移后 | 工作量 |
|----------|----------|--------|--------|
| `match err { Error::NotFound { item, .. } => ... }` | enum 解构 | ❌ 失效, 改 `if e.kind() == ErrorKind::NotFound` | 高 |
| `match err { Error::Validation { message, .. } => ... }` | 字段解构 | ❌ 失效, 改 `.context.get("message")` 字符串 match | 高 |
| `err.is_retryable()` | boolean 方法 | ✅ 仍可保留 (走 kind() + match) | 低 |
| `err.code()` | 返回 ErrorCode | ⚠️ 改 `err.kind()` 返回 `&'static str` (在 synthia-core 中替代, ErrorCode 已迁出) | 低 |
| `?` 自动 From 转换 | `From<X>` 自动 | ❌ 失效, 全部手写 `e.map_err(|e| Error::new(...).set_source(e))` | **极高** |
| `Error::from(e: reqwest::Error)` | 一行 derive | ❌ 改 ~30 行手写 builder 链 | 中 |

---

## 8. Path Forward 推荐

### 8.1 不推荐: 完整 OpenDAL 迁移

**理由**:
- 8.0 人天工作量 vs 0.3 人天 partial, ROI 不可接受
- enum → struct 是不可逆决策, 但收益集中在 per-frame context (可独立借鉴)
- ErrorKind 12 vs ErrorCode 36 数量差异, **直接套用会让 24+ 业务子分类失去归属**
- synthia 已有 `#[track_caller]` helper (P2.2 红利), **OpenDAL 这边反而缺**
- P2 已落地的 `ErrorCode + Error` 双层 **结构性等价于 OpenDAL**, 只差 builder 三件套

### 8.2 推荐: Partial Adoption — P3-A "Builder 三件套"

**改造目标** (仅在 `crates/synthia-core/src/error/error.rs` 加 3 个 builder 方法):

```rust
impl Error {
    /// OpenDAL-style per-frame context. 追加一个 (k, v) 键值对到错误上下文,
    /// 用于调用方在错误上附加运行时信息 (例: `err.context("session_id", id)`).
    pub fn with_context(mut self, key: &'static str, value: impl ToString) -> Self {
        // 借用 ErrorStreamVariant / RetryExhausted 中某个变体作为容器?
        // 或扩展 Error 加 `context: Vec<(&'static str, String)>` 字段?
        // ...
    }

    /// OpenDAL-style operation label. 显式标注错误来自哪个 operation
    /// (例: `err.with_operation("session::load")`).
    pub fn with_operation(self, op: &'static str) -> Self { ... }

    /// OpenDAL-style source attachment. 包装底层错误为 source, 在 Display 输出
    /// "source: ..." 后缀.
    pub fn set_source<E: std::error::Error + Send + Sync + 'static>(
        mut self, source: E,
    ) -> Self { ... }
}
```

**实施前置**:
- [ ] **设计决策**: context 字段加在 Error 上 (引入 Vec, 改 struct 形状) 还是用 anyhow::Error 包装?
  - 倾向: 不动 enum 形状, 用 `Error::Context(Vec<(&'static str, String)>)` 新 variant 作容器
    (类似 OpenDAL 的 `context` 字段, 但作为 enum 变体避免 struct 化)
- [ ] 在 5 个高频 variant 上保留 `#[track_caller]` helper, builder 是补充非替代
- [ ] 文档 + 1-2 个 example 在 `crates/synthia-core/examples/`
- [ ] 触发条件: **P3 trigger** = `Error::Provider(String)` 等 String-payload 变体开始丢失上下文时

### 8.3 不推荐: 借鉴 `status` (Permanent/Temporary/Persistent)

**理由**:
- synthia 已有 `is_retryable()` boolean, 覆盖 4 个 variant
- 加 status 字段需要修改 Error 结构, ROI 不明
- OpenDAL 的 status 主要给 RetryLayer 用, synthia 没有 RetryLayer 这种集中组件
- **保持当前 `is_retryable()` 即可**, 如果未来需要三态再升级

### 8.4 不推荐: 借鉴 `source: anyhow::Error`

**理由**:
- synthia `RetryExhausted { last_error: Box<Self> }` 是 typed (强类型)
- 改为 anyhow = 退到 type-erased, **违反 Tier 2 稳定性** (ADR-0007)
- 仅在无法 typed 时 (跨 crate 任意错误) 才考虑

---

## 9. 决策汇总

| 方向 | Verdict | 工作量 | ROI |
|------|---------|--------|-----|
| **完整 OpenDAL 迁移** (Error enum → struct) | ❌ **Skip** | 8.0 人天 / 890 LOC | 不可接受, 不可逆, 失 enum 解构 |
| **Partial: builder 三件套** (`with_context` + `with_operation` + `set_source`) | ✅ **Adopt as P3-A** | 0.3 人天 / 60 LOC | 27x 便宜于完整迁移 |
| **借鉴 `ErrorStatus` 三态** | ⚠️ **Defer** | 1.0 人天 | 现有 `is_retryable()` 够用 |
| **借鉴 `source: anyhow::Error`** | ❌ **Skip** | 0.5 人天 | 违反 Tier 2 typed source |
| **借鉴 `Backtrace`** | ⚠️ **Defer to P4+** | 2.0 人天 | depends on `std::error::provide` 稳定 |

### 最终建议

**保持现状 + 加 3 个 builder 方法** = 既有 thiserror enum 的全部能力 (解构 + 穷举)
+ OpenDAL 的核心价值 (per-frame context).

不迁移到 struct, 不加 status, 不换 source 形态.

**Trigger 重新评估的条件**:
- 业务子分类 > 50 个 `ErrorCode` 变体 (从 36 涨到 50+)
- 出现 > 5 个 String-payload variant 严重丢失上下文 (Provider/Session/Tool 等)
- snafu 整体迁移被采纳 (则 builder 三件套随 snafu 自动获得, 不需独立做)

---

## 10. 证据附录

### 10.1 OpenDAL 源码 permalinks

(SHA `a88bc848010959e97c708a6bba7bd1b01f0615ac`, HEAD of `main` on clone date 2026-08-05)

| 内容 | URL |
|------|-----|
| `ErrorKind` enum (12 variants) | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L49-L89> |
| `ErrorStatus` enum | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L131-L156> |
| `Error` struct 定义 | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L228-L238> |
| `Display` impl | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L240-L268> |
| `Debug` impl | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L270-L311> |
| `Error::new` | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L321-L341> |
| `with_operation` | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L349-L356> |
| `with_context` | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L359-L362> |
| `set_source` | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L369-L374> |
| `kind()` accessor | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L431-L433> |
| `From<Error> for io::Error` | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/error.rs#L501-L511> |
| `new_request_build_error` (builder 用法示例) | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/http_util/error.rs#L25-L29> |
| `with_error_response_context` (chain 用法) | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/http_util/error.rs#L55-L67> |
| `new_std_io_error` (From 替代示例) | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/std_io_util.rs#L29-L49> |
| `match err.kind()` in std_io_util | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/std_io_util.rs#L32-L40> |
| `Err(e) if e.kind() == X` in flat_list | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/raw/oio/list/flat_list.rs> |
| 官方 `if e.kind() == ErrorKind::NotFound` 推荐 | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/types/operator/operator.rs> |
| RFC-0977 设计 rationale | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/docs/rfcs/0977_refactor_error.md> |
| RFC-0044 早期设计 | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/docs/rfcs/0044_error_handle.md> |
| RFC-0247 retryable error 触发 | <https://github.com/apache/opendal/blob/a88bc848010959e97c708a6bba7bd1b01f0615ac/core/core/src/docs/rfcs/0247_retryable_error.md> |

### 10.2 synthia 当前 permalinks

| 内容 | URL |
|------|-----|
| `Error` enum (33 variants, P2 已加 5 个高频 `location`) | `crates/synthia-core/src/error/error.rs` L27-L138 |
| `ErrorCode` enum (36 variants, `#[non_exhaustive]`) | `crates/synthia-core/src/error/error_code.rs` L27-L63 |
| 5 个 `#[track_caller]` helper | `crates/synthia-core/src/error/error.rs` L144-L185 |
| `From<reqwest::Error>` / `From<serde_json::Error>` / `From<serde_yaml::Error>` | `crates/synthia-core/src/error/error.rs` L264-L294 |
| `Error::code()` (35-arm match → ErrorCode) | `crates/synthia-core/src/error/error.rs` L225-L261 |
| `ServerError` enum (8 variants + `#[non_exhaustive]`) | `crates/synthia-server/src/error.rs` L19-L28 |
| `map_core_error_code` (wire 映射) | `crates/synthia-server/src/error.rs` L70-L93 |
| `IntoResponse` impl | `crates/synthia-server/src/error.rs` L30-L68` |

### 10.3 synthia 数据 (本 ADR 引用的 grep 汇总)

```text
$ rg -t rust "Error::" crates/ --count | awk -F: '{ sum += $2 } END { print sum }'
568

$ rg -t rust "match err|match self|match error" crates/ --count | awk -F: '{ sum += $2 } END { print sum }'
68   # 跨 47 个文件

$ rg -t rust "Error::NotFound|Error::Validation|Error::Internal|Error::AlreadyExists|Error::InvalidItem" crates/ --count
33   # 主要在 synthia-agent/error + tests + From impls

$ rg -t rust "is_retryable\(\)|is_rate_limited\(\)" crates/ --count | awk -F: '{ sum += $2 } END { print sum }'
30

$ rg -t rust "Error::code\(\)|\.code\(\)" crates/ --count | awk -F: '{ sum += $2 } END { print sum }'
10

$ rg -t rust "impl From<.+> for Error|impl From<.+> for synthia_core::Error" crates/
crates/synthia-core/src/error/error.rs:3     # reqwest / serde_json / serde_yaml
crates/synthia-session/src/error.rs:3         # SessionError / StoreError / StateMachineError
```

---

## 11. References

- ADR-0007 (P2 错误架构决策): `docs/architecture/adr/0007-error-architecture-p2.md`
- P2 错误生态对比报告: `docs/architecture/error-ecosystem-comparison.md` §1.7 (OpenDAL)
- Apache OpenDAL `ErrorKind + Error` 架构:
  <https://opendal.apache.org/docs/rust/opendal_core/types/struct.Error.html>
- OpenDAL RFC-0977 "refactor-error": <https://opendal.apache.org/docs/rust/opendal_core/docs/rfcs/rfc_0977_refactor_error/index.html>
- OpenDAL RFC-0044 早期错误设计: <https://opendal.apache.org/docs/rust/opendal_core/docs/rfcs/rfc_0044_error_handle/index.html>
- thiserror vs snafu vs anyhow 对比:
  <https://github.com/dtolnay/thiserror/blob/master/README.md>
- GreptimeDB "Error handling in Rust" (snafu 选型):
  <https://greptime.com/blogs/2024-05-07-error-rust>
- iroh "Error handling in iroh" (snafu + n0-snafu):
  <https://www.iroh.computer/blog/error-handling-in-iroh>
