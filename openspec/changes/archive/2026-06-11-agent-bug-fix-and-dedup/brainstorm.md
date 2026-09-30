<!--
Raw capture of multi-expert adversarial brainstorming session (2026-06-10).

本檔捕捉 6 位专家对抗性审查的完整决策链：
- 6 名专家子代理（R1 架构师、R2 安全、R3 性能、R4 Rust、R5 并发、R6 魔鬼代言人）
- 苏格拉底式追问 + 多方对抗
- 用户在 4 个决策的分歧点上裁决"接受元结论"

设计文档 docs/superpowers/specs/2026-06-10-agent-bug-fix-and-dedup-design.md 是重组后的结构化设计。
本檔是 raw decision log，与 design.md 互不重叠。
-->

# Brainstorming Raw Capture — Agent Bug Fix & Dedup

## Session: 2026-06-10

## Method: 6-Expert Adversarial Review

每位专家独立审查 4 个原设计决策（D1-D4），使用：
- 苏格拉底式追问（"为什么是 X 而不是 Y？"）
- 假设证伪（"如果我反对，最强论据是什么？"）
- 失败模式分析（"6 个月后这个设计如何失败？"）

专家团队：
- **R1** Principal Architect（架构边界、抽象层次）
- **R2** Security Specialist（攻击者思维、FAIL-CLOSED、TOCTOU）
- **R3** Performance Engineer（零成本抽象、热路径预算）
- **R4** Rust Language Expert（trait 惯用法、Send/Sync、生命周期）
- **R5** Concurrency/Distributed Systems（竞态、死锁、Cancellation）
- **R6** Devil's Advocate（挑战共识、寻找认知偏差、YAGNI）

---

## Background（背景）

**原始意图**：分析 Synthia 与生产级 AI Agent（opencode、codex）差距，发现 5 大重复：
- 3× LoopDetector
- 3× Permission 系统
- 2× Sandbox
- 2× ReAct 主循环
- 2× Circuit Breaker

**原提案方向**（4 个 trait 抽象）：
- **D1**: `LoopDetector` trait（4 方法：id/observe/verdict/reset）
- **D2**: `PermissionPolicy` 拆分（read-only + mutable sub-trait）
- **D3**: `OsSandbox` trait（wrap_command 签名 + 平台回退）
- **D4**: `Message::cache_control` 显式标记（CacheBreakpoint enum）

用户已接受 R7+R8 翻转（Cache Control 显式标记优于 Provider 层自动标记），并要求完整设计文档。

---

## 决策链 Q1-Qn

### Q1: 重复的精确数量是多少？（R1 实际核查）

R1 通过 grep 与文件遍历核查原提案的重复数量：
- "3× LoopDetector" 实际是 **1.5× 重复 + 1× 异类**（Guardian 完整 + Agent 简化 + DoomLoopDetector 手写绕过 trait）
- "3× PermissionPolicy" 实际是 **4 套**（漏算了 `synthia-tool::exec::permission`，且该套存在**编译错误**）
- "2× Sandbox" 实际是 2 套字段近似但语义不同（`SandboxExecutor` vs `Sandbox`）
- "2× ReAct" 实际是 1 套新 1 套旧
- "2× Circuit Breaker" 待核查

**R1 结论**：D1/D2 提案的"问题陈述"夸大了重复数量。

### Q2: 当前实现有哪些已存在 bug？（6 专家共发现 7 个事实）

| # | Bug | 文件 | 严重度 | 共识 |
|---|-----|------|--------|------|
| C1 | `cache_control_hash = compute_hash(system_content)` 独立信号坍塌 | cache.rs:235 | 🔴 Critical | 6/6 |
| C2 | `MergedPolicy::evaluate(unknown) = Allow` fail-open 默认 | merged_policy.rs:53-64 | 🔴 Critical | 6/6 |
| C3 | `try_write` 静默丢记录 | step.rs:489 | 🟠 High | 5/6 |
| C4 | 4 套 PermissionPolicy + 编译错误 | synthia-tool/exec/permission.rs | 🟠 High | 6/6 |
| C5 | 3 套 LoopDetector（非 2 套） | 多处 | 🟡 Medium | 6/6 |
| C6 | O(N)/N² 算法 + JSON clone | loop_detection.rs:53-57, 215 | 🟡 Medium | 5/6 |
| C7 | `OsSandbox` trait 完全不存在 | (no file) | 🟡 Medium | 6/6 |

