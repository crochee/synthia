## Context

synthia 经历了与 3 个生产级 AI agent（opencode / codex-rs / pi-mono）的差距分析。探索阶段识别 12 个"差距"，但 4 视角对抗性批判（架构/安全/性能/生产就绪）发现：

- **12 个差距中 8 个是伪差距**（已实现 / 设计哲学差异 / 跨语言范畴错误）
- **原 project_memory 的 P0 清单 4 项中 3 项已完成**（cache_breaker 已删、applyCachePolicy 已实现、bash PermissionChecker 已接入）
- **5 个未被任何现有 openspec change 覆盖的高 ROI 修复项**被审计发现

当前约束：
- bash 是唯一执行任意 attacker-influenced code 的工具，其沙箱被显式忽略（`_sandbox_attempt` 下划线）
- 文件工具不 spawn 子进程，沙箱层不会兜底，`../` 子串检查是唯一防线
- guardian 每次审查 ~10K token，`cache_policy: None` 导致 ~$19.7K/年浪费
- prefix_tracker 只 hash system_bytes，`stability_ratio` 虚高，违反 P9
- pipeline.rs `remove(0)` 是 O(n²)，200+ 消息时累计 100-250ms 延迟

利益相关者：
- SaaS 多租户用户：U1/U2 安全修复直接影响数据隔离
- 所有场景：cache_policy 与 prefix_tracker 影响成本与可观测性
- 开发者：O(n²) 修复影响长会话体验

## Goals / Non-Goals

**Goals:**
- 修复 bash 沙箱单点失效（U1）：bash executor 显式调用 `SandboxAttempt::wrap`，unavailable 时按 policy 决策
- 修复文件工具路径校验（U2）：用 workspace 包含判定替代 `../` 子串检查
- 消除 guardian 路径 cache miss 浪费：所有 LLM 调用路径携带 cache_policy
- 修正 prefix_tracker 可观测性盲区：hash 覆盖 system + tools + messages
- 修复 pipeline.rs O(n²) 反模式：改用 O(n) drain

**Non-Goals:**
- 不修复 H1/H2/H3/H4（已在现有 openspec change 中跟踪）
- 不实现 hook trust 状态机（真差距但属企业场景 P2，非 P0）
- 不增强危险命令黑名单（U3：黑名单只能作 defense-in-depth，不能作安全边界）
- 不实现 bwrap env_clear / PGID kill / IO_DRAIN_TIMEOUT（U1 修复后才相关，作为后续加固）
- 不引入 per-realpath file mutation queue（符号链接场景罕见，SubagentManager 已兜底）
- 不移植 TS 特定模式（Retry Promise / 18-provider regex / stdout takeover）

## Decisions

### D1：U1 修复策略 — 在 bash executor 调用 wrap，unavailable 时按 policy 决策

