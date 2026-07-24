## Context

Synthia 的 P1-P10 设计原则强调系统级不信任、fail-closed 与可观测性。`synthia-sandbox` crate 已经定义了 `SandboxManager` / `SandboxAttempt` / `SandboxPolicy` 抽象，`BubblewrapBackend` 也已实现。但 `LandlockBackend` 仍是 stub，导致在容器或无 user namespace 环境中所有 Standard/Strict 策略的工具调用都会因 `Unavailable` 被拒绝。

project_memory 明确把 `landlock fallback` 列为 P1-3，策略是：
- bubblewrap 工程细节是 P0；
- landlock 作为容器环境的 fallback 是 P1；
- seccomp 明确不做。

本 change 在保持现有抽象不变的前提下，实现真正的 Landlock 后端并引入一个轻量级的组合选择器，使 bubblewrap 不可用时能自动降级到 Landlock。

## Goals / Non-Goals

**Goals:**
1. 实现 `LandlockBackend`，在 Linux 内核支持时提供真实的文件系统沙箱隔离。
2. 提供 `CompositeSandboxManager`，按 `Bubblewrap -> Landlock -> Unavailable` 顺序自动选择后端。
3. 保持 `SandboxManager` trait 与 `SandboxAttempt` 枚举不变，确保接口兼容。
4. 通过 feature flag 控制 landlock 依赖，默认不启用，避免 SDK/轻量场景负担。
5. 补齐 `synthia-sandbox` 的 Landlock 集成测试与单元测试。

**Non-Goals:**
1. 不实现 seccomp 后端（project_memory 明确不做）。
2. 不扩展 `SandboxPolicy::Custom` 的解析与映射。
3. 不修改 `ToolOrchestrator` 或 `BashTool` 的上层调用逻辑（仅切换其注入的 `SandboxManager` 实现）。
4. 不实现 macOS/Windows 沙箱后端。

## Decisions

### D1：使用 `landlock` crate 而不是直接系统调用
- **选择**：依赖 crates.io 上的 `landlock` crate（带 `compat` feature）。
- **理由**：该 crate 已封装 ABI v1-v4 检测与自动降级，避免手写 unsafe syscall 和复杂的内核版本判断。
- **已考虑 alternative**：
  - 直接调用 `landlock_create_ruleset` / `landlock_restrict_self`：需要自行处理 ABI 版本、LSM 顺序、错误码，重复造轮子。
  - 使用更重的沙箱库（e.g. `capsicum`）：超出 Linux Landlock 范围，且与项目"明确不做 seccomp"冲突。

### D2：Landlock 规则设置在子进程 exec 前完成
- **选择**：`LandlockAttempt::wrap` 把原命令替换为一个 wrapper 流程：先 `fork`/`pre_exec` 设置规则，再 exec 原程序。
- **理由**：Landlock 规则只能限制调用 `landlock_restrict_self` 的进程及其后代；必须在 exec 前应用。
- **已考虑 alternative**：
  - 在父进程设置规则后 spawn：规则会应用到父进程，错误。
  - 编译一个独立 wrapper 二进制：部署复杂，与单 crate 目标冲突。
  - 使用 `Command::pre_exec` 直接：tokio::process::Command 不暴露 pre_exec；需要转换为 std::process::Command 再包装。

### D3：采用 `CompositeSandboxManager` 而非修改 `BubblewrapBackend`
- **选择**：新增独立的 `CompositeSandboxManager`，内部持有多个 backend 并按优先级尝试。
- **理由**：保持每个 backend 的职责单一；未来增加新 backend 时无需改动现有 backend。
- **已考虑 alternative**：
  - 在 `BubblewrapBackend` 内部 fallback 到 Landlock：违反单一职责，测试与配置变得复杂。
  - 让调用方手动尝试多个 backend：把选择逻辑泄漏到每个调用点。

### D4：`Standard` 策略允许 workspace 读写 + 系统目录只读，`Strict` 仅允许 workspace
- **选择**：与当前 bubblewrap 的默认 mount 行为对齐。
- **理由**：降低策略语义差异；用户切换 backend 时行为一致。
- **已考虑 alternative**：
  - Landlock 也按 path 白名单动态计算：实现复杂，且与 bubblewrap 的固定系统目录列表不一致。