### Q3: 4 个 trait 提案各自的命运？（苏格拉底式追问）

#### Q3.1: D1 `LoopDetector` trait — 4 方法是否最优？

- **R1 反方（强）**：`DoomLoopDetector` 已经"叛逃"——它在 `LoopDetectorSet` 里手写，不走 trait。代码自己投了反对票。
- **R4 反方（中）**：`Cow<'static, str>` 永远 `Borrowed`，触发 `clippy::redundant_allocation`。
- **R5 反方（弱）**：`verdict(&self)` 拿不到 `observe(&mut self)` 刚写入的状态——**API 错配**。
- **R3/R4 正方（弱）**：统一后 telemetry 一致、可插拔。
- **R6 裁决**：trait **不抽**。保留 `LoopDetectorSet`，删除 `agent::LoopDetector` 重复。

#### Q3.2: D2 `PermissionPolicy` 拆分 — 解决真问题吗？

- **R2/R6 反方（极强）**：在错误抽象上做精细化 = 债上加债。`RuleSet` 兼容垫片已是技术债证据。
- **R1 反方（强）**：`MergedPolicy` 是活跃模型，`PermissionPolicy`（旧 struct）**应该死亡**。
- **R4 反方（中）**：`&mut self` 在 `Arc<RwLock<>>` 下是死锁陷阱。
- **R3 正方（弱）**：读路径 < 100 ns 已经够快，**根本不需要** sub-trait 优化。
- **R6 裁决**：sub-trait **不做**。先**删除** `synthia-permission::policy::PermissionPolicy` + `RuleSet`。

#### Q3.3: D3 `OsSandbox` trait — 统一了什么？

- **R1/R2/R5/R6 反方（4 票）**：seccomp/landlock/JobObject **零实现**。提议"统一抽象" = 抽象的是不存在的代码。Linux 中心化会污染 macOS/Windows。
- **R3/R4 正方（2 票）**：trait 边界即使不完美也是必要的（防止 Linux 假设渗漏）。
- **R6 警告**：`sandbox-exec` Apple deprecated；`AppContainer` 是 token 不是 wrap；3 平台根本没有可对应的"wrap_command"。
- **裁决（4:2）**：trait **不做**。先在 Linux 实现 1 个真 Landlock sandbox。

#### Q3.4: D4 `Message::cache_control` 显式标记 — 优于 TwoPartPrompt 吗？

- **R1/R4 正方（条件）**：仅在**合并到 TwoPartPrompt** 前提下接受。
- **R2/R3/R5/R6 反方（4 票）**：
  - 当前 `cache_control_hash = system_hash` 证明"独立信号不存在"（C1）
  - LongTtl 跨 session 会泄漏敏感数据
  - `CacheBreakpoint` 是 Anthropic 中心化（OpenAI/Gemini/Bedrock 协议不同）
  - 与 `SectionCaching` 概念正交但都做"cache 标记"，会产生 12 种 nonsensical 组合
- **裁决**：在 `Message` 加 `cache_control` 字段**不做**。先修 C1（独立 hash `CacheControlMark`）。是否真做 LongTtl 标记取决于 cache 命中率是否 <70%。

### Q4: 元结论 — 应该按什么顺序执行？

R1/R2/R5/R6 共同元结论：
> **"原 4 个 D 提案的'问题陈述'是真实的，但'解决方案'过度设计。在没有完整理解每套现有实现的语义边界之前，抽 trait 是把不安全的现状封装成看起来安全的接口——这违反 P6（Distrust by Default）。"**

R6 终极元结论：
> **"在错误抽象上做精细化 = 拖延真正的清理。"**

R1 建议执行顺序：
1. 立即修 5 个必修复的 bug
2. 删除 3 套重复（不抽 trait）
3. 6 个月后再回头看是否需要 trait 抽象

### Q5: 用户的最终裁决？

用户对 AskUserQuestion 的回答：
> "接受元结论（推荐）"

— 放弃原 D1-D4 全部 trait 提案；按 R1 建议的 3 阶段执行。

### Q6: 4 个 Open Questions 的答案（用户隐含接受）

| # | Open Question | 用户隐含答案 |
|---|---------------|---------------|
| 1 | P1.2 破坏性变更（fail-open → fail-closed） | 接受（用户接受元结论隐含接受此变更） |
| 2 | P2.3 命名（sandbox → command_blacklist） | 接受 |
| 3 | Phase 1 时间（1-2 天） | 接受 |
| 4 | Phase 3 重新评估窗口（6 个月） | 接受 |

