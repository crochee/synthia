# Brainstorm: Synthia Gap Analysis vs OpenCode / Codex

> 这是 superpowers:brainstorming 的原始输出，保留为决策追溯证据。
> 关联设计：`design.md`，关联实现：`proposal.md` + `specs/*.md` + `tasks.md`。

---

## 背景

用户需求：分析当前 Synthia (Rust) AI Agent 跟生产级 AI Agent（OpenCode, Codex）的差距，识别可借鉴的优秀实现和重复逻辑。

**用户先验选择**（澄清后）：
- 优化方向：**补齐基础能力**（不是激进新特性）
- 参考对象：**两个都借鉴**（融合 OpenCode + Codex）
- 工作粒度：**先写完整设计文档**（openspec 流程）

---

## 决策链

### Q1: Synthia 当前的"骨架"是否完整？

**结论**：骨架完整，**主要问题是"骨架长好了但肌肉没接上"**。

证据：
- 27 个 spec 已落地（agent loop, context, tool, hook, guardian, memory, steering, multi-agent, permission, error recovery, telemetry, cron, evaluation, observability, structured logging...）
- 但同 spec 内 4-5 套重复实现（prompt 5 套、compaction 3 套、truncate 4 套、token 计数 15 个文件、loop detection 2 套、prefix_hash 2 套）
- P1 (prefix 稳定) 和 P6 (不信任 LLM) 两条最高约束**落不到地**——prefix_tracker 全部孤岛 API，无任何调用方

### Q2: Synthia 硬骨头（按 ROI 排序）

| 优先级 | Gap | 严重度 | 借鉴对象 |
|---|---|---|---|
| #1 | `is_concurrency_safe` 硬编码 false | 🔴 Critical | OpenCode `Tool` 工厂显式声明 |
| #2 | PrefixTracker 孤岛，cache 命中率不可观测 | 🔴 Critical | OpenCode `llm.ts:103-128` 2 段式 + Codex `compact.rs:204-218` |
| #3 | `trim_to_budget` O(n²) | 🔴 Critical | OpenCode `compaction.ts` 单遍扫描 |
| #4 | Tool output 无落盘截断，1GB log 爆 context | 🟠 High | OpenCode `truncate.ts:130-141` |
| #5 | 5 套 prompt 组装路径并存 | 🟠 High | OpenCode 单 `ContextAssembler` |
| #6 | 15 个文件写 token 估算，差 5-10× | 🟠 High | 单一 `TokenCounter` trait |
| #7 | `Pruning::hard_clear` 静默丢内容（违反 P8） | 🟡 Medium | Codex `rollout::recorder` JSONL |
| #8 | bash 截断 UTF-8 panic | 🟡 Medium | OpenCode `truncate.ts` char-boundary |
| #9 | Permission 字符串精确匹配 | 🟡 Medium | Codex `execpolicy` token-aware |
| #10 | `read_history` 无界 Vec | 🟡 Medium | LRU bounded |

### Q3: Synthia 比两个标杆强的地方

**不要回退**：
- **Hook 系统**：比 Codex 更早建立（`hook-modify-tool-input` 已落地），Codex 是后来追平
- **ProtectionZone / Loop Detection 4 层**：覆盖 DoomLoop、Circuit、NoProgress、GenericRepeat，比 OpenCode 单 doom-loop 更完整
- **Telemetry Drop-timer 风格**：已经接近 Codex `Timer::drop` 模式
- **Compaction 3 层 (L1/L2/L3)**：比 OpenCode `PRUNE_PROTECT=40K` 单档更细

### Q4: 三个专家对"先做什么"的不同意见

| 专家 | 立场 | 风险 |
|---|---|---|
| A 派（性能派） | 先修 #1+#2+#3 | 修了但 prefix 还是不稳，cache 收益有限 |
| B 派（生产派） | 先修 #1+#2+#4 | 缺 compaction 配合，长会话仍会 OOM |
| C 派（架构派） | 先做 #5+#6 收敛 | 不修 P1/P6 致命问题，治标不治本 |

**用户选择**：C 派（架构收敛）+ B 派（加可观测） = **战略 1：基础收敛**
- 主目标：#5 收敛 + #1 修 bug + #2 wire PrefixTracker
- 额外加：#6 Token 单一化（与 #5 同属"重复逻辑收敛"主题）
- 暂不动：#3 O(n²)、#4 落盘截断、#7 P8、#8 UTF-8、#9 Permission、#10 read_history

### Q5: 工作粒度

- **先写完整设计文档** (openspec 流程) — 用户选择
- 不立即实现
- 不写 plan.md（plan 由 writing-plans skill 在用户批准后产出）
- 不做 retrospective（实现完成才做）

---

## 设计 trade-offs

### T1: `Tool::is_concurrency_safe` 是 trait 方法还是字段？

| 方案 | 优 | 劣 |
|---|---|---|
| trait 方法 | 可运行时反映状态 | 每次调用有虚函数开销 |
| struct 字段 | 零开销，编译期可知 | ToolEntry 重复声明 |
| **采用** | **trait 方法 + 默认 false** | **向后兼容** |

### T2: Prompt 收敛是"删除"还是"统一 trait"？

| 方案 | 优 | 劣 |
|---|---|---|
| 删除 4 套，只留 `ContextAssembler` | 最简单 | 需逐一迁移调用点 |
| 统一 trait，5 套并存 | 渐进迁移 | 治标不治本 |
| **采用** | **删除 4 套，全部走 `ContextAssembler`** | **tasks.md 按顺序逐个迁移** |

### T3: PrefixTracker 是单独 crate 还是 context 子模块？

| 方案 | 优 | 劣 |
|---|---|---|
| 独立 crate | 边界清晰 | workspace 内过度拆分 |
| context 子模块 | 同 crate 内部，无循环 | 边界弱 |
| **采用** | **context 子模块，独立文件** | **未来提取为 crate 容易** |

### T4: TokenCounter trait 在 provider 还是 core？

| 方案 | 优 | 劣 |
|---|---|---|
| `synthia-provider::TokenCounter` | 提供方天然拥有 BPE 知识 | context 依赖 provider |
| `synthia-core::TokenCounter` | 中心化 | core 变胖 |
| **采用** | **synthia-provider::TokenCounter** | **同 workspace 无循环** |

---

## Open Questions

- OQ1: `ContextAssembler` 是否需要新加 public method 暴露"section by name"查询？
- OQ2: `prefix_stability_ratio` 窗口大小（rolling 多少 turn）？
- OQ3: `TokenCounter::count_messages` 是单条还是 batch？

→ 在 `proposal.md §8` 中给出倾向答案，spec 实现时若推翻需更新 design。

---

## 不在本次范围的项（明确拒绝）

| 项 | 拒绝理由 |
|---|---|
| `trim_to_budget` O(n²) 修 | critical 但属于"性能修复" change，下一迭代 |
| 4 套 truncate 收敛 | 属于"安全稳定" change |
| Permission 粒度升级 | 已在 `permission-merge` spec 中部分处理 |
| 3 套 compaction 合并 | 单独 change，需先确定统一接口语义 |
| Guardian / Rollout / Plugin | 已有 spec 覆盖，无需重做 |
| `pruning::hard_clear` 写 event log | 属 P8 改造，需大范围 telemetry 改造 |

---

## 验证策略

- 每个 spec 配 ≥ 6 个 unit tests（行为锁定）
- 每个 spec 配 ≥ 1 个 integration test（端到端）
- 保留所有现有 e2e tests 必须通过
- 4 个能力独立 commit，可单独 revert
- 公开 API 完全向后兼容
