## Why

当前 `synthia-sandbox` 的 Linux 后端只有 bubblewrap。在 Docker、Kubernetes 或禁用 user namespace 的宿主机上，`bwrap --unshare-all` 会失败，导致 `SandboxAttempt::Unavailable` 并触发 fail-closed 拒绝大量 bash 工具调用。Linux Landlock LSM（内核 5.13+）可以在不依赖 user namespace 的情况下限制子进程的文件系统访问，正是容器环境需要的轻量级 fallback。本 change 实现真正的 Landlock 后端，填补 bubblewrap 不可用时的隔离缺口，同时保持现有 `SandboxManager` 抽象不变。

## What Changes

**Landlock 后端实现**
- From: `LandlockBackend::select` 是 stub，始终返回 `Unavailable`；`SandboxAttempt::Landlock` 的 `wrap` 返回 `UNSUPPORTED`。
- To: 引入 `landlock` crate，实现 `LandlockBackend` 的真实检测与规则设置；`SandboxAttempt::Landlock::wrap` 在子进程 exec 前调用 `landlock_restrict_self`。
- Reason: 容器环境 bubblewrap 失败率高，需要无 user namespace 的 OS 级隔离。
- Impact: `landlock` feature 启用时 Linux 平台多一条 fallback 链；默认 feature 不变，行为兼容。

**Fallback 顺序**
- From: Linux Standard/Strict 策略只尝试 bubblewrap，失败后直接 `Unavailable`。
- To: 增加 `CompositeSandboxManager`，按 `BubblewrapBackend -> LandlockBackend -> Unavailable` 顺序选择。
- Reason: Landlock 是 fallback 而非替代，bwrap 仍是默认。
- Impact: 需要把现有调用点从 `BubblewrapBackend` 切换到 `CompositeSandboxManager`（CLI/server 配置层）。

**沙箱策略映射**
- From: `Standard`/`Strict` 对 Landlock 无定义。
- To: `Standard` 允许 workspace 读写 + 系统目录只读；`Strict` 只允许 workspace 读写。
- Reason: 与 bubblewrap 的默认规则对齐，降低认知成本。
- Impact: `Custom` 策略仍返回 `Unavailable`，不在本 change 范围。

## Capabilities

### New Capabilities
- `landlock-fallback`: 在 bubblewrap 不可用时使用 Linux Landlock LSM 提供文件系统沙箱隔离。
- `composite-sandbox-selection`: 按 bubblewrap -> landlock -> unavailable 的顺序自动选择可用后端。

### Modified Capabilities
- 无。本 change 不修改现有 spec 能力的行为契约，仅填充未实现的后端。

## Impact

- **代码层面**：修改 `synthia-sandbox` crate；新增 `landlock` crate 依赖（feature gated）；新增 `CompositeSandboxManager`。
- **API 层面**：`SandboxManager` trait 与 `SandboxAttempt` 枚举保持不变。
- **依赖层面**：Linux 下可选依赖 `landlock` crate；运行时仍要求内核 5.13+ 且 LSM 未禁用 Landlock。
- **UX 层面**：容器内 bash 工具调用成功率提升；不可用路径仍保持 fail-closed。