---

## 多方对抗辩论的关键交锋

### 交锋 1: D1 trait vs enum dispatch
- **R1 主张**：`LoopDetectorSet` 已经是结构体内嵌（单态化），trait 化引入 vtable + 堆分配。
- **R3 反驳**：当前 `agent::LoopDetector` 是 `Arc<RwLock<>>` 包装，已经有间接性。
- **R1 再反驳**：`Arc<RwLock<>>` 的间接性是并发所需，不是抽象所需。trait 化**额外**加 vtable。
- **R6 终结**：保留 `LoopDetectorSet`，**删除** `agent::LoopDetector`（消除竞争源）→ 间接性消失。

### 交锋 2: D2 sub-trait 必要性
- **R4 主张**：读写分离避免 `Mutex` lock。
- **R3 反驳**：当前 `PermissionChecker` 持 owned policy，根本**没有 lock**。Clone 模式就够了。
- **R6 终结**：sub-trait 是为**不存在的需求**做的优化。**真需求**是删除旧 `PermissionPolicy` struct。

### 交锋 3: D3 OsSandbox 抽象时机
- **R4 主张**：先定 trait 边界，再填实现。
- **R1 反驳**：在 zero implementation 状态下定 trait = 锁定**错误语义**。Linux Landlock 是 per-process filter，`sandbox-exec` 是 declarative profile，**根本不是同一类操作**。
- **R6 终结**：先在 1 个平台实现 1 个真 sandbox，**再**讨论抽象。

### 交锋 4: D4 cache_control 字段位置
- **R1 主张**：`Message` 是**最小对话单位**，加 cache 字段污染所有 caller mental model。应该改在 `TwoPartPrompt::finalize` 内部。
- **R2 反驳**：跨 session 共享 cache 时，namespace 必须含 user_id——**`TwoPartPrompt` 不知道 user_id**。
- **R4 终结**：D4 强制独立字段，违反 P1（prefix 字节级稳定：旧 JSON 与新 JSON 字段顺序不同 → 全量 cache miss）。必须**字段末尾化**或**合并到 TwoPartPrompt**。

---

## 沉默的反方（R6 指出：谁没在房间里）

1. `synthia-permission` 的**用户**（被 trait 复杂度影响最大）
2. **macOS 开发者**（被 D3 中心化设计影响最大）
3. **OpenAI-only 用户**（被 D4 Anthropic 中心化设计影响最大）
4. **cache 命中率运维**（关心 LongTtl 实际能省多少 token）

→ 共识 + 缺席 = 必然的认知偏差放大。
→ 推迟 trait 抽象 = 留时间让这些"不在场"的人参与下次决策。

---

## 关键洞察（来自不同专家）

- **R1**: "trait 化是'为统一而统一'。4 处重复里有 1.5× 是真实结构重复，2.5× 是命名/字段差异——后者用 alias 而不是 trait 解决。"
- **R2**: "`MergedPolicy` 默认 `Allow` 是 CVE 级别的 fail-open 漏洞。`bash -c` 是单点失败。`cache_control_hash` 是真值表坍塌。"
- **R3**: "3-5 ms/任务浪费在 O(N) 算法和 JSON clone——是真实性能 bug，不是抽象级别问题。"
- **R4**: "`Cow<'static, str>` 必触发 clippy::redundant_allocation。`&mut self` 在 `Arc<RwLock<>>` 下是死锁陷阱。`&mut Command` 在 std vs tokio 下不是同一类型。"
- **R5**: "`try_write` 静默丢记录——信息被默默删除，违反 P8。`cache_control_hash` 字段坍塌——`CacheBreakDetector` 的"显式标记检测" 实际失效。`Command` 跨 `.await` 不是 Send——破坏 async 执行流。"
- **R6**: "DoomLoopDetector 已经叛逃；RuleSet 是技术债的证据；OsSandbox 是'伪通用性陷阱'；cache_breakpoint 是 Hyrum's Law 必触发。"

---

## 最终决策（一句话）

**取消原 D1-D4 全部 trait 提案。** 执行 3 阶段：
1. 修 5 个 critical bug（C1-C4, C6）
2. 删 3 套重复（P2.1, P2.2, P2.3 改名）
3. 推迟 trait 抽象 ≥6 个月（带 re-evaluation 门槛条件）
