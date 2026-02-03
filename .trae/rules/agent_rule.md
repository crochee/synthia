# Agent 设计的核心约束与原则

---

## 核心思想

> 真正的品味不是“会做”，而是“会收”。就像雕塑：人人都能挥刀凿石，但只有少数人知道约束点在哪。哪里该削去，哪里必须保留。**品味，生于懂得边界与克制。**

本文充当设计 Agent 时的**判断标准而不是教条**。当两个设计方案冲突时，用这些原则裁决。

---

## 第一部分：物理约束（不可违抗的现实）

这些是 LLM Agent 运行环境的硬性约束。不是我们选择的，是物理世界强加的。任何设计如果假装这些不存在，就会在生产中崩溃。

### C1. 上下文窗口有限且昂贵
- `128K token 窗口 ≈ 一本 300 页的书`
- 每次 API 调用都要为窗口中的每个 token 付费
- 未缓存 input: $3/M tokens → 50 轮对话可能花 $75
- **→ 上下文是最稀缺的资源，每个 token 都有机会成本**

### C2. KV Cache 前缀一旦改变，全部重算
- Anthropic Prompt Caching: 前缀匹配 → $0.30/M（缓存价）；前缀不匹配 → $3.00/M（全价）
- DualPath 论文证明：Agent 多轮推理中 KV-Cache 命中率 ≥ 95%，瓶颈不在算力，在“把缓存搬回来的 I/O”
- **prefix 一变 → 不只是多花钱，而是打满存储带宽**
- **→ 前缀的任何变动都有 10 倍成本放大效应**

### C3. 注意力在中间衰减（Lost in the Middle）
- LLM 对 prompt 开头和末尾的 token 注意力最强
- 中间部分的信息容易被“遗忘”，上下文越长，衰减越严重
- **→ 重要信息必须在开头（system prompt）或末尾（最近的 tool_result）**
- **→ 放在中间的信息，等于没放**

### C4. LLM 输出不可靠
- LLM 会循环（同一个工具调同样参数 20 次）
- LLM 会幻觉（编造不存在的文件路径）
- LLM 会忘记（30 轮后忘了最初的任务目标）
- LLM 会偏移（被最近的错误带偏，忽略全局计划）
- **→ 不能信任 LLM 会自己停下来、自己记住、自己纠偏**
- **→ 系统必须有独立于 LLM 的防护机制**

### C5. 工具执行不可预测
- bash 命令可能跑 0.1 秒，也可能跑 10 分钟
- 文件读取可能返回 100 字节，也可能返回 2MB
- 网络请求可能成功，也可能超时
- 用户可能随时想中断当前操作
- **→ 任何工具调用都必须可取消、可超时、可截断**

### C6. 信息有不同的半衰期
| 信息类型 | 半衰期 |
|---------|--------|
| 决策（“我们选 JWT 而不是 session”） | 整个 session |
| 错误教训（“这个方法行不通”） | 当前任务 |
| 文件内容（“auth.rs 第 42 行”） | 下次修改前 |
| 命令输出（“npm install 的日志”） | 几分钟 |
| 中间推理（“让我想想...”） | 当前轮次 |

- **→ 不同类型的信息应该有不同的保留策略**
- **→ 一刀切的“保留最近 N 条”会丢决策、留垃圾**

---

## 第二部分：设计原则（我们的选择）

每条原则标注它在应对哪些物理约束。

### P1. KV Cache 前缀一致性 ← [C1, C2]

**连续 API 调用之间，prompt 的前缀必须保持字节级一致。**

这是所有设计决策的最高优先级约束。违反它的代价是 10 倍成本 + 推理系统 I/O 瓶颈。

#### 具体要求
1. System prompt 构建完成后永远不变（不放时间戳、不动态增删工具 schema、不更新技能列表）
2. 信息只通过 tool_result 追加到末尾，不在中间插入
3. 技能指南附着在 tool_result 上，不改 system prompt
4. 序列化必须确定性（JSON key 排序、浮点格式一致、空格/换行规范化）
5. 压缩段一旦生成就不再修改（pruning 产出的摘要成为新的“不可变前缀”）
6. Skill snapshot 在 session 开始时锁定，session 中途不因 skill 变化而改变 system prompt

#### 违反信号（如果你在做以下事情，停下来）
- 往 system prompt 里塞动态信息（当前时间、工作进度…）
- 在对话历史中间插入新消息
- 修改已发送的 tool_result 内容
- 每轮重新生成 system prompt

---

### P2. Append-Only 上下文 ← [C1, C2, C3]

**上下文只能追加，不能插入、不能重排。所有信息回流上下文的方式只有一条：作为 tool_result 追加到末尾。**

