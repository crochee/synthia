# Brainstorm: landlock-fallback

## 背景

project_memory 把 `landlock fallback` 列为 P1-3，明确策略是：
- bubblewrap 工程细节是 P0；
- landlock 作为容器环境的 fallback 是 P1；
- seccomp 明确不做（"landlock 已是 fallback，seccomp 是 fallback 的 fallback"）。

当前代码状态：
- `synthia-sandbox` 已有 `SandboxManager` / `SandboxAttempt` / `SandboxPolicy` 抽象；
- `BubblewrapBackend` 已实现；
- `LandlockBackend` 是 stub，始终返回 `Unavailable`；
- `production-tool-execution-sandbox` change 已把 landlock 定义为可选特性，但未实现。

## 问题

在容器（Docker/Kubernetes）或无 user namespace 的环境里，`bwrap --unshare-all` 会失败。当前行为是 fail-closed（返回 `Unavailable`），这导致大量 bash 命令被直接拒绝。需要在 bubblewrap 不可用且系统支持 Linux Landlock LSM 时，提供一个无需 user namespace 的轻量级文件系统沙箱 fallback。

## 决策链

Q1: 是否要实现真正的 Landlock 后端，还是只做配置层包装？
- A: 实现真正的后端。`landlock` crate 已存在且封装了 Landlock ABI v1-v4，直接引入依赖成本可控。

Q2: Landlock 与 bubblewrap 的关系是什么？
- A: 主从 fallback，不是并列。优先检测 bubblewrap；只有在 bubblewrap 不可用且 Landlock 可用时才 fallback 到 Landlock。

Q3: 是否需要新的抽象层？
- A: 不引入新 trait。复用 `SandboxManager` trait，新增 `LandlockBackend` 实现即可。

Q4: 在容器里 Landlock 一定可用吗？
- A: 不一定。Landlock 需要内核 5.13+ 且未被 seccomp/LSM 禁用。因此仍需 `OnUnavailable` fail-closed 行为。

Q5: 沙箱策略如何映射到 Landlock？
- A:
  - `SandboxPolicy::None` -> `SandboxAttempt::None`
  - `SandboxPolicy::Standard` -> workspace 可读写，系统只读目录（/usr, /bin, /lib, /lib64, /sbin, /proc, /dev）只读
  - `SandboxPolicy::Strict` -> workspace 可读写，其余全部禁止
  - `SandboxPolicy::Custom` -> 仍返回 `Unavailable`（超出本 change 范围）

Q6: 子进程如何应用 Landlock 规则？
- A: 在子进程启动后、exec 前，调用 `landlock_restrict_self`。这需要 Rust landlock crate + `std::process::Command::pre_exec`（unsafe）或 nix::sched::clone_child 等机制。最简洁的方式是用一个小的 wrapper：先为当前进程设置规则，再 exec 目标程序。

Q7: 是否需要 landlock crate 的版本选择？
- A: 使用当前 crates.io 最新稳定版 `landlock`，启用 compat 特性以支持 ABI 自动降级。

Q8: 与 `production-tool-execution-sandbox` change 是否冲突？
- A: 不冲突。该 change 定义了抽象和 bubblewrap；本 change 在其基础上填充 Landlock 后端，保持接口不变。

## 设计取舍

**方案 A: 在 `LandlockBackend::select` 中设置规则，然后 spawn 一个 wrapper 二进制**
- 优点：规则设置代码集中；wrapper 可用 Rust 写。
- 缺点：需要额外二进制或动态生成 wrapper，增加部署复杂度。

**方案 B: 在父进程通过 `pre_exec` 设置规则**
- 优点：无需额外二进制。
- 缺点：`pre_exec` 是 unsafe；在 tokio::process::Command 中没有直接 pre_exec hook，需要使用 std::process::Command 或 nix。

**方案 C: `LandlockAttempt::wrap` 把原命令改写为 `sh -c 'landlock-setup && exec original'`**
- 优点：无需 unsafe，纯命令行。
- 缺点：依赖 shell；规则设置代码需要单独编译进一个可调用的小程序；stdout/stderr 路径复杂。

**选择：方案 B 的变体。** 用 `std::process::Command` 的 `pre_exec` 在子进程中设置 Landlock 规则，再通过 tokio::process 包装。具体实现细节在 design.md 中展开。

## 风险

- Landlock 兼容性：内核版本、LSM 顺序、seccomp 可能禁止 `landlock_restrict_self`。
- `pre_exec` 的 async-signal-safety 限制。
- 与 `tokio::process::Command` 的 stdio 配置兼容。

## 结论

聚焦实现 `LandlockBackend`，使其在 bubblewrap 不可用时作为 fallback 提供文件系统隔离。不引入新 trait，不实现 seccomp，不处理 `Custom` 策略。范围约 500 行，符合 project_memory 估算。