### D5：`landlock` 依赖默认关闭，通过 Cargo feature 启用
- **选择**：`synthia-sandbox` 的 `landlock` feature 保持为空声明，但新增 optional dependency `landlock`；实际代码在 `#[cfg(feature = "landlock")]` 下编译。
- **理由**：project_memory 要求用 cargo feature 平衡企业级（重）与 SDK（轻）；默认禁用可保持 SDK 场景轻量。
- **已考虑 alternative**：
  - 默认启用：会增加 SDK 用户的不必要依赖与编译时间。

### D6：不可用时仍 fail-closed
- **选择**：`CompositeSandboxManager` 最终仍可能返回 `SandboxAttempt::Unavailable`；调用方继续按现有 `OnUnavailable` 处理。
- **理由**：符合 project_memory 硬约束"Permission policy must default to 'AskUser' (fail-closed)"与沙箱设计原则。
- **已考虑 alternative**：
  - 静默降级到无沙箱：违反 fail-closed，引入安全风险。

## Risks / Trade-offs

[Risk] 内核/LSM 不支持 Landlock 时检测代码报错或误判。
→ Mitigation: 使用 `landlock` crate 的 ABI 检测；在 `select` 阶段捕获错误并返回 `Unavailable`，绝不 panic。

[Risk] `pre_exec` / fork 在 async 上下文中引入死锁或 fd 泄漏。
→ Mitigation: 使用 `std::process::Command` + `pre_exec` 的最小 unsafe 块；设置 `FD_CLOEXEC`；在单测中覆盖 stdout/stderr 管道场景。

[Risk] Landlock 与 bubblewrap 的隔离语义不完全等价（Landlock 不做 namespace 隔离）。
→ Mitigation: 文档明确说明 Landlock 是文件系统沙箱 fallback，不替代网络/进程隔离；`Strict` 策略限制更严。

[Risk] 容器 seccomp profile 禁止 `landlock_restrict_self`。
→ Mitigation: 检测阶段调用 crate 的 `compat` API  probing；失败返回 `Unavailable`，走 fail-closed。

[Trade-off] `CompositeSandboxManager` 引入一个额外选择层。
→ 接受理由：几十行代码，换来 backend 可组合、可测试，且 bubblewrap 与 landlock 的选择顺序对上层透明。

## Migration Plan

1. **Phase 0：依赖与接口**
   - 在 `synthia-sandbox/Cargo.toml` 添加 optional `landlock` dependency。
   - 保持 `SandboxManager` trait、`SandboxAttempt`、`SandboxPolicy` 不变。

2. **Phase 1：LandlockBackend 实现**
   - 重写 `crates/synthia-sandbox/src/backends/landlock.rs`。
   - 实现 `LandlockBackend::select`：检测 ABI 可用性、根据策略构建规则、返回 `SandboxAttempt::Landlock`。
   - 实现 `SandboxAttempt::Landlock::wrap`：通过 `pre_exec` 在子进程应用规则。

3. **Phase 2：CompositeSandboxManager**
   - 新增 `crates/synthia-sandbox/src/composite.rs`。
   - 在 CLI/server 配置层把 `BubblewrapBackend` 替换为 `CompositeSandboxManager`。

4. **Phase 3：测试与验证**
   - 单元测试：规则构建、ABI 检测、策略映射。
   - 集成测试：在支持 Landlock 的 Linux 环境验证工作区外读取失败。
   - 运行 `cargo clippy --all-targets --all-features --tests --all`。

**Rollback 策略**：每个 phase 独立可回退；`landlock` feature 默认关闭，不影响默认构建。

**验收条件**：
- `cargo check --features landlock` 与默认 `cargo check` 均通过。
- 集成测试在 Landlock 可用时验证隔离语义。
- `CompositeSandboxManager` 在 bubblewrap 可用时仍优先选择 bubblewrap。

## Open Questions

1. `pre_exec` 中是否可以安全地调用 `landlock_restrict_self` 而不触发 `tokio` 的 fd 警告？是否需要改用 `rustix` / `nix` 的 `clone` 控制？
2. 容器环境中 Landlock 的 ABI 检测是否需要在 `select` 阶段实际创建并销毁一个临时 ruleset 来探测？
3. 是否需要在 `SandboxConfig` 中暴露 `landlock_first` 等配置开关，还是固定 bubblewrap 优先？
