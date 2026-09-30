<!--
Raw capture of explore mode 决策链。
本档原样捕捉多专家对抗性分析与用户决策，不强制结构。
design.md 从本档萃取并重新整理为结构化设计文档。
-->

# Brainstorm: Borrow Best from Production Agents

## 背景

用户要求分析 synthia 与生产级 AI agent（opencode、codex、pi-mono）的差距，特别是 opencode。
要求：多专家对抗性运行、整体简洁、可拓展性强、more all as tool、真实性。

## 探索阶段输出（4 路并行 subagent）

### 路径 1：opencode 深度分析（8 维度）

**核心发现**：
- SystemContext typed source + reconcile/replace 状态机（★★★★★）
- SessionContextEpoch revision-based concurrent agent replacement
- Plugin Immer Draft 链式 hook 输出
- Permission "always" 自动传播到 pending
- TurnTransition defect 乐观重试模式
- Steer vs Queue 双投递模型
- Anchored Summary 8 段式模板 + 增量更新
- Durable vs Ephemeral 事件二分 + Projector/CommitGuard
- Cache Policy RESPECTS_INLINE_HINTS provider 白名单短路
- PTY Ticket-based scoped access

### 路径 2：codex 深度分析（7 维度）

**核心发现**：
- Hooks 系统（10 事件 + 信任状态机 + JSON Schema）
- ExecPolicy DSL（PrefixPattern + NetworkRule）
- Agent Role 机制（default/explorer/worker）
- Goal 扩展（budget 追踪 + 3 轮 blocked 阈值）
- LogDbLayer（tracing Layer + 批量写 SQLite）
- CompactionAnalyticsAttempt 全链路遥测
- SpanAttributesProcessor on_start 注入

### 路径 3：pi-mono 深度分析（6 维度）

**核心发现**：
- Extension lifecycle events + invalidate 防止 stale ctx
- Branch summarization for tree navigation
- File mutation queue（per-filepath 串行化）
- Context overflow 检测（21 正则 + silent overflow）
- Compaction split-turn + File operation tracking

### 路径 4：synthia 自审（10 维度）

**重大发现 — project_memory 与代码实测矛盾**：

| 项 | project_memory | 代码实测 | 结论 |
|---|---|---|---|
| H1 编排不可达 | 0/12 不可达 | Agent::resume 已调用，run_stream 否 | ⚠️ 部分修复，run_stream 静默降级风险 |
| H2 user_id ns | 0/11 未完成 | 四层贯通完整实现 | ✅ 已修复，project_memory 严重过时 |
| H3 文件工具 stub | 仍是 stub | 8 工具完整实现 | ✅ 已修复，project_memory 严重过时 |
| H4 LoopContext | 0/10 未恢复 | API 完整但主路径只恢复 2/4 字段 | ⚠️ 部分修复，iteration 未恢复 |
| landlock fallback | 0/21 未完成 | 代码已实现+测试 | ⚠️ openspec 任务文档滞后于代码 |

**synthia 当前实现完整度矩阵**：10 维度中 6 完整 / 4 部分 / 0 stub / 0 未实现。

**synthia 4 处领先**（保留深化）：
1. 5 层循环检测（poll no progress 是独家）
2. tool_result_cleared_at idempotent marker
3. derive_subagent_permission 只继承 Deny
4. 每 5 轮 self_reflect

## 决议链（5 个开放问题）

### Q1：project_memory 状态同步

**问题**：project_memory 中 H1-H4 数字与代码实测多处矛盾，是否需要先更新 project_memory？

**用户决策**：project_memory 是意图清单，与代码分开维护。

**含义**：
- 不更新 project_memory 的数字
- 但 proposal 中必须明确标注"代码实测状态"作为基线
- 后续任务以代码实测为准，而非 project_memory 数字

### Q2：H1/H4 修复优先级

**问题**：是否先修复 H1 静默降级 + H4 iteration 未恢复，再引入新特性？

**用户决策**：先修复后引入新特性。

