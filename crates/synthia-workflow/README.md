# synthia-workflow

Declarative multi-agent workflow runtime: serde `WorkflowSpec` (agent
/ fan-out / pipeline), one `WorkflowHost` effect seam, caps +
concurrency + live control, JSONL journal with prefix replay.

> 公开 API 与 spec 格式见 [`synthia-workflow/src/lib.rs`](src/lib.rs)。

## 用法

```toml
[dependencies]
synthia = { version = "0.1", features = ["workflow"] }
```

```rust
use synthia_workflow::{WorkflowSpec, WorkflowHost};
```

## CI 契约

- `cargo test -p synthia-workflow --lib` 绿；
- 不依赖运行时类型（`make check-public-api-runtime`）。

## Examples（3 个）

| Example | Shows |
| :--- | :--- |
| `workflow_fanout` | `WorkflowSpec` fan-out + gated call + JSONL journal replay |
| `workflow_best_of` | `Step::BestOf`：N 次尝试首个 gate pass 的 |
| `workflow_mcts` | MCTS 风格的节点选择 / 回填 |

跑： `cargo run --example <name> -p synthia-workflow`。
