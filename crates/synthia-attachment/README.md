# synthia-attachment

Multimodal attachment store (image / audio / file bytes) with
filesystem + in-memory backends.

> 详细 seam 与公开 API 见 [`synthia-attachment/src/lib.rs`](src/lib.rs)
> 与顶层 [`SEAMS.md`](../../SEAMS.md)。本 README 是该 crate 在
> `README` 顶层的索引页。

## 用法

```rust
use synthia_attachment::{AttachmentStore, ImageAttachmentRef};

let store = AttachmentStore::in_memory();
let image = ImageAttachmentRef::from_bytes_at(&store, b"...", clock)?;
```

## 依赖方向

- `synthia-attachment` → `synthia-core`（仅 trait：`Clock` / `Error`）
- 不依赖 `synthia-provider` / `synthia-tool` / 任何运行时。

## CI 契约

- `cargo test -p synthia-attachment --lib` 绿；
- 不引入 `reqwest|hyper|rustls|h2|tower`（`make check-mvp-deps`）。
