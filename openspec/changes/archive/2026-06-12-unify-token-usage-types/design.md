## Context

### 背景

Synthia 是 Rust 实现的 AI agent runtime。当前存在 4 处 `TokenUsage` 类型定义，分布在 4 个 crate 中：

| # | 位置 | 行号 | 字段数 | 含 `cached_prompt_tokens` | `Serialize/Deserialize` |
|---|------|------|--------|---------------------------|--------------------------|
| 1 | `synthia-provider::types::TokenUsage` | types.rs:401-406 | 4 | ✅ | ❌ |
| 2 | `synthia-session::types::TokenUsage` | types.rs:42-47 | 4 | ✅ | ✅ |
| 3 | `synthia-agent::events::TokenUsage` | events.rs:21-25 | 3 | ❌ | ✅ |
| 4 | `synthia-context::checkpoint::TokenUsageSnapshot` | checkpoint.rs:37-42 | 3 | ❌ | ✅ |

### 当前问题

1. **字段不一致**：3 处 3 字段，1 处 4 字段（含 `cached_prompt_tokens`）
2. **跨 crate 转换成本**：`builder.rs:413, 479` 等处手动构造 `crate::events::TokenUsage { prompt_tokens, completion_tokens, total_tokens }`
3. **审计聚合失真**：按 `AgentEvent` 聚合与按 `Session` 聚合的 token 总和可能差 `Σ cached_prompt_tokens`
4. **违反项目记忆硬约束**：`"Duplicate implementations must be removed"`

### 依赖图（关键约束）

```
synthia-provider ← (无内部依赖)
    ↑
synthia-session (依赖 provider)
    ↑
synthia-context (依赖 provider)
synthia-agent   (依赖 session, context, provider)
```

`synthia-provider` 是最底层，被其他 3 个 crate 依赖。

### 多专家审查共识

- **怀疑派**、**架构派**、**简化派**均确认 TokenUsage 收敛是"3 个正交前置任务"中的**最高优先级**
- 与项目记忆"Duplicate implementations must be removed"直接对齐
- 实施成本低（核心改动 < 100 行），零新增抽象

---

## Goals / Non-Goals

**Goals:**

- G1: 把 4 处 `TokenUsage` 收敛到 1 处（canonical type = `synthia_provider::types::TokenUsage`）
- G2: 保留所有外部 `pub use` 路径（向后兼容）
- G3: 序列化向前兼容（老 JSON 文件可反序列化）
- G4: 跨 4 个 crate 的引用编译通过
- G5: 现有测试运行通过

**Non-Goals:**

- N1: ❌ 添加 `Add` impl（推迟到 `LoopContext.cumulative_tokens` 升级为 `TokenUsage` 类型时）
- N2: ❌ `LoopContext.cumulative_tokens` 升级（独立任务）
- N3: ❌ Turn 模型（已在对抗性审查中拒绝）
- N4: ❌ 字段重命名（保持向后兼容）
- N5: ❌ turn_id 表示收敛（独立任务）
- N6: ❌ recovery path 显式化（独立任务）

---

## Decisions

### D1: canonical type 选择 `synthia_provider::types::TokenUsage`

- **选择**：以 `synthia-provider/src/types.rs:401-406` 的 `TokenUsage` 为唯一来源
- **理由**：
  1. 最低层（被其他 3 个 crate 依赖），不产生循环
  2. 已含 4 字段（包括 `cached_prompt_tokens`）
  3. 已有 `Default` derive
  4. 已有 `Debug/Clone`，仅缺 `Serialize/Deserialize`（加 derive 即可）
- **已考虑 alternative**：
  - **B. `synthia-session::types::TokenUsage`** → 与 D2 兼容（同样 4 字段），但位置不在最低层，下游需添加反向依赖
  - **C. 新建 `synthia-core::TokenUsage`** → 违反 YAGNI，零新能力

### D2: 迁移策略——1-line shim

