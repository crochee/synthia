# synthia-tool-shell

The `shell` tool plugin: bounded command executor that owns its
execution policy (`synthia-tool-shell::sandbox` module). The shell
plugin is the **only** tool with a process-level sandbox policy,
because that's where the policy actually matters. The
`ExecutionPolicy` / `SandboxBackend` / `BwrapBackend` /
`effective_policy` etc. live here, not in `synthia-tool`, so the
description the model sees and the argv the harness actually spawns
are guaranteed to come from the same value.

> Plugin crate for `synthia-tool`. See [`SEAMS.md`](../../SEAMS.md) for
> the tool seam; see [`src/sandbox.rs`](src/sandbox.rs) for the policy
> types.

## CI 契约

- 离线（无 HTTP / DB / OTel），`make check-mvp-deps` 断言；
- `cargo test -p synthia-tool-shell --lib` 绿。
