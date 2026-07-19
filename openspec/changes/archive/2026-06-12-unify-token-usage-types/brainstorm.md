<!--
Raw capture of multi-expert adversarial review (acting as brainstorming)
for the TokenUsage convergence change.

设计探索已于 2026-06-13 完成（4 派对抗性审查 + 苏格拉底式拆解）。
共识结论：拒绝完整 Turn 模型提案，但在"3 个正交前置任务"中将
TokenUsage 收敛列为最高优先级。

本档是 raw capture, 包含背景、问题链、决策依据与设计取捨。
design.md 将从本档萃取并重新整理。
-->

# Brainstorm: TokenUsage Convergence

## 背景（Context）

### 仓库现状

Synthia 是一个 Rust 实现的 AI agent runtime。当前存在 4 处
`TokenUsage` 类型定义，分布在 4 个 crate 中，导致：

1. **字段不一致**：3 处仅 3 字段（prompt/completion/total），1 处 4 字段（含 `cached_prompt_tokens`）
2. **类型转换成本**：跨 crate 边界需要手动 `TokenUsage { prompt, completion, total }` 构造
3. **审计聚合失真**：按 `AgentEvent` 聚合的 token 总和与按 `Session` 聚合的不一致
4. **违反项目记忆硬约束**："Duplicate implementations must be removed"

### 4 处 TokenUsage 定义（实测）

| # | 位置 | 行号 | 字段数 | 是否含 `cached_prompt_tokens` | 是否 `Serialize/Deserialize` | 是否 `Default` |
|---|------|------|--------|------------------------------|------------------------------|----------------|
| 1 | `synthia-provider::types::TokenUsage` | types.rs:401-406 | 4 | ✅ | ❌ | ✅ |
| 2 | `synthia-session::types::TokenUsage` | types.rs:42-47 | 4 | ✅ | ✅ | ✅ |
| 3 | `synthia-agent::events::TokenUsage` | events.rs:21-25 | 3 | ❌ | ✅ | ✅ |
| 4 | `synthia-context::checkpoint::TokenUsageSnapshot` | checkpoint.rs:37-42 | 3 | ❌ | ✅ | ✅ |

### 依赖图（确认 provider 是最底层）

```
synthia-provider ← (无内部依赖)
    ↑
synthia-session
    ↑
synthia-context (依赖 provider)
synthia-agent   (依赖 session, context, provider)
```

→ **canonical type 候选：`synthia-provider::types::TokenUsage`**
（最低层 + 已含完整字段 + 已有 `Default`）

---

## 问题链（Decision Chain）

### Q1: 是否值得做 TokenUsage 收敛？

**共识**（怀疑派、架构派、简化派一致）：**值得**。理由：
- 项目记忆硬约束"Duplicate implementations must be removed"未实现
- 当前 4 处类型已造成生产痛点（聚合失真、字段不一致）
- 实施成本低（核心改动 < 100 行）

### Q2: canonical type 选哪个？

候选：
- A. `synthia-provider::types::TokenUsage`（最低层 + 含 cached）
- B. `synthia-session::types::TokenUsage`（session 是 token 累加的核心）
- C. 新建 `synthia-core::TokenUsage`（新增抽象）

**决策**：**A. `synthia-provider::types::TokenUsage`**

理由：
1. 最低层（被其他 3 个 crate 依赖），不会产生循环
2. 已含 4 字段（`cached_prompt_tokens`）
3. 已有 `Default` derive
4. 已有完整 `Debug/Clone/Debug`，仅缺 `Serialize/Deserialize`（加 derive 即可）
5. session 内部已经依赖 provider（types.rs:8-9），session 的 TokenUsage
   收成 `pub use synthia_provider::types::TokenUsage` 是零依赖开销

排除 C 的理由：违反 YAGNI，零新能力，仅多一个跳板

### Q3: 迁移策略——shim 还是硬替换？