**含义**：
- tasks.md 阶段 1 = 修复 H1/H4 静默风险
- 阶段 2+ = 引入新特性
- 修复未完成前不并行引入新特性

### Q3："More all as tool" 边界

**问题**：用户希望哪些能力 tool 化？

**用户决策**：系统提示词、权限策略、session 不作为 tool，其他都应该可以作为 tool。

**含义**：
- Guardian as Tool ✓（self_reflect tool）
- Compaction as Tool ✓
- SystemContext Source as Tool ✗（属于系统提示词）
- Permission as Tool ✗（属于权限策略）
- Session as Tool ✗
- Skill as Tool ✓（已有）
- 其他能力（如 diagnostics、telemetry query）可 tool 化

**原则**："能 tool 的尽量 tool，但系统提示词/权限/session 必须系统层。"

### Q4：SystemContext 工程量接受度

**问题**：opencode SystemContext typed source 需 ~800 行，是否接受？

**用户决策**：接受。

**含义**：
- SystemContext 作为长期目标纳入 proposal
- 但需分阶段实施（先引用相等短路等小改动，再 SystemContext）
- 在 Rust 中需自建 trait + Eq impl（无 Effect-TS 的 Schema/Equivalence 一等公民）

### Q5：OpenSpec 提案范围

**问题**：是否创建一个新 OpenSpec change 捕获本次分析结论？

**用户决策**：新的。

**含义**：
- 创建 `borrow-best-from-production-agents` change
- 综合借鉴 opencode/codex/pi-mono
- 不拆分为多个 change（用户明确说"新的"，单数）

## 设计取舍（多专家对抗性筛选）

### 剔除清单（不纳入 proposal）

- **codex Goal 扩展**：synthia 5 层循环检测已覆盖死循环防护，Goal budget 追踪与 synthia"文件即记忆"原则冲突
- **codex LogDbLayer**：与 synthia"Phase 0 不引入 SQLite"硬约束冲突，推迟到 P3
- **codex ExecPolicy DSL**：bash blacklist + sandbox 已覆盖 90%，DSL 过度设计
- **opencode PTY ticket**：synthia 当前非 PTY 优先场景
- **pi-mono Branch summarization**：synthia session tree 功能未完整，先补基础
- **pi-mono Output guard**：synthia 非 TUI 优先
- **codex Hooks 全量移植**：20 个 schema 文件复杂度过高，只取 PreToolUse/PostToolUse + trust 精简版

### 保留清单（纳入 proposal，按 ROI 排序）

#### 阶段 1：修复静默风险（前置必做）

1. **H1 run_stream 静默降级修复**（~30 行）
   - run_stream 内部自动调 assemble_default_tool_orchestrator
   - 或缺 orchestrator 时 panic（fail-fast）

2. **H4 LoopContext iteration 未恢复修复**（~10 行）
   - main_loop.rs:191-194 改用 LoopContext::from_metadata(metadata)
   - 验证 iteration 恢复后是否应立即触发 stop

#### 阶段 2：即时高 ROI 小改动（~100 行）

3. **Cache Policy 引用相等短路**（~50 行，from opencode）
   - Arc::ptr_eq 等价检查
   - tools/system/messages 三者都未变时直接返回原引用
   - 零分配、零 cache invalidation

4. **File mutation queue（per-filepath 串行化）**（~50 行，from pi-mono）
   - Mutex<HashMap<PathBuf, Arc<Mutex<()>>>> 实现
   - realpath 解析 key 处理 symlink
   - 完成后清理 Map 防内存泄漏

#### 阶段 3：中期补强（~500 行）

5. **Permission "always" 自动传播到 pending**（~100 行，from opencode）
   - 扫描同 session pending 请求
   - 新规则下 resources 全部 allow 时自动 resolve
   - "reject" 级联终止同 session 所有 pending

6. **Anchored Summary 8 段式模板 + 增量更新**（~200 行，from opencode）
   - Goal / Constraints / Progress(Done/InProgress/Blocked) / Key Decisions / Next Steps / Critical Context / Relevant Files
   - "Update the anchored summary" 而非重新生成
   - Token-budget aware split with mid-message slicing

