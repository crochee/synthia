# synthia-server

HTTP / WebSocket server with axum: exposes the REST + SSE chat surface
that the React frontend (`synthia-web/`) talks to.

This is the **only** application crate in the workspace — the one
place where `tokio` / `axum` may appear in a public signature. Library
crates elsewhere must remain runtime-agnostic
(`make check-public-api-runtime`).

> 公开 API、路由清单与 `config.yaml` schema 见
> [`synthia-server/src/lib.rs`](src/lib.rs)。

## 用法

```sh
cargo run -p synthia-server -- --config config.yaml
# → HTTP on :8080, SSE chat on /api/v1/chat
```

## CI 契约

- `cargo test -p synthia-server --lib` 绿；
- `cargo build -p synthia-server --release` 产出单二进制
  （`make build-release`）；
- 是 `make ci` 的运行时豁免面（其它 lib crate 必须零 tokio）。
