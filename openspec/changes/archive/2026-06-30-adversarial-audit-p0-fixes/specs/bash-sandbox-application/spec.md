## ADDED Requirements

### Requirement: Bash Executor Must Apply Selected Sandbox

bash 工具的 executor 与后台 spawn 路径 MUST 显式调用 `SandboxAttempt::wrap(&mut command)` 应用编排器选定的沙箱，禁止以 `_sandbox_attempt` 形参名显式忽略或丢弃沙箱实例。

#### Scenario: Sandbox Available Wraps Command

- **WHEN** orchestrator 选择了 Bubblewrap 沙箱且沙箱可用
- **THEN** bash executor 调用 `SandboxAttempt::wrap(&mut command)` 后再执行，命令在 bwrap 隔离环境（`--unshare-all` / `--die-with-parent` / 只读 bind）内运行

#### Scenario: Sandbox Unavailable Denies By Default

- **WHEN** orchestrator 选择的沙箱不可用（如 bwrap 二进制缺失）且 `SandboxPolicy` 为 Standard
- **THEN** bash 工具 MUST 返回 Deny 而非以父进程权限裸跑命令，错误信息指明沙箱不可用

#### Scenario: Sandbox Policy Allow Permits Bare Run

- **WHEN** `SandboxPolicy` 显式配置为 Allow（如测试环境）
- **THEN** bash executor 可在无沙箱下执行，但 MUST 在日志中记录 "sandbox disabled by policy"

#### Scenario: Background Spawn Path Applies Sandbox

- **WHEN** bash 工具以后台模式 spawn 子进程（`trait_impl.rs` 的 background 路径）
- **THEN** 后台 spawn 路径 MUST 同样调用 `SandboxAttempt::wrap`，禁止绕过沙箱
