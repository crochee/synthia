# synthia

The facade: every piece behind feature flags plus a curated `prelude`,
so a consumer can depend on one crate and assemble from zero.

> 公开 API 与 4 步 MVP guide 见
> [`MINIMAL.md`](../../MINIMAL.md)；7 大组件的可替换点见
> [`SEAMS.md`](../../SEAMS.md)。

## 用法

```toml
[dependencies]
synthia = { version = "0.1", features = ["provider-anthropic", "tool-read", "harness"] }
```

```rust
use synthia::prelude::*;
```

## CI 契约

- `cargo test -p synthia --lib` 绿；
- 零逻辑 facade：`lib.rs` / `*.rs` 只做 `pub use`，不写 `fn` / `impl`
  （`make check-pub-surface` 棘轮化）；
- 默认 features 零 HTTP / DB / OTel 依赖（`make check-mvp-deps`）。