这是 P1 的直接推论。

#### 允许的操作
- ✓ 在末尾追加 user / assistant / tool_result 消息
- ✓ 对旧消息做“原地缩减”（Soft Trim：缩短 tool_result）
- ✓ 对旧消息做“原地替换”（Hard Clear：替换为占位符）
- ✓ 整段替换为压缩摘要（Stage 3 pruning，但此后摘要不再变）

#### 禁止的操作
- ✗ 在中间插入新消息
- ✗ 重排消息顺序
- ✗ 修改已有消息的内容（Soft Trim/Hard Clear 除外）
- ✗ 删除消息后让后续消息“前移”

---

### P3. 按需加载一切（Lazy Loading） ← [C1, C3]

**不预装任何可以推迟加载的信息。System prompt 只放“索引”，完整内容等到需要时才通过 tool_result 注入。**

原因：上下文寸土寸金（C1），中间的信息会衰减（C3）。预装 10 个技能指南，agent 可能只用 2 个——另外 8 个占了 16K token 却零收益，还把有用信息推到了注意力衰减区。

#### 实现方式
- **技能加载**：System prompt 放一行摘要（~50 tokens/skill）；首次使用工具时 → 完整指南（~2K tokens）附着在 tool_result；后续使用（5-20 轮后）→ 关键提醒（~100 tokens）
- **记忆检索**：不预加载任何历史记忆；Agent 主动调用 memory_search → 结果作为 tool_result 返回
- **文件内容**：不预读项目文件；Agent 用 `read` 工具按需读取

#### 判断标准
如果一段信息放进 system prompt 后，有 >30% 的 session 不会用到它，就不该预装。

---

### P4. 渐进降级（Progressive Degradation） ← [C1, C6]

**资源紧张时，按信息价值从低到高逐步丢弃，而不是一刀切。**

不同信息的半衰期不同（C6），所以 pruning 必须区分对待。

#### 三阶段 Pruning
- **Stage 1 (30%)**: Soft Trim — 只缩减大 tool result
  - 损失：细节（但 head + tail 保留）
  - 收益：前缀完全不变 → 零 cache 失效
- **Stage 2 (50%)**: Hard Clear — 旧 tool result → 占位符
  - 损失：旧工具输出的全部内容
  - 收益：大量释放空间，前缀前段不变
- **Stage 3 (70%)**: 分级压缩 — 按事件类型差异化处理
  - Decision → Level 1（保留关键细节）
  - Error → Level 1（保留失败路径）
  - FileModified → Level 2（一行摘要）
  - FileRead → Level 3（删除，可检索召回）
  - Output → Level 3（删除）

#### 五层错误恢复（同理）
Truncate → Retry → Fallback → Auto-compact → Reset → Fail-fast

#### 核心直觉
能在 Stage 1 解决的问题，绝不升级到 Stage 2。每多一级，信息损失呈指数增长。

---

### P5. 末尾复述（Recency Anchoring） ← [C3, C4]

**关键工作状态必须被“复述”到上下文末尾，利用 LLM 对近期 token 注意力更强的特性。**

LLM 会忘记 30 轮前的任务目标（C4），中间信息会被淹没（C3）。解法不是把信息固定在开头（那会破坏前缀一致性），而是让 agent 主动把关键信息“搬到”末尾。

#### 机制：todo.md 复述模式
- Agent 读取 todo.md → 工作状态作为 tool_result 出现在末尾
- → LLM 在做决策时直接“看到”当前进度
- → 不需要额外的“工作记忆”抽象
- → 复用已有的文件读写工具

#### 何时复述
- 任务切换时 → 读 todo.md 重新对齐
- 长工具执行后 → 读 todo.md 恢复上下文
- 连续 10+ 轮工具调用后 → 主动复述

---

### P6. 系统级不信任（Distrust by Default） ← [C4]

**系统必须有独立于 LLM 的防护机制，不能依赖 LLM “自觉”停下来。**

LLM 陷入循环时不会自我诊断（C4）。所以防护层必须在 LLM 之外。

#### 四层循环检测（全部基于 hash 比对，不依赖 LLM 判断）
1. **Generic Repeat** — 同工具同参数重复 N 次
2. **Poll No Progress** — 轮询结果无变化
3. **Ping-Pong** — 两工具交替循环
4. **Circuit Breaker** — 全局连续 30 次无进展

#### 错误触发技能重载
连续 2 次同类错误 → 系统自动重新注入完整指南（不等 LLM 自己意识到需要重读文档）