7. **Context overflow 检测（21 正则 + silent overflow）**（~300 行，from pi-mono）
   - 21 个 provider-specific 正则 + 3 个排除
   - silent overflow 检测（usage.input + cacheRead > contextWindow）
   - 孤儿 tool call 合成空 result

#### 阶段 4：长期架构补强（~1000 行）

8. **TurnTransition defect 乐观重试**（~150 行，from opencode）
   - Rust 中用 Result<_, ControlFlow> 近似
   - 外层 catchDefect 等价 = match ControlFlow
   - 乐观执行 + 冲突重试

9. **CompactionAnalyticsAttempt 遥测**（~200 行，from codex）
   - 追踪 active_context_tokens_before / trigger / reason / implementation / phase
   - 补强 P9 pruning_stage_distribution 指标

10. **SpanAttributesProcessor on_start**（~400 行，from codex，已在 P1-5 roadmap）
    - per-span 属性在 on_start 注入
    - 剥离 Statsig 分支
    - mTLS + 多 exporter

11. **SystemContext typed source + reconcile/replace**（~800 行，from opencode）
    - 最大架构差距
    - Source trait + baseline/update/removed 函数
    - Snapshot 持久化
    - reconcile 用 Eq 比较，返回 Unchanged/Updated/ReplacementReady/ReplacementBlocked

#### 阶段 5：Tool 化改造（~300 行）

12. **Guardian as Tool**（~150 行）
    - 暴露 self_reflect tool
    - LLM 在需要时主动调用
    - 保留每 N 轮兜底机制（防 LLM 不调用）

13. **Compaction as Tool**（~150 行）
    - 暴露 compact_context tool
    - tool description 中提供 token 数 hints
    - 保留自动触发兜底（防 LLM 不调用）

## 对抗性裁决记录

### 裁决 1：SystemContext 不 tool 化

**冲突**：SystemContext source update 是否应该作为 tool 暴露？

**裁决**：不 tool 化。用户明确"系统提示词不作为 tool"。SystemContext 是系统提示词的子项管理，属于系统层。

### 裁决 2：Hooks 精简版 vs 全量

**冲突**：codex Hooks 系统是全量移植还是精简版？

**裁决**：精简版。只取 PreToolUse/PostToolUse + HookTrustStatus，不上 20 个 schema 文件。理由：synthia"简洁"原则 + 维护负担。

### 裁决 3：file mutation queue 死锁风险

**冲突**：Rust 中 Mutex<HashMap<PathBuf, Arc<Mutex<()>>>> 在 tokio runtime 下死锁风险？

**裁决**：接受风险，用 tokio::sync::Mutex 而非 std::sync::Mutex。per-filepath 粒度足够细，死锁概率低。

### 裁决 4：Guardian as Tool 兜底机制

**冲突**：LLM 不调用 self_reflect tool 怎么办？

**裁决**：保留每 N 轮兜底。LLM 自主调用为主，硬编码轮次为辅。这是"more all as tool"与"P6 不信任 LLM"的折衷。

### 裁决 5：H1 修复方案选择

**冲突**：H1 run_stream 修复用 Option A（自动调 assemble）还是 Option B（panic fail-fast）？

**裁决**：Option A（自动调 assemble_default_tool_orchestrator）。理由：CLI/Examples 调用方不应承担注入 orchestrator 的责任，自动装配更符合"简洁"原则。

## 未解决问题（留给 design.md）

1. SystemContext 在 Rust 中的 trait 设计（无 Effect-TS Schema/Equivalence）
2. TurnTransition defect 在 Rust 中的优雅表达（Result<_, ControlFlow> vs custom error type）
3. Anchored Summary 模板对不同 provider 的兼容性
4. file mutation queue 与现有 apply_patch 工具的集成点
5. Guardian as Tool 的兜底轮次（每 5 轮？每 10 轮？动态？）
