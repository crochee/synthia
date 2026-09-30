# synthia-tool-search

The `search` tool plugin: cross-domain search over a host-built
`synthia-search::Registry` (skills / memory / tools catalogs) with
deferred exposure. Cold start advertises only name + description; on
first call the schema upgrades to the actual `query` / `limit` /
`domain` shape.

> Plugin crate for `synthia-tool`. See [`SEAMS.md`](../../SEAMS.md) for
> the tool seam.

## CI 契约

- 离线（无 HTTP / DB / OTel），`make check-mvp-deps` 断言；
- `cargo test -p synthia-tool-search --lib` 绿。
