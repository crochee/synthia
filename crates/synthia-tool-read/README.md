# synthia-tool-read

The `read` builtin: workspace file reader with line ranges. Reads a
UTF-8 text file within the workspace, optionally restricted to a
1-based line range. Output is prefixed with right-aligned line numbers.

> Plugin crate for `synthia-tool`. See [`SEAMS.md`](../../SEAMS.md) for
> the tool seam; see the docs in [`src/lib.rs`](src/lib.rs) for the
> full API.

## 用法

```rust
use synthia_tool_read::ReadTool;

let tool = ReadTool::default();
let out = tool.execute(ctx).await?;
```

## CI 契约

- 离线（无 HTTP / DB / OTel），`make check-mvp-deps` 断言；
- `cargo test -p synthia-tool-read --lib` 绿。
