# synthia-tool-task

The `task` tool plugin: multi-agent delegation by running registered
peers as sub-agents (gates, worktree isolation, `@agent:` routing,
subagent admission, batched completion notification).

It is the first consumer of the
[`ToolInterceptor`](../../SEAMS.md) seam — i.e. it needs the loop's
internal capability (event forwarding, cancellation, depth), so it
depends on `synthia-harness` rather than just `synthia-tool`.

> Plugin crate for `synthia-tool`. See [`SEAMS.md`](../../SEAMS.md) for
> the interceptor seam.

## CI 契约

- 离线（无 HTTP / DB / OTel），`make check-mvp-deps` 断言；
- `cargo test -p synthia-tool-task --lib` 绿。

## Examples（3 个）

| Example | Shows |
| :--- | :--- |
| `fan_out_with_group_join` | `GroupJoin` 批后台 agent 一次性通知 |
| `delegation_gate` | 子任务 gate 命令必须通过 |
| `worktree_isolation` | 每个子 agent 一个 `synthia/agent-<ulid>` worktree |

跑： `cargo run --example <name> -p synthia-tool-task`。
