<!--
Raw capture of brainstorming output.

本档原样捕捉 brainstorming skill 的产出，不强制结构。
Skill 的自然产出通常是 decision log 格式（背景 → 决议链 Q1-Qn → 设计取舍），
但依对话内容可能有不同组织方式。

design.md 从本档萃取并重新整理为结构化设计文件。

不要将本档的内容复制到 design.md — design.md 是独立的重组产物，
两者互补但不重叠。
-->

# 对抗性审计决策日志

## 背景

synthia 与 3 个生产级 AI agent（opencode / codex-rs / pi-mono）的差距分析经历了两个阶段：
1. **探索阶段**：4 个并行探索子代理产出综合报告，识别 12 个"差距"和 4 个领先点
2. **对抗性批判阶段**：4 个对抗性视角（架构 / 安全 / 性能 / 生产就绪）挑战探索发现

批判阶段的核心方法：每个批判代理必须实际读取代码验证，不能臆测；必须区分"真差距"与"不同设计哲学"；必须追到 sink 端（能力是否被调用），不能停留在 source 端（能力是否齐全）。

## 决议链

### Q1：探索阶段列出的 12 个"差距"有多少是真差距？

**判定**：12 个中仅 1 个真差距，3 个部分真，8 个伪差距。

**证据**：
- **applyCachePolicy 短路**：[apply_cache_policy](file:///home/crochee/workspace/synthia/crates/synthia-provider/src/cache_policy.rs#L103) 已完整实现，Rust 用覆盖写入替代引用相等短路，零分配
- **Durable/Ephemeral split**：[AgentEvent::is_durable()](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/events/event_enum.rs#L217) 已实现，有 [test_durable_event_classification_consistency](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/events/tests.rs#L246)
- **Source trait**：[source/mod.rs](file:///home/crochee/workspace/synthia/crates/synthia-context/src/source/mod.rs) 完整定义 Source trait + SourceDelta(Changed/Unchanged/Removed) + SourceEpoch
- **AgentTool 占位符**：[agent_tool.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/agent_tools/agent_tool.rs) 完整实现，含 SlotGuard/树取消/background 模式
- **Compaction cut-point**：synthia 用 in-place 变换 + 幂等标记，不删消息，结构上不可能产生孤儿 toolCall
- **Retry Promise 同步创建**：Node.js 事件循环特有竞态，Rust oneshot 无此问题（范畴错误）
- **18-provider overflow regex**：synthia 用结构化枚举替代正则，编译期穷尽（更优设计）
- **stdout takeover guard**：Node.js monkey-patch，Rust crossterm 原生支持（范畴错误）
- **hook trust 状态机**：唯一真差距（企业多租户场景必需，[HookRegistry](file:///home/crochee/workspace/synthia/crates/synthia-hook/src/lib.rs) 不区分来源）

**根因**：探索阶段只读表面层未追踪深层实现。看到 `CachePolicy::default()` 注入就判定"无 provider 感知"，未追踪到 provider transform 层的 `apply_cache_policy` 调用。

### Q2：原 project_memory 的 P0 清单是否仍然有效？

**判定**：4 项中 3 项已完成、1 项降级。project_memory 已严重过时两次。

**证据**：
- **P0-1 cache_breaker 移除**：`grep cache_breaker` 全 crates 0 命中 → 已完成
- **P0-2 applyCachePolicy**：[cache_policy.rs:103](file:///home/crochee/workspace/synthia/crates/synthia-provider/src/cache_policy.rs#L103) 已存在 + 测试 → 已完成
- **P0-4 bash PermissionChecker**：[bash_tool/trait_impl.rs:54](file:///home/crochee/workspace/synthia/crates/synthia-tool-bash/src/bash_tool/trait_impl.rs#L54) `requires_permission() = true`，[registry.rs:179-198](file:///home/crochee/workspace/synthia/crates/synthia-tool/src/registry/registration/registry.rs#L179-L198) 强制 check → 已完成
- **P0-3 bubblewrap 工程细节**：[wrap_with_bubblewrap](file:///home/crochee/workspace/synthia/crates/synthia-sandbox/src/lib.rs#L107) 已存在，但缺 /tmp tmpfs、/etc ro-bind、env 消毒 → 部分完成，降为 P1-SaaS-only

**根因**：openspec change 的 tasks.md 大量未更新是系统性问题。landlock-fallback 代码已落地但 tasks 0/25；p0-subagent-execution-session-persistence §2-§5 已实现但 tasks.md 全 0。

### Q3：探索阶段是否遗漏了真正的 P0 阻塞项？

**判定**：是的，遗漏了 4 个隐藏 P0 + 1 个安全单点失效 + 3 个性能瓶颈。

**隐藏 P0**：
- **H1**：[wire-tool-orchestrator-into-agent-runtime](file:///home/crochee/workspace/synthia/openspec/changes/wire-tool-orchestrator-into-agent-runtime/tasks.md) 0/12，`Agent::run_stream` 不调用 `build_default_tool_orchestrator()`，整个编排工作链路不可达（"超级 P0"）
- **H2**：[user-id-namespace](file:///home/crochee/workspace/synthia/openspec/changes/user-id-namespace-and-bash-permission-gate/tasks.md) §1 0/11，`session_dir` 未按 user_id 分目录，任意用户可枚举他人会话（安全红线，仅 SaaS）
- **H3**：[production-tool-execution-sandbox](file:///home/crochee/workspace/synthia/openspec/changes/production-tool-execution-sandbox/tasks.md) §5 全未做，read_file/write_file/apply_patch/search_files 仍是 stub
- **H4**：[p0-subagent-execution-session-persistence](file:///home/crochee/workspace/synthia/openspec/changes/p0-subagent-execution-session-persistence/tasks.md) §1 0/10，LoopContext 不从 SessionMetadata 恢复

**安全单点失效 U1**：
- [synthia-tool-orchestrator/src/lib.rs:731](file:///home/crochee/workspace/synthia/crates/synthia-tool-orchestrator/src/lib.rs#L731) `ToolAdapter::execute` 形参 `_sandbox_attempt`（下划线 = 显式忽略）
- [executor.rs:32](file:///home/crochee/workspace/synthia/crates/synthia-tool-bash/src/bash_tool/executor.rs#L32) bash 直接 `Command::new("bash").arg("-c")`，从不调用 `SandboxAttempt::wrap()`
- 影响：bash 是唯一执行任意 attacker-influenced code 的工具，`--unshare-all`/`--die-with-parent`/只读 bind 全部对 bash 一行都没生效
- 补齐所有 bwrap 工程细节而 U1 不修，等于把锁芯装在没合上的门上

**性能瓶颈**：
- [reviewer.rs:217,313](file:///home/crochee/workspace/synthia/crates/synthia-guardian/src/review/reviewer.rs#L217) guardian 路径 `cache_policy: None`，~$19.7K/年浪费（超过所有"差距"加起来）
- [prefix_tracker/tracker.rs:90-97](file:///home/crochee/workspace/synthia/crates/synthia-context/src/prefix_tracker/tracker.rs#L90-L97) 只 hash system_bytes，忽略 tools + messages，stability_ratio 虚高（违反 P9 可观测性）
- [pipeline.rs:38-48](file:///home/crochee/workspace/synthia/crates/synthia-context/src/assembler/pipeline.rs#L38-L48) `trimmed.remove(0)` 是 O(n²)，200+ 消息时 100-250ms 延迟

### Q4：synthia 的 4 个领先点是否真的领先？

**判定**：3 个真领先，1 个需深化。

- **5 层循环检测**：真领先。PollNoProgressDetector 独有，opencode 仅 1 层 doom_loop
- **tool_result_cleared_at 幂等标记**：真领先。符合 P2 Append-Only，codex 用原地修改违反 P2
- **derive_subagent_permission 只继承 Deny**：真领先。符合 P6 最严格体现
- **每 5 轮 self_reflect**：机制存在但"每 5 轮"频率控制需验证（可能每轮都 reflect，浪费 token）

### Q5：跨语言对比的公平基准是什么？

**判定**：codex-rs 是唯一公平对照基准。

- opencode 是 TypeScript，Linux 上基本无沙箱（synthia 的部分 bwrap 已领先 opencode）
- pi-mono 是 TypeScript，其防护（toolCall ID 规范化、prototype pollution 防、stdout takeover）在 Rust 强类型 + 编译期单态化下天然消除
- codex-rs 是 Rust，exec.rs / process_group.rs / safety.rs / linux_run_main.rs 四个文件构筑了完整纵深，是公平对比基准

**批判含义**：把 TS 项目的运行时优化平移到 Rust 是认知偏差。Rust 的零成本抽象已消除多数 TS 项目的运行时开销需求。

## 设计取舍

### 取舍 1：本变更的范围边界

**决策**：仅捕获未被现有 openspec change 覆盖的、审计新发现的、高 ROI 修复项。

**理由**：H1/H2/H3/H4 已有对应 openspec change（wire-tool-orchestrator / user-id-namespace / production-tool-execution-sandbox / p0-subagent-execution-session-persistence），不应重复。本变更聚焦：
- U1 bash 沙箱接入（安全单点失效，未被任何 change 覆盖）
- U2 文件工具路径校验（安全，未被覆盖）
- guardian cache_policy: None（性能，未被覆盖）
- prefix_tracker hash 范围（可观测性，未被覆盖）
- pipeline.rs O(n²) remove(0)（性能，未被覆盖）

**反例**：不包含 hook trust 状态机（虽是真差距，但属企业场景 P2，非 P0）；不包含已被现有 change 覆盖的 H1/H2/H3/H4。

### 取舍 2：修复顺序

**决策**：按"修复成本 vs 影响"排序，而非按原始 P0/P1 分级。

1. **P1-9（pipeline.rs O(n²)）**：30 分钟修复，100-250ms 延迟改善 → ROI 最高
2. **P0-G（guardian cache_policy）**：1 小时修复，~$19.7K/年节省 → 成本收益最高
3. **P0-E（U1 bash 沙箱接入）**：2-3 小时修复，安全单点失效消除 → 安全收益最高
4. **P0-F（U2 文件工具路径校验）**：3-4 小时修复，防 /etc/passwd 直通 → 安全收益高
5. **P1-8（prefix_tracker hash 范围）**：2 小时修复，可观测性修正 → 基础设施收益

### 取舍 3：U1 修复策略

**决策**：在 bash executor 与后台 spawn 路径调用 `SandboxAttempt::wrap`；沙箱 unavailable 时按 `SandboxPolicy::on_unavailable()` 决策（Standard → Deny 而非裸跑）。

**反例**：不做"黑名单增强"（U3 危险命令子串匹配）——黑名单只能作 defense-in-depth 提示，不能作安全边界。只有沙箱 + path 包含才配称安全边界。

### 取舍 4：U2 修复策略

**决策**：参照 codex [safety.rs:138-193](file:///home/crochee/workspace/codex/codex-rs/core/src/safety.rs#L138-L193) 的纯路径 `normalize()` + workspace 前缀包含判定，替代 `../` 子串检查。

**反例**：不引入 pi-mono 的 per-realpath file mutation queue（符号链接场景罕见，当前 SubagentManager 已有 try_acquire_slot 兜底）。

## 关键约束

- 本变更为 explore 模式产出，捕获审计发现，不重复已有 change 的工作
- 所有修复项必须基于代码事实，已有 file:/// 引用
- 跨语言范畴错误已排除（不引入 TS 特定模式）
- codex-rs 是唯一公平对照基准
