# synthia-sandbox

Sandbox backend abstraction for the Synthia agent runtime.

This crate selects and configures OS-level sandboxing mechanisms for external
tool invocations. It exposes a uniform [`SandboxManager`] interface over
multiple backends and a [`SandboxAttempt::wrap`] helper that rewrites a
[`tokio::process::Command`] so the target program runs inside the chosen
sandbox.

## Supported backends

- **Bubblewrap** (`bwrap`) — Linux namespace-based sandbox using bind mounts.
- **Landlock** — Linux filesystem access-control sandbox (optional, behind the
  `landlock` feature).
- **Noop** — disables sandboxing entirely.

## Feature flags

| Feature     | Default | Description                                      |
|-------------|---------|--------------------------------------------------|
| `landlock`  | No      | Enables the Landlock fallback backend.           |
| `seccomp`   | No      | Reserved for future seccomp-bpf support (stub).  |

Build with Landlock support:

```bash
cargo build -p synthia-sandbox --features landlock
```

Run the Landlock integration test:

```bash
cargo test -p synthia-sandbox --features landlock
```

## Kernel requirements

- **Bubblewrap** requires a Linux kernel with user namespaces and `bwrap`
  installed on `PATH`.
- **Landlock** requires **Linux 5.13 or newer** with the Landlock LSM enabled.
  The `landlock` crate probes the running kernel at runtime; on older kernels
  or non-Linux platforms the backend reports `Unavailable` and the composite
  manager falls back to the next backend.

## Backend selection

Use [`CompositeSandboxManager::default_linux`] to obtain the standard fallback
chain:

1. Try **Bubblewrap** first (full namespace + filesystem isolation).
2. If Bubblewrap reports `Unavailable`, try **Landlock** when the `landlock`
   feature is enabled.
3. If neither backend is available, return `SandboxAttempt::Unavailable`.

`SandboxPolicy::None` is always short-circuited to `SandboxAttempt::None`
without querying any backend.

## Differences from Bubblewrap

Landlock is **filesystem-only** sandboxing. Unlike Bubblewrap, it does **not**
create a new mount, PID, network, or IPC namespace. Programs wrapped with
Landlock still see the host filesystem layout; their access is restricted by
path-based rules.

As a result:

- Landlock cannot hide files outside the workspace; it can only deny access to
  them.
- Network, IPC, and process-namespace isolation are not provided by Landlock.
- Landlock has lower overhead and fewer deployment requirements than
  Bubblewrap, making it a useful fallback when `bwrap` is unavailable.
