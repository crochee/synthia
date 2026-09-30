# synthia-tool-todo

The `TodoWrite` builtin: structured task-list state the model can
maintain.

> Plugin crate for `synthia-tool`. See [`SEAMS.md`](../../SEAMS.md) for
> the tool seam.

## CI 契约

- 离线（无 HTTP / DB / OTel），`make check-mvp-deps` 断言；
- `cargo test -p synthia-tool-todo --lib` 绿。
