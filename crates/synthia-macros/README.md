# synthia-macros

Procedural macros for synthia: `#[derive(Tool)]` and
`#[tool(name, description, mode)]` — generate a `synthia_tool::Tool`
impl from a struct + inherent `execute` body.

> 公开 API 见 [`synthia-macros/src/lib.rs`](src/lib.rs)。

## 用法

```rust
use synthia_macros::Tool;

#[derive(Tool, Default)]
#[tool(name = "echo", description = "Echoes the input")]
pub struct Echo;

#[async_trait::async_trait]
impl synthia_tool::Tool for Echo {
    async fn execute(&self, ctx: synthia_tool::ToolContext) -> synthia_tool::ToolResult { /* ... */ }
}
```

## CI 契约

- `cargo test -p synthia-macros --lib` 绿；
- proc-macro crate 不出现在 lib 公共 API 之外。
