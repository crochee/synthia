## Context

synthia 是一个 Rust 实现的 AI agent，具有 27 个 crate 的分层架构。多专家对抗性审查（架构、性能可靠性、安全生产化三个视角）发现 5 个 P0 级确定性风险，均有代码证据。

**当前状态**：
- bash 工具超时/取消时不杀子进程，孙子进程变孤儿（确定性资源泄漏）
- L5 ResetCoordinator 的 ToolState/Full scope 返回 "not yet implemented"，触发 30s 冷却
- `should_stop` 仅检查 `max_iterations`，无 wall-clock 超时
- GuardianReviewer 的 `check()` 快速路径传入空 transcript，`make_guardian_decision` 用占位符 request

**重要发现**：`GuardianReviewer` 当前仅在测试中被实例化，生产路径使用 `GuardianCoordinator::check()` → `SimpleGuardian::check()`（规则快速路径）。但 GuardianReviewer 的 bug 仍需修复——代码存在且可能被接入生产路径。

**约束**：
- 遵循 agent_rule.md 的 P1-P10 原则
- 最小改动，不做重构
- 每个修复必须有测试
- Rust 编码规范：`cargo +nightly fmt --all` + `cargo clippy`

## Goals / Non-Goals

**Goals:**
- 消除 bash 进程组泄漏（确定性资源泄漏）
- 消除 L5 Reset 冷却死循环（6+错误进入冷却→FailFast）
- 增加 session 级 wall-clock 超时（防失控 session）
- 修复 GuardianReviewer 空 transcript bug（跨轮次 injection 防护）
- 修复 GuardianReviewer 占位符 request bug（用户确认指向错误动作）

**Non-Goals:**
- 不实现 L5 ToolState/Full reset（仅回退到 Conversation）
- 不重构 Guardian transcript 预算（仅修复 bug，预算作为后续改进）
- 不统一 PermissionChecker/CommandBlacklist（高优先级，非 P0）
- 不升级 apply_patch 模糊匹配（高优先级，非 P0）
- 不实现工具并发执行（高优先级，非 P0）

## Decisions

### D1: bash 进程组杀 — `process_group(0)` + `killpg`

- **选择**：使用 `Command::process_group(0)` 创建新进程组，超时/取消时 `killpg(SIGTERM)` → 3s → `killpg(SIGKILL)`，IO 排空 2s
- **理由**：codex 和 opencode 都用进程组杀；`kill_on_drop` 不够，孙子进程会变孤儿；`process_group(0)` 是 tokio::process 的原生支持
- **已考虑 alternative**：
  - `.kill_on_drop(true)` — 只杀直接子进程，不杀孙子进程，拒绝
  - `setsid` + `killpg` — 与选中方案类似但更显式，但 `process_group` 是 tokio 原生 API，更简洁

### D2: L5 Reset — 未实现 scope 回退到 Conversation

- **选择**：`determine_scope` 返回 ToolState/Full 时，回退到 Conversation 并添加 warning 日志
- **理由**：最小改动，消除冷却死循环；保留 determine_scope 设计，未来可实现
- **已考虑 alternative**：
  - 实现 ToolState/Full — 工作量大，需定义语义，拒绝（超出 P0 范围）
  - 移除 determine_scope，统一用 Conversation — 简化但丢失未来扩展性，拒绝

### D3: 全局 wall-clock 超时 — 放在 `should_stop` 中

- **选择**：在 `LoopContext::should_stop` 增加 wall-clock 检查，默认 30 分钟，可通过 `AgentConfig` 配置
- **理由**：`should_stop` 是循环退出条件，与 max_iterations 并列最自然
- **已考虑 alternative**：
  - 放在 SessionController 层 — 与 idle_timeout 混淆，拒绝
  - 放在 Agent::run_stream 入口 — 太外层，无法在循环中检查，拒绝

### D4: Guardian 空 transcript — `check()` 接收 conversation 参数

- **选择**：`check()` 签名增加 `conversation: &[Message]` 参数，传入最近 N 轮（有 token 预算限制）
- **理由**：bug 根源是快速路径缺乏上下文，必须修复；改签名是正确的
- **已考虑 alternative**：
  - 从 session 读取 conversation — 不改签名但需要 session 访问，增加耦合，拒绝
  - 文档化并降级决策权重 — 不修 bug，拒绝
- **注意**：当前 GuardianReviewer 未接入生产路径，但修复为未来接入做准备

### D5: Guardian 占位符 request — 透传实际 request

- **选择**：`call_llm_internal` 接收 `request` 参数，透传到 `make_guardian_decision`
- **理由**：占位符是明显 bug，用户确认指向错误动作
- **已考虑 alternative**：无（这是唯一合理的修复）

## Risks / Trade-offs

- [Risk] 进程组杀可能误杀父进程 → Mitigation: `process_group(0)` 创建新进程组，pgid=子进程 pid，不影响父进程
- [Risk] L5 回退到 Conversation 可能丢失工具状态 → Mitigation: 添加 warning 日志；未来实现 ToolState
- [Risk] 全局超时可能中断长任务 → Mitigation: 默认 30 分钟可配置；超时前发 warning 事件
- [Risk] Guardian check() 改签名影响调用方 → Mitigation: 当前仅测试调用，更新测试即可
- [Trade-off] 修复 Guardian bug 但 GuardianReviewer 未接入生产 → 接受理由：代码存在且可能被接入，修复为未来准备

## Migration Plan

N/A — 本 change 不涉及部署变更（纯代码修复，无 endpoint/DB 变更）。

**部署顺序**：
1. 修复 bash 进程组杀（独立，无依赖）
2. 修复 L5 Reset 回退（独立，无依赖）
3. 增加全局超时（独立，无依赖）
4. 修复 Guardian 两个 bug（可合并为一个 commit）

**Rollback**：每个修复独立，可单独 revert。

**验收条件**：
- `cargo +nightly fmt --all` 通过
- `cargo clippy --all-targets --all-features --tests --all` 无警告
- `cargo test` 全部通过
- 新增测试覆盖每个修复

## Open Questions

1. **GuardianReviewer 是否应接入生产路径？** 当前仅 SimpleGuardian 在生产路径。GuardianReviewer 的接入是高优先级架构差距，但超出本次 P0 范围。本次仅修复 bug，不接入。
2. **全局超时的默认值 30 分钟是否合理？** 需要用户反馈。可配置，初始 30 分钟。
3. **进程组杀的 IO 排空超时 2s 是否足够？** 借鉴 codex 的 `IO_DRAIN_TIMEOUT_MS=2s`，可调整。