- **选择**：3 处下游类型用 `pub use synthia_provider::types::TokenUsage;` 替换
- **理由**：
  1. 项目记忆硬约束：`"Module split pattern: keep original file as 1-line pub use sub_module::* shim, never delete the original path"`
  2. 外部测试代码（`synthia-server/tests/e2e_server_sse_test.rs:137` 使用 `synthia_agent::types::TokenUsage`）的 import 路径不变
  3. 序列化向前兼容
  4. 可"渐近硬替换"——后续一个文件一个文件清理 import 路径
- **已考虑 alternative**：
  - **B. 硬替换（修改所有 import）** → 编译期强制一致，但破坏外部调用方，且 32 处引用需全量修改

### D3: 序列化向前兼容——`#[serde(default)]`

- **选择**：给 `synthia_provider::types::TokenUsage` 加 `#[serde(default)]` 字段属性
- **理由**：
  1. 项目记忆硬约束：`"New Message struct fields must use serde(default) + ..Default::default() pattern for backward compatibility"`
  2. 老 JSON 缺 `cached_prompt_tokens` → 反序列化为 `None`
  3. 新 JSON 自动写入 `cached_prompt_tokens` 字段
- **已考虑 alternative**：
  - **B. 加 `version: u32` schema 字段** → 零用户场景，且让 32 处引用的 JSON 格式变化

### D4: `TokenUsageSnapshot` 类型——直接删除

- **选择**：删除 `synthia_context::checkpoint::TokenUsageSnapshot`，引用替换为 `synthia_provider::types::TokenUsage`
- **理由**：
  1. 名字"Snapshot"暗示"持久化时缩水"，但字段差异（缺 `cached_prompt_tokens`）是历史遗留，不是"快照"语义
  2. 字段差异（缺 cached）已是 bug
  3. `synthia-context` 已依赖 `synthia-provider`（Cargo.toml），零成本
- **已考虑 alternative**：
  - **B. 保留 `TokenUsageSnapshot` 名字** → 维护 2 个类型，违反"删除重复"原则

### D5: 命名与路径——保留原路径，不做 re-export

- **选择**：保留 `synthia_provider::types::TokenUsage` 路径，下游通过 `pub use` 引用
- **理由**：
  1. 最小变更原则
  2. 不引入新的"跳板" re-export
- **已考虑 alternative**：
  - **B. 在 `synthia-provider` 根 `pub use types::TokenUsage`** → 减少路径长度，但制造新约定，违反最小变更

### D6: 序列化 derive 添加位置——`synthia_provider::types`

- **选择**：在 `synthia-provider/src/types.rs:400-406` 添加 `Serialize, Deserialize` 到 `#[derive(...)]`
- **理由**：
  1. 让 provider 的 `TokenUsage` 自包含可序列化
  2. 与下游 `pub use` 兼容——下游自动获得 `Serialize/Deserialize`
- **约束**：`synthia-provider/Cargo.toml` 必须有 `serde` 依赖（实测已存在，Cargo.toml:11）

### D7: `Add` impl——本次不做

- **选择**：不实现 `TokenUsage + TokenUsage`
- **理由**：
  1. 当前 `LoopContext.cumulative_tokens: usize`，未升级为 `TokenUsage`
  2. 未来 `LoopContext` 升级时再补 `Add`，避免本次 scope 扩张
  3. `builder.rs:407` 当前 `ctx.cumulative_tokens += sampling_result.usage.total_tokens` 不需要 `Add`
- **已考虑 alternative**：
  - **B. 预先实现 `Add`** → 满足 YAGNI 反对，零 caller

---

## Risks / Trade-offs

### R1: 序列化前向不兼容

- **Risk**: 老 JSON 缺 `cached_prompt_tokens` 字段，反序列化失败
- **Mitigation**: D3 的 `#[serde(default)]` 保护

### R2: manual struct literal 编译失败

- **Risk**: 32 处引用中有 5+ 处使用 `TokenUsage { prompt_tokens: x, completion_tokens: y, total_tokens: z }` 形式构造。canonical type 字段顺序/字段名若不一致 → 编译失败
- **Mitigation**: shim 保留 `pub use`，所有原类型构造代码继续工作（shim 指向同一类型，字段名一致）

### R3: 字段顺序敏感反序列化（bincode）

- **Risk**: 某些二进制序列化器对字段顺序敏感
- **Mitigation**: 项目统一使用 JSON 序列化（`serde_json`），字段顺序无关