- **选择**：在 [bash executor](file:///home/crochee/workspace/synthia/crates/synthia-tool-bash/src/bash_tool/executor.rs#L32) 与 [后台 spawn 路径](file:///home/crochee/workspace/synthia/crates/synthia-tool-bash/src/bash_tool/trait_impl.rs#L108-L113) 显式调用 `SandboxAttempt::wrap(&mut command)`；沙箱 unavailable 时按 `SandboxPolicy::on_unavailable()` 决策（Standard → Deny 而非裸跑）
- **理由**：补齐 bwrap 工程细节而 U1 不修，等于把锁芯装在没合上的门上。U1 是 G1（bwrap 工程细节）/G3（二阶段沙箱）的根因放大器——只要 U1 不修，补齐 env_clear/PGID kill/seccomp 内层对 bash 全部是"装配了但没通电"
- **已考虑 alternative**：
  - "只增强黑名单"（U3）：拒绝。黑名单子串匹配可被 `rm -rf $HOME`/`find / -delete`/`r""m -rf /` 一行绕过，只能作 defense-in-depth 提示，不能作安全边界
  - "在 orchestrator 层 wrap"：拒绝。orchestrator 已正确选择沙箱并传入 `&sandbox_attempt`，问题在 ToolAdapter 丢弃它；在 orchestrator 层 wrap 会破坏"工具决定如何执行"的关注分离

### D2：U2 修复策略 — 纯路径 normalize + workspace 前缀包含

- **选择**：参照 [codex safety.rs:138-193](file:///home/crochee/workspace/codex/codex-rs/core/src/safety.rs#L138-L193) 的 `normalize()` + `can_write_path_with_cwd()` 纯路径运算，替代 `../` 子串检查
- **理由**：文件工具是纯 Rust 不 spawn 子进程，沙箱层不会兜底——`../` 子串检查是唯一防线。`read_file("/etc/passwd")`、`read_file("/home/victim/.ssh/id_rsa")` 全部通过。codex 的纯路径运算不调 `canonicalize()`，防 TOCTOU 符号链接逃逸
- **已考虑 alternative**：
  - "调 `canonicalize()` 后校验"：拒绝。引入 TOCTOU 竞态（校验与使用之间符号链接可被替换）
  - "引入 pi-mono per-realpath file mutation queue"：拒绝过度设计。该 queue 解决的是并发写同文件损坏，当前问题是无任何 workspace 包含校验

### D3：guardian cache_policy 修复 — 注入 Some(CachePolicy::default())

- **选择**：[reviewer.rs:217,313](file:///home/crochee/workspace/synthia/crates/synthia-guardian/src/review/reviewer.rs#L217) 与 [e2e_llm_test.rs:120,200](file:///home/crochee/workspace/synthia/crates/synthia-agent/tests/e2e_llm_test.rs#L120) 从 `cache_policy: None` 改为 `Some(CachePolicy::default())`
- **理由**：guardian 每次审查 ~10K token，每 session 触发 2 次，1k/day × 365 × 2 × 10K × ($3 - $0.30)/M = ~$19.7K/年。`CachePolicy::default()` 已在 [pipeline.rs:61](file:///home/crochee/workspace/synthia/crates/synthia-context/src/assembler/pipeline.rs#L61) 验证可用，guardian 应复用同一默认
- **已考虑 alternative**：
  - "为 guardian 定制细粒度 policy"：推迟。当前 `default()` 已足够，细粒度优化属 P2
  - "在 orchestrator 层统一注入"：部分采纳。orchestrator 应有默认 cache_policy 注入点，但 guardian 路径不走 orchestrator，需单独修复

### D4：prefix_tracker hash 范围 — 扩展到 system + tools + messages

- **选择**：[PrefixTracker::record_pre](file:///home/crochee/workspace/synthia/crates/synthia-context/src/prefix_tracker/tracker.rs#L90-L97) 从只 hash `system_bytes` 扩展到 hash `system_bytes + tools_schema_bytes + messages_prefix_bytes`
- **理由**：P1 原则明确 prefix 一致性涵盖 system_prompt + tools + messages。当前只 hash system 会导致 `stability_ratio` 虚高——system 没变但 tools 改了仍记为"stable"，掩盖真实 cache 失效
- **已考虑 alternative**：
  - "分三个独立 hash"：拒绝。增加复杂度且无额外价值；单一 hash 已能检测任何 prefix 变更
  - "只 hash tools，不 hash messages"：拒绝。messages 前缀（tool_result_cleared_at 之前的部分）也影响 cache

### D5：pipeline.rs O(n²) 修复 — 用 drain 替代 remove(0)

- **选择**：[pipeline.rs:38-48](file:///home/crochee/workspace/synthia/crates/synthia-context/src/assembler/pipeline.rs#L38-L48) 的 `trimmed.remove(0)` 改为 `trimmed.drain(start..end)` 一次性移除，或维护 `start_index` 而非 mutate Vec
- **理由**：`Vec::remove(0)` 每次把所有后续元素前移 O(n)，while 循环最多 n 次 → 总 O(n²)。200+ 消息时 100-250ms 延迟。`drain()` 是 O(n) 一次性移除，符合 Rust 惯用法
- **已考虑 alternative**：
  - "用 VecDeque"：拒绝过度设计。drain 已足够，VecDeque 会改变所有下游 API 签名
  - "用迭代器跳过"：可行但增加复杂度，drain 更直接

## Risks / Trade-offs

- [Risk] U1 修复后沙箱 unavailable 时 bash 将 Deny → 可能破坏依赖裸跑 bash 的现有测试 → Mitigation: 沙箱 unavailable 测试应显式用 `SandboxPolicy::Allow` 或 mock；新增"unavailable 时 Deny"测试
- [Risk] U2 纯路径 normalize 可能在符号链接密集工作区误判 → Mitigation: 保守实现，只做前缀包含不做 resolve；codex safety.rs 已验证该算法可行
- [Risk] guardian cache_policy 注入可能使 guardian 请求命中主对话的 cache → 意外收益（成本进一步降低），非风险
- [Risk] prefix_tracker hash 范围扩展可能使 stability_ratio 突然下降（暴露之前被掩盖的 cache miss）→ Mitigation: 接受。这是可观测性修正，暴露问题优于掩盖问题
- [Risk] pipeline drain 修复可能改变 ProtectionZone 的边界语义 → Mitigation: 保持 trim 逻辑不变，只改数据结构操作方式；现有测试应覆盖

[Trade-off] U1 选择"unavailable 时 Deny"而非"unavailable 时 Allow + 警告" → 接受理由：fail-closed 符合 P6（系统级不信任），宁可误杀也不让 bash 裸跑

[Trade-off] U2 不实现 per-realpath mutation queue → 接受理由：符号链接场景罕见，当前问题是无任何 workspace 包含校验，先解决主要矛盾

## Migration Plan

部署顺序（按 ROI 排序，每步独立可回滚）：

1. **P1（30 分钟）**：pipeline.rs drain 修复 → `cargo test -p synthia-context` 验证
2. **P2（1 小时）**：guardian/e2e cache_policy 注入 → `cargo test -p synthia-guardian` 验证
3. **P3（2-3 小时）**：U1 bash 沙箱接入 → 新增"unavailable 时 Deny"测试；`cargo test -p synthia-tool-bash` 验证
4. **P4（3-4 小时）**：U2 路径校验重写 → 新增"绝对路径越界 Deny"测试；`cargo test -p synthia-permission` 验证
5. **P5（2 小时）**：prefix_tracker hash 扩展 → `cargo test -p synthia-context` 验证 stability_ratio 计算正确

回滚策略：每个修复独立提交，可单独 revert。U1/U2 涉及安全语义，回滚前应确认无依赖新行为。

验收条件：
- `cargo +nightly fmt --all` 通过
- `cargo clippy --all-targets --all-features --tests --all` 无警告
- `cargo test --all` 全部通过
- 新增测试覆盖：U1 unavailable Deny、U2 绝对路径越界 Deny

## Open Questions

- prefix_tracker hash 扩展后，`stability_ratio` 的滑动窗口大小是否需要调整？（当前 `recent_window` 默认值可能基于只 hash system 的假设）
- guardian cache_policy 注入是否应该用更细粒度的 policy（如只缓存到 system 部分）而非 `default()`？（推迟到 P2 优化，当前 `default()` 已足够）
- U1 修复后，`SandboxPolicy::on_unavailable()` 的默认行为是否需要从 Standard 调整为更严格的 profile？（当前 Standard → Deny 已符合 P6，无需调整）