候选：
- A. **1-line shim**（`pub use synthia_provider::types::TokenUsage`）—— 100% 向后兼容
- B. 硬替换（修改所有 import）—— 编译期强制一致

**决策**：**A. 1-line shim**

理由：
1. 项目记忆："Module split pattern: keep original file as 1-line
   `pub use sub_module::*` shim, never delete the original path"
2. 外部测试代码（`synthia-server/tests/e2e_server_sse_test.rs:137` 使用
   `synthia_agent::types::TokenUsage`）的 import 路径不变
3. 序列化向前兼容：老 checkpoint JSON 文件仍可反序列化
4. 后续可以"渐近硬替换"——一个文件一个文件清理

### Q4: `TokenUsageSnapshot` 怎么办？

它是 `synthia-context::checkpoint` 的 3 字段快照，**没有 `cached_prompt_tokens`**
但有 `Serialize/Deserialize`。

**决策**：**删除 `TokenUsageSnapshot` 类型定义，全部替换为 `synthia_provider::types::TokenUsage`**

理由：
1. 名字带 "Snapshot" 暗示"持久化时缩水"，但实际上和 `TokenUsage` 字段
   不一致只是历史遗留，不是"快照"语义
2. 字段差异（缺 cached）已经是 bug
3. 删除后 `synthia-context::checkpoint` 必须依赖 `synthia-provider`
   （已有依赖，零成本）

### Q5: 序列化兼容性怎么办？

老的 checkpoint JSON 文件可能含 `TokenUsage` 的 3 字段序列化。
新 `TokenUsage` 多一个 `Option<usize>` 字段（`cached_prompt_tokens`）。

**决策**：**加 `#[serde(default)]` 保护**

理由：
- 项目记忆："New `Message` struct fields must use `serde(default)` +
  `..Default::default()` pattern for backward compatibility"
- 老 JSON 缺 `cached_prompt_tokens` 字段 → serde 反序列化为 `None`
- 新 JSON 自动写入 `cached_prompt_tokens` 字段
- 双向兼容

### Q6: 是否需要 `Add` 实现？

生产派提出：`builder.rs:407` 当前是 `ctx.cumulative_tokens += sampling_result.usage.total_tokens`，
未来若 `LoopContext.cumulative_tokens` 升级为 `TokenUsage`，需要 `TokenUsage + TokenUsage`。

**决策**：**本次提案**不实现 `Add`（避免 scope 扩张）。如果未来
`LoopContext.cumulative_tokens` 升级为 `TokenUsage` 类型，再补 `Add` impl。

### Q7: 影响范围

跨 crate 影响点（grep 32 处引用）：

**必须修改**（类型定义位置）：
- `crates/synthia-provider/src/types.rs:401-406`（加 `Serialize/Deserialize`，加 `#[serde(default)]`）
- `crates/synthia-session/src/types.rs:42-47`（替换为 shim）
- `crates/synthia-agent/src/events.rs:21-25`（替换为 shim）
- `crates/synthia-context/src/checkpoint.rs:37-42`（删除，使用 provider）

**可改可不改**（仅 import 路径影响）：
- `crates/synthia-agent/src/checkpoint.rs:18, 82, 95`（`use crate::types::TokenUsage` → 可用 `synthia_provider::types::TokenUsage`）
- `crates/synthia-agent/src/lib.rs:73`（`pub use events::TokenUsage` → 仍可工作）
- 测试文件（`synthia-server/tests/*`, `synthia-agent/tests/*` 等 8 处）—— shim 保留路径，**测试代码无需修改**

---

## 设计取捨（Design Trade-offs）

### 取捨 1: 命名

canonical type 名为 `synthia_provider::types::TokenUsage`。
- 优点：保留 crate 内部命名习惯
- 缺点：使用方需写 `synthia_provider::types::TokenUsage`（长路径）
- 替代：`pub use crate::TokenUsage` 在 `synthia_provider` 根 re-export
- **决策**：保留原路径，不做 re-export（最小变更原则）

