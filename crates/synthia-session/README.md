# synthia-session

Session lifecycle and state management: `Session` is the time-ordered
stream of `SessionEvent`; `SessionSink` is the trait consumers
implement to persist events (in-memory, filesystem, remote, …).

> 详细 seam 与公开 API 见 [`synthia-session/src/lib.rs`](src/lib.rs)；
> 7 大组件的可替换点见 [`SEAMS.md`](../../SEAMS.md) §1。

## 用法

```rust
use synthia_session::{Session, SessionSink, MemorySink, SessionEvent};

let mut session = Session::new(id, MemorySink::default());
session.append(SessionEvent::UserMessage("hello".into())).await?;
```

## CI 契约

- `cargo test -p synthia-session --lib` 绿；
- 默认 features 零 DB 依赖（`make check-mvp-deps`）。