#### 上下文安全阈值
- HARD_MIN = 16K tokens → 拒绝执行（不让 LLM 在极小窗口里挣扎）
- WARN_BELOW = 32K tokens → 发出警告

#### 核心原则
不靠 LLM 控制 LLM。防护机制必须是确定性的、基于规则的、在 LLM 推理循环之外的。

---

### P7. 可中断性（Interruptibility） ← [C5]

**Agent 的任何操作都必须可以被用户随时中断，并且中断后系统状态保持一致。**

工具执行不可预测（C5），用户需要实时控制权。

#### 三种中断粒度
1. **Steering（软中断）**：
   - 用户在工具执行期间发消息
   - → 当前工具完成后，跳过剩余工具
   - → 已完成的工作保留，未执行的标记 [Skipped]
   - → LLM 看到完整上下文（包括中断请求）

2. **Abort（硬中断）**：
   - AbortSignal 全链路传递
   - → 当前工具立即终止
   - → 部分结果丢弃

3. **Session Reset（最硬中断）**：
   - 不可恢复时 → 重建 session
   - → 旧 transcript 保留用于调试

#### 为什么 Steering 优于简单 Abort
- Abort 丢弃所有 in-flight 工作
- Steering 保留已完成的，只跳过未执行的
- → 信息损失最小化

---

### P8. 信息只转换，不消失（Transform, Never Lose） ← [C6]

**信息可以被压缩、摘要、降级，但不应该被默默删除。被 pruning 的内容必须仍然可检索。**

#### 事件日志（旁路存储）是“完整记忆”
- 每个 tool call、每个决策、每个错误，都 append 到 JSONL
- 上下文中被 pruning 掉的内容，在事件日志中依然完整

#### 检索路径
- Agent 调用 memory_search → 从事件日志中搜索
- → 结果作为 tool_result 回到上下文末尾
- → 被“遗忘”的信息可以被“想起来”

#### 实际意义
- Stage 3 pruning 把 FileRead 事件删除了
- → 但事件日志中完整保留
- → 如果 agent 后来需要这个信息 → memory_search 找回来
- → 无信息永久丢失

---

### P9. 可观测性优先（Observable First） ← [全部]

**无法测量的就无法优化。每个关键路径都必须有指标。**

#### 反例
- 上下文不可见 → 无法判断 prefix 是否稳定 → 无法优化 cache 命中
- Pruning 无记录 → 不知道信息损失了多少 → 无法调整阈值
- 循环检测无日志 → 不知道 LLM 浪费了多少 token → 无法改进 prompt

#### 必须追踪的指标
| 指标 | 说明 | 目标 |
|------|------|------|
| prefix_stability_ratio | 连续调用间 prefix 不变的比例 | >85% |
| cache_hit_ratio | Prompt Caching 命中率 | >85% |
| pruning_stage_distribution | 各阶段触发比例 | 80%+ 在 Stage 1 |
| loop_detection_blocks | 循环阻止次数 | 趋近 0 说明 prompt 质量好 |
| context_utilization | 上下文使用率分布 | 均匀使用 40-70% |

#### Context Trace（每次 API 调用记录）
```json
{
  "stage": "pruning_stage",
  "message_count": 45,
  "total_tokens": 89000,
  "prefix_hash": "a3f7c2...",
  "timestamp": "2024-01-15T10:30:00Z"
}
```
→ prefix_hash 变了 = cache miss → 立即定位原因

---

### P10. 文件系统即记忆（File System as Memory） ← [C1, C3]

**Agent 通过正常的文件读写管理工作状态，不需要专门的“记忆”抽象。**

#### 为什么不用专门的 memory 系统（Phase 0）
- 文件系统已经是持久化的、可搜索的、人可读的
- Agent 写 todo.md = 在“记笔记”
- Agent 读 todo.md = 在“回忆”
- Agent grep 项目 = 在“搜索记忆”
- 不需要向量数据库、不需要 embedding、不需要额外的 API 调用

#### 文件系统 vs 专用记忆的权衡
| 特性 | 文件系统 | 向量记忆 |
|------|----------|----------|
| 成本 | 零额外成本 | 增加复杂度 |
| 可审计性 | 人可审计 | 不透明 |
| 工具支持 | agent 已有工具 | 需要新 API |
| 搜索能力 | 弱（grep/ripgrep） | 强（语义搜索） |

#### 决策
- Phase 0 用文件系统
- Phase 3 可选加向量搜索

---

## 第三部分：原则间的优先级

**当原则冲突时，按此顺序裁决：**

```
P1 (前缀一致性) > P2 (Append-only) > P7 (可中断性) > P6 (不信任 LLM)
> P4 (渐进降级) > P3 (按需加载) > P5 (末尾复述) > P8 (不丢信息)
> P9 (可观测性) > P10 (文件即记忆)
```

