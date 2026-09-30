# synthia-tool-write

The `write` builtin: workspace file writer (overwrite / append).

> Plugin crate for `synthia-tool`. See [`SEAMS.md`](../../SEAMS.md) for
> the tool seam.

## CI 契约

- 离线（无 HTTP / DB / OTel），`make check-mvp-deps` 断言；
- `cargo test -p synthia-tool-write --lib` 绿。