### 取捨 2: 默认值语义

`TokenUsage::default()` 当前是全 0（prompt=0, completion=0, total=0,
cached=None）。
- 优点：累加起点合理
- 缺点：`total_tokens` 不一定等于 prompt+completion（某些 provider
  计数方式不同）—— 但这与原行为一致

**决策**：保留 `Default` 行为。

### 取捨 3: 序列化是否带版本字段？

某些系统会加 `version: u32` 字段用于 schema 升级。

**决策**：**不加**。原因：
- 没有用户场景需要 schema 版本
- 加版本字段会让 32 处引用的 JSON 格式变化
- 现有 `#[serde(default)]` 已经足够向前兼容

### 取捨 4: 性能影响

`Serialize/Deserialize` derive 会增加编译期开销 + 轻微运行时开销。

**决策**：接受。理由：
- TokenUsage 不是 hot path（每 LLM 调用 1 次序列化）
- 增加的开销可忽略（< 100ns / 序列化）

---

## 风险与缓解（Risks & Mitigations）

| 风险 | 严重性 | 缓解 |
|------|--------|------|
| 序列化前向不兼容（老 JSON 反序列化失败） | 高 | `#[serde(default)]` 保护 |
| 编译期类型不匹配（manual struct literal） | 中 | shim 保留 `pub use`，所有原类型构造代码继续工作 |
| 字段顺序敏感的反序列化（bincode） | 低 | 项目使用 JSON 序列化，serde JSON 字段顺序无关 |
| 性能回退 | 低 | Serialize/Deserialize derive 增量 < 100ns |
| 文档/注释未更新 | 中 | 在 `TokenUsage` 文档注释中标注 "canonical type" |
| 4 处类型定义变 1 处的回归 | 低 | 删除前用 `cargo check` 验证所有引用编译通过 |

---

## 范围边界（Scope Boundary）

### 包含（IN SCOPE）

- ✅ 收敛 4 处 `TokenUsage` 类型到 `synthia_provider::types::TokenUsage`
- ✅ 给 provider 的 `TokenUsage` 加 `Serialize/Deserialize` + `#[serde(default)]`
- ✅ 3 处下游类型改为 `pub use` shim（不删除类型路径）
- ✅ `TokenUsageSnapshot` 类型删除（与 provider 合并）
- ✅ 跨 4 个 crate 的引用编译验证
- ✅ 现有测试运行通过

### 不包含（OUT OF SCOPE）

- ❌ `Add` impl（推迟到 LoopContext 升级时）
- ❌ `LoopContext.cumulative_tokens` 升级为 `TokenUsage`（独立任务）
- ❌ Turn 模型（已在对抗性审查中拒绝）
- ❌ TokenUsage Snapshot 字段重命名（保持向后兼容）
- ❌ turn_id 表示收敛（独立任务）
- ❌ recovery path 显式化（独立任务）

---

## 决策链最终结论（Final Decision）

**做**。**TokenUsage 收敛**是 Turn 提案的 3 个正交前置任务中的
**最高优先级**（与项目记忆硬约束 "Duplicate implementations must
be removed" 直接对齐）。

canonical type：**`synthia_provider::types::TokenUsage`**。
迁移策略：1-line shim，保持向后兼容。
序列化：加 `#[serde(default)]` 保护。

---

## 后续任务衔接

完成本提案后：
1. 评估 `turn_id` 表示收敛（hook context 4 处 String/u64/usize 统一）
2. 评估 recovery path 显式化（builder.rs:355-363 的 `continue`）
3. 3 个月内收集"按 turn 维度查询"的真实 caller 需求
4. 如果有真实需求 → 走简化派 MVP（`TurnId(Uuid)` + `LoopContext.current_turn_id`）
5. 如果无需求 → 按"6 个月再评估"原则推迟 Turn 提案
</content>
</invoke>