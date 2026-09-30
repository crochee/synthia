# synthia-steering

Steering layer: `Guard` / `AgentHook` / `Hint` / `Tracker` /
`OutputTransformer` + `HookMap` typed registry. A `Steering` is a
container that the harness consults each turn; you compose it with
`Steering::noop()` + `.add_guard(g)` + `.add_hook(h)` + ….

> 详细 seam 与公开 API 见 [`synthia-steering/src/lib.rs`](src/lib.rs)；
> 7 大组件的可替换点见 [`SEAMS.md`](../../SEAMS.md) §1。

## 用法

```rust
use synthia_steering::{Steering, Guard};

let steering = Steering::default_policy(root)
    .add_guard(Guard::max_tool_calls(20))
    .add_hint("Always cite file paths as workspace-relative.");
```

## CI 契约

- `cargo test -p synthia-steering --lib` 绿；
- 默认 features 零运行时依赖（`make check-no-runtime`）。
