# synthia-tool

The tool paradigm: `Tool` trait, `ToolRegistry`, exposure projection,
`workspace` path confinement, and the two synthetic contracts
(`__get_full_output`, `structured_output`).

**No agent-facing tool set ships here** — every brick of the model-
facing tool kit lives in its own plugin crate
(`synthia-tool-{read,write,shell,todo,web,task,scheduler,search}`).
A consumer assembles whichever subset it needs by
`registry.register_entry(ToolEntry::new(Arc::new(my_tool)))`.

> 公开 API 见 [`synthia-tool/src/lib.rs`](src/lib.rs)；
> 4 大组件的可替换点见 [`SEAMS.md`](../../SEAMS.md) §1。

## 用法

```rust
use synthia_tool::{ToolRegistry, ToolEntry, Tool};
use std::sync::Arc;

let mut reg = ToolRegistry::new();
reg.register_entry(ToolEntry::new(Arc::new(MyTool::default())));
```

## CI 契约

- `cargo test -p synthia-tool --lib` 绿；
- 不引入 `reqwest|hyper|rustls|h2|tower`（`make check-mvp-deps`）。

## Examples（3 个）

| Example | Shows |
| :--- | :--- |
| `tool_groups` | `GroupedRegistry`：命名组 + 独立激活 |
| `argument_validation` | R74 同 `EchoTool` / off / on，错误 JSON 校验 |
| `deferred_tools` | `ToolExposure::Deferred`：name-only → 全 schema |

跑： `cargo run --example <name> -p synthia-tool`。
