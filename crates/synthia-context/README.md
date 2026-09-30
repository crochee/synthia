# synthia-context

Context-window management (`ContextManager` trait) with four strategies
out of the box: `NoopContextManager`, `TruncatingContextManager`,
`SummarizingContextManager`, `DagContextManager`. Plus a 3-layer
`Memory` tier (conversation / working / long-term) over the session
sink. SQLite persistence is optional behind the `sqlite` feature.

> 详细 seam 与公开 API 见 [`synthia-context/src/lib.rs`](src/lib.rs)；
> 4 大组件的可替换点见 [`SEAMS.md`](../../SEAMS.md) §3。

## 用法

```rust
use synthia_context::{TruncatingContextManager, ContextManager, ContextEvent};

let cm = TruncatingContextManager::default();
let mut history = Vec::new();
cm.append_event(&mut history, event)?;
```

## CI 契约

- `cargo test -p synthia-context --lib` 绿；
- `cargo test -p synthia-context --features sqlite` 绿
  （`make test-sqlite`）；
- 默认 features 零 SQL 依赖（`make check-mvp-deps`）。