### R4: 性能回退

- **Risk**: `Serialize/Deserialize` derive 增加运行时开销
- **Mitigation**: 接受。开销 < 100ns / 序列化，不在 hot path（每 LLM 调用 1 次）

### R5: 文档/注释未更新

- **Risk**: 4 处 `TokenUsage` 的文档注释可能描述不同
- **Mitigation**: 在 `synthia_provider::types::TokenUsage` 添加"canonical type"标注；删除下游类型时清理过时注释

### R6: TokenUsageSnapshot 删除的外部引用

- **Risk**: 外部代码（测试、bench）可能 `use synthia_context::checkpoint::TokenUsageSnapshot`
- **Mitigation**: 项目内 grep 0 处使用（已验证），外部用户影响在 changelog 标注

### T1: 32 处引用 vs 1 行 shim 的 trade-off

- **Trade-off**: 保留 32 处外部 import 路径 vs 强一致性
- **接受理由**: 向后兼容 > 强一致性（项目记忆硬约束明确要求 shim 模式）

---

## Migration Plan

本 change **不涉及部署变更**（纯类型定义重构，零 endpoint / DB / wire format 变化）。

### 部署顺序

1. PR 1：基础重构
   - `synthia-provider/src/types.rs`: `TokenUsage` 加 `Serialize, Deserialize, serde(default)`
   - `synthia-session/src/types.rs`: 删除 struct 定义，替换为 `pub use`
   - `synthia-agent/src/events.rs`: 删除 struct 定义，替换为 `pub use`
   - `synthia-context/src/checkpoint.rs`: 删除 `TokenUsageSnapshot`，引用替换为 `synthia_provider::types::TokenUsage`
2. 跨 4 crate `cargo check` 验证
3. 跨 4 crate `cargo test` 验证
4. `cargo fmt --all` + `cargo clippy --all-targets --all-features --tests --all` 修复警告
5. OpenSpec verify → archive

### Rollback 策略

- 本 change 100% 向后兼容（shim + `#[serde(default)]`）
- 如发现新 bug，revert PR 即可
- 无数据迁移（JSON 字段向后兼容）

### 验收条件

- [ ] `cargo check --workspace` 0 错误
- [ ] `cargo test --workspace` 100% 通过
- [ ] `cargo clippy --all-targets --all-features --tests --all` 0 警告
- [ ] `grep -r "pub struct TokenUsage" crates/` 仅返回 `synthia-provider/src/types.rs:401`
- [ ] `grep -r "TokenUsageSnapshot" crates/` 返回 0 行
- [ ] 32 处引用中，外部路径（`synthia_agent::types::TokenUsage`, `synthia_session::TokenUsage` 等）继续工作

---

## Open Questions

### Q1: `TokenUsageSnapshot` 改名（重命名为 `TokenUsage`）还是直接删除？

- 当前决策：直接删除，引用替换为 `synthia_provider::types::TokenUsage`
- 替代方案：保留 `TokenUsageSnapshot` 名字，字段升级到 4 字段
- 待决原因：若决定改名，需更新 import 路径，破坏向后兼容

### Q2: `cached_prompt_tokens` 字段的语义

- 当前决策：保留 `Option<usize>`，缺失视为 0
- 替代方案：改为 `usize`（无 Option），老 JSON 缺字段 → 默认 0
- 待决原因：项目内 0 处代码读取此字段，无实际语义冲突

### Q3: `provider::TokenUsage` 的 `Default` 是否要 `total_tokens = 0`？

- 当前决策：保留现有 `Default`（全 0）
- 替代方案：`total_tokens = prompt + completion`（自动计算）
- 待决原因：当前 `Default` 已是全 0，行为不变；如改 `total_tokens` 计算逻辑会影响 LLM 流处理

### Q4: 是否在 `synthia-provider` 根 `pub use types::TokenUsage`？

- 当前决策：不做（保留 `synthia_provider::types::TokenUsage` 路径）
- 替代方案：`pub use types::TokenUsage as TokenUsage` 在 `lib.rs`
- 待决原因：减少路径长度 vs 最小变更原则