### 典型冲突示例

#### 冲突 1：P3 (按需加载) vs P5 (末尾复述)
- **场景**：Agent 需要技能指南，但 tool_result 已经很长了
- **裁决**：P3 > P5。技能指南按需加载（附着在 tool_result 上），如果 tool_result 因此过长，P4（渐进降级）会处理——先 Soft Trim 旧的 tool_result，而不是跳过技能注入。

#### 冲突 2：P1 (前缀一致性) vs P4 (渐进降级)
- **场景**：上下文 70%，需要 Stage 3 pruning，但 cache 还在 TTL 内
- **裁决**：P1 > P4。延迟 pruning，等 cache TTL 过期。除非到了 95%（紧急阈值），此时 P4 升级为“生存级”优先。

#### 冲突 3：P7 (可中断性) vs P2 (Append-only)
- **场景**：用户 steering 中断，需要跳过剩余 tool calls
- **裁决**：不冲突。跳过的 tool calls 仍然追加 `"[Skipped]"` 占位符，保持 append-only 语义。P7 通过 P2 的机制实现，不违反它。

#### 冲突 4：P6 (不信任 LLM) vs 用户体验
- **场景**：循环检测触发 block，但 LLM 其实快要解决问题了
- **裁决**：P6 优先。宁可误杀也不能让 LLM 空转 50 次。但提供 warning 阶段（10 次）让 LLM 有机会自我纠正，只在 20 次时才 block。渐进，但不妥协。

---

## 第四部分：一句话速查表

| # | 原则 | 一句话 |
|---|------|--------|
| P1 | 前缀一致性 | 连续调用间 prefix 字节级不变 |
| P2 | Append-Only | 信息只追加到末尾，不插入不重排 |
| P3 | 按需加载 | 不预装任何可以推迟的信息 |
| P4 | 渐进降级 | 能在 Stage 1 解决的不升级到 Stage 2 |
| P5 | 末尾复述 | 关键状态通过文件读写推到上下文末尾 |
| P6 | 不信任 LLM | 防护机制独立于 LLM，基于规则和 hash |
| P7 | 可中断性 | 任何操作可随时中断，状态保持一致 |
| P8 | 不丢信息 | 信息可压缩降级，但旁路存储完整保留 |
| P9 | 可观测性 | 无法测量的无法优化 |
| P10 | 文件即记忆 | 用文件系统管理工作状态，不造抽象 |

---

## 附：这些原则的来源追溯

| 原则 | 主要来源 | 验证来源 |
|------|----------|----------|
| P1 前缀一致性 | Anthropic Prompt Caching 机制 | DualPath 论文（I/O 瓶颈量化证据） |
| P2 Append-Only | 我们 v2 架构的核心设计 | OpenClaw（prefix stability 设计） |
| P3 按需加载 | OpenClaw（skill lazy loading） | Manus（“不预装知识”原则） |
| P4 渐进降级 | OpenClaw（两阶段 pruning） + 我们（分级压缩） | Pi-Mono（结构化 compaction） |
| P5 末尾复述 | Manus（todo.md 模式） | Lost-in-the-Middle 论文 |
| P6 不信任 LLM | OpenClaw（4 种循环检测器） | Agent-Zero（intervention 机制） |
| P7 可中断性 | Pi-Mono（steering messages） | Agent-Zero（intervention） |
| P8 不丢信息 | 我们 v2 架构（旁路事件日志） | Agent-Zero（三层记忆区域） |
| P9 可观测性 | OpenClaw（cache trace 7 阶段） | 工程常识 |
| P10 文件即记忆 | Manus（agent 自管文件记忆） | Agent-Zero（knowledge/ 目录） |

---

## 关键洞察总结

1. **KV Cache 前缀一致性是最高约束** —— 违反它的代价不是 2 倍，是 10 倍（成本 + I/O 瓶颈）

2. **上下文是最稀缺资源** —— 不是算力，不是存储，是 attention 能覆盖的 token 数

3. **不要信任 LLM 会自我管理** —— 循环检测、错误恢复、任务对齐，必须是系统级、确定性的

4. **信息半衰期决定保留策略** —— 决策保留整个 session，命令输出只保留几分钟，一刀切会丢决策、留垃圾

5. **文件系统足够作为 Phase 0 的记忆系统** —— 不要为了“更智能”过早引入向量数据库，增加不透明性

6. **可观测性不是可选的** —— 没有 prefix_hash、pruning_stage、cache_hit_ratio，你就无法优化，只能猜

---