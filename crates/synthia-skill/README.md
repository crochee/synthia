# synthia-skill

Skill registry (slash-command, prompt, and tool bundles): a named
catalog of reusable prompt + tool combinations the agent can adopt.

> 公开 API 见 [`synthia-skill/src/lib.rs`](src/lib.rs)。

## CI 契约

- `cargo test -p synthia-skill --lib` 绿；
- 不依赖运行时类型（`make check-public-api-runtime`）。
