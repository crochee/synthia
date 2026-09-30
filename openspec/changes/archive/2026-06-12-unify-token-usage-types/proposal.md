## Why

Synthia 当前在 4 个 crate 中重复定义 `TokenUsage` 类型（`synthia-provider`, `synthia-session`, `synthia-agent`, `synthia-context`），导致字段不一致（3 字段 vs 4 字段，缺 `cached_prompt_tokens`）、跨 crate 转换成本高、审计聚合失真。这违反项目记忆硬约束 "Duplicate implementations must be removed"。4 派对抗性审查在 2026-06-13 一致认为此收敛是 Turn 提案的"3 个正交前置任务"中的最高优先级。

## What Changes

**TokenUsage 类型的 4 处定义收敛为 1 处**
- From: 4 处独立 struct（provider 4 字段、session 4 字段、agent 3 字段、context 3 字段）
- To: 1 处 canonical struct（`synthia_provider::types::TokenUsage`，4 字段）
- Reason: 消除字段不一致、降低维护成本、修复审计聚合失真
- Impact: 非破坏性（shim 模式 + `#[serde(default)]` 保护序列化前向兼容）

**3 处下游类型改为 `pub use` shim**
- From: `synthia_session::types::TokenUsage`（独立 4 字段 struct）
- To: `pub use synthia_provider::types::TokenUsage;`（1 行 shim）
- Reason: 项目记忆硬约束 "Module split pattern: keep original file as 1-line pub use shim, never delete the original path"
- Impact: 非破坏性（外部 `use synthia_session::TokenUsage` 仍可工作）

**`TokenUsageSnapshot` 类型删除**
- From: `synthia_context::checkpoint::TokenUsageSnapshot`（3 字段，独立 struct）
- To: 删除类型定义，引用替换为 `synthia_provider::types::TokenUsage`
- Reason: 名字"Snapshot"是历史遗留，字段差异（缺 cached）已是 bug
- Impact: 内部 API 破坏（项目内 0 处使用，已验证）；外部用户影响在 changelog 标注

**`synthia_provider::types::TokenUsage` 加 `Serialize/Deserialize` + `#[serde(default)]`**
- From: 仅 `Clone, Debug, Default`
- To: `Clone, Debug, Default, Serialize, Deserialize` + 字段级 `#[serde(default)]`
- Reason: 让 canonical type 自包含可序列化；保护老 JSON 文件反序列化兼容性
- Impact: 序列化格式新增 `cached_prompt_tokens` 字段；老 JSON 缺字段 → 默认为 `None`

## Capabilities

### New Capabilities

- `unified-token-usage`: 把 4 处 `TokenUsage` 类型定义收敛为 1 处 canonical type（`synthia_provider::types::TokenUsage`），通过 1-line shim 模式保持向后兼容，加 `Serialize/Deserialize` + `#[serde(default)]` 保护序列化前向兼容

### Modified Capabilities

（无。`token-counter-unification` spec 关于 `TokenCounter` trait，`token-budget-observability` 关于事件发射，与本 change 的数据类型层正交）

## Impact

**影响的代码位置（4 个 crate，~10 个文件）：**
- `crates/synthia-provider/src/types.rs:401-406`（加 derive）
- `crates/synthia-session/src/types.rs:42-47`（替换为 shim）
- `crates/synthia-agent/src/events.rs:21-25`（替换为 shim）
- `crates/synthia-context/src/checkpoint.rs:37-42`（删除类型）
- 引用替换：`crates/synthia-context/src/checkpoint.rs:30-58`（`TokenUsageSnapshot` 引用点）

**测试影响：**
- 32 处 `TokenUsage` 引用中，外部 import 路径（`synthia_agent::types::TokenUsage`, `synthia_session::TokenUsage`）通过 shim 继续工作，**测试代码无需修改**
- `crates/synthia-server/tests/*` 5 处、`crates/synthia-agent/tests/*` 1 处、`crates/synthia-session/tests/*` 3 处

**依赖影响：**
- `synthia-context` 已有 `synthia-provider` 依赖（已验证 Cargo.toml），零新增依赖
- `synthia-session` 已有 `synthia-provider` 依赖（已验证 Cargo.toml），零新增依赖
- `synthia-agent` 已有 `synthia-provider` 依赖（已验证 Cargo.toml），零新增依赖

**API 影响：**
- 内部 API：`synthia_context::checkpoint::TokenUsageSnapshot` 删除（项目内 0 使用）
- 公共 API：4 处 `TokenUsage` 路径全部保留（shim 模式）
- 序列化：新增 `cached_prompt_tokens` 字段（Option，老 JSON 兼容）

**性能影响：**
- 序列化/反序列化开销 < 100ns / 调用（不在 hot path）
- 编译时间：增加 `Serialize/Deserialize` derive，增量可忽略

**风险等级：低**（shim 模式 + 序列化向前兼容）
</content>
</invoke>