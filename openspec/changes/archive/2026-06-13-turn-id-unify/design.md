# turn-id-unify Design

## Context

### 背景

`turn-id-mvp` change（冻结至 2026-09-13）要求 3 个正交前置任务完成才能解冻实施：
1. `unify-token-usage-types`（✓ 已 archived 2026-06-12）
2. `turn-id-unify`（**本 change**）
3. `recovery-path-explicit`（未启动）

`turn-id-mvp/design.md` R4 明确列出本 change 的目标范围：

> ### R4: TurnId 与现有 5 个 turn_id 表示产生第 6 个
> - **Risk**: `LoopContext.iteration` (usize) / `AgentContext.turn_id` (String) / `PrefixStabilityEvent.turn_id` (u64) / `ApprovalRequest.turn_id` (String) / `TurnId(Uuid)` 共 5 个
> - **Mitigation**: turn_id 表示收敛前置任务必须先完成（解冻条件之一）

本 change 实施"turn_id 表示收敛"前置任务。

### 4 个 turn_id 表示的完整数据流（事实层）

| # | 表示 | 类型 | 构造 | 使用方 | 位置 |
|---|------|------|------|--------|------|
| 1 | `LoopContext.iteration` | `usize` | `+= 1` per LLM call | 内部循环控制 | `loop_context.rs:11` |
| 2 | `AgentContext.turn_id` | `String` | `format!("turn-{}", iteration)` | hook 回调 | `builder.rs:360` |
| 3 | `PrefixStabilityEvent.turn_id` | `u64` | `iteration as u64` | 遥测事件 | `builder.rs:503` |
| 4 | `ApprovalRequest::NetworkAccess.turn_id` | `String` | `"t"` 字面量（仅测试） | Guardian 审批 | `approval_request.rs:33` |

```
LoopContext.iteration: usize
    ├── (builder.rs:360) format!("turn-{}", iteration) ──→ AgentContext.turn_id: String
    ├── (builder.rs:376) iteration as u64 ──→ PrefixTracker.record_pre(turn_id: u64)
    ├── (builder.rs:500) iteration as u64 ──→ record_post(turn_id: u64)
    └── (builder.rs:503) iteration as u64 ──→ PrefixStabilityEvent.turn_id: u64
```

**关键观察**：
- #1 是 source of truth，#2-#4 都是派生视图
- #2/#3 同文件（`builder.rs`）相邻构造，但类型不同（`String` vs `u64`）
- #4 与 #2-#3 **无实际数据流耦合**（grep 0 处跨传递，Guardian 决策函数 0 处读 `turn_id`）
- #4 是 5 个 `ApprovalRequest` variant 中**唯一**有 `turn_id` 字段的（变体内不一致）

### 4 派对抗性审查共识

| 派 | 立场 |
|----|------|
| 怀疑派 | 4 个表示**不是**"重复实现"（identical code），是**不同视图**（different views）。"收敛"应聚焦"概念同源"而非"类型合并" |
| 架构派 | "收敛"= 让 4 个表示共享同一"turn_id 概念"，不= 让 4 个表示合并成 1 个类型。前者是抽象层，后者是类型层 |
| 生产派 | #4 是孤儿字段（5 variant 中 1 variant 有，0 生产 caller，Guardian 决策 0 处读）。删除 #4 是零风险 |
| 简化派 | 最小可行方案优先。任何引入"5.5 个表示"风险的方案都要拒绝（如 D：提前引入 `TurnId(Uuid)`） |

---

## Goals / Non-Goals

**Goals:**
- G1: 集中 `format!("turn-{}", iter)` 字符串构造到 `synthia_agent::turn_id::format_turn_id(iter: usize) -> String`
- G2: 删除 `ApprovalRequest::NetworkAccess.turn_id: String` 孤儿字段
- G3: 保留 `LoopContext.iteration: usize`（internal 计数器，零外部暴露）
- G4: 保留 `PrefixStabilityEvent.turn_id: u64`（内部 `PrefixTracker` 配套，不暴露 hook）
- G5: 零新类型，零新依赖
- G6: 与 `turn-id-mvp` 解冻时的升级路径一致（`format_turn_id(ctx.iteration)` → `ctx.current_turn_id`）

**Non-Goals:**
- N1: ❌ 引入 `TurnId(Uuid)` 类型（留给 `turn-id-mvp` 解冻时）
- N2: ❌ 替换 `LoopContext.iteration: usize` 为 `TurnId`（internal 计数器，零外部影响）
- N3: ❌ 替换 `PrefixStabilityEvent.turn_id: u64` 为 `TurnId`（与 `PrefixTracker` 内部配套，外部零影响）
- N4: ❌ `Add` impl / `Display` impl / `Serialize, Deserialize` derives（留给 `turn-id-mvp`）
- N5: ❌ Guardian `turn_id` 字段加到其他 4 个 `ApprovalRequest` variant（不属于本 change scope）
- N6: ❌ recovery path 显式化（独立前置任务）

---

## Decisions

### D1: 实施方案 = 集中格式化（B）+ 删除孤儿字段（C）

- **选择**：B + C 组合方案
- **理由**：
  1. B 解决"概念同源"——`#2` 字符串构造点集中
  2. C 解决"孤儿代码"——`#4` 字段无 caller、无数据流
  3. 两者独立，互不依赖
  4. 0 新类型，与 `turn-id-mvp` 0 协调
- **已考虑 alternative**：
  - **A. 仅文档化** → 0 收益，4 派一致同意需"做点什么"
  - **D. 提前 `TurnId(Uuid)`** → 与 `turn-id-mvp` 协调成本（5.5 个表示），4 派一致拒绝
  - **E. 类型别名 `type TurnId = u64`** → 0 实际收益（Rust type alias 不创建新类型），4 派拒绝

### D2: 集中函数命名 `synthia_agent::turn_id::format_turn_id`

- **选择**：函数路径 `synthia_agent::turn_id::format_turn_id(iter: usize) -> String`
- **理由**：
  1. 路径 `synthia_agent::turn_id::` 与未来 `synthia_agent::turn::TurnId`（来自 `turn-id-mvp`）解耦
  2. 函数名 `format_turn_id` 语义清晰（不是 `to_turn_id`、不是 `make_turn_id`）
  3. 签名 `(usize) -> String` 与当前 `format!("turn-{}", iter)` 行为完全一致
- **已考虑 alternative**：
  - **B. 放 `synthia-agent/src/lib.rs`** → 与 `loop_context` 概念耦合（`iteration` 概念在 `LoopContext`）
  - **C. 放 `synthia-hook` crate** → `synthia-hook` 不应是 helper 工具的位置

### D3: 新文件 `crates/synthia-agent/src/turn_id.rs`（不修改 `lib.rs`）

- **选择**：新建 `crates/synthia-agent/src/turn_id.rs`（~5 行）
- **理由**：
  1. 集中函数是"未来 `TurnId` 类型的占位模块"——`turn-id-mvp` 解冻时新增 `turn.rs`，本文件路径不冲突
  2. 5 行以下函数不值得放 `lib.rs`（项目记忆硬约束：split 大文件为聚焦模块）
- **已考虑 alternative**：
  - **B. 直接放 `lib.rs`** → 与项目记忆"Large files should be split"原则冲突

### D4: 删除 `ApprovalRequest::NetworkAccess.turn_id: String` 字段（破坏性 API 变更）

- **选择**：删除 `turn_id` 字段 + 简化 `network_access()` 构造函数
- **理由**：
  1. 5 个 `ApprovalRequest` variant 中**仅 1 个**有该字段（变体内不一致）
  2. grep 项目内 0 处生产 caller（仅 1 处测试用 `"t"` 字面量：`guardian_coordinator.rs:113`）
  3. Guardian 决策函数（`assess_risk`、`make_guardian_decision`）0 处读 `turn_id` 字段
  4. `NetworkAccess` 的 `id: String` 已足够唯一标识请求
- **风险**：破坏性 API 变更（外部用户调用 `network_access` 时少 1 参数，编译失败）
- **缓解**：
  1. 项目内 grep 0 处使用 6 参版本
  2. 项目记忆 `synthia-guardian` 是 `synthia-agent` 的下游，内部依赖可控
  3. 外部用户影响通过 changelog 标注
- **已考虑 alternative**：
  - **B. 保留字段但加 `#[allow(dead_code)]`** → 违反项目记忆硬约束 `"新产生的 Rust 代码，如果没有使用的情况请删除"`
  - **C. 改字段名为 `_turn_id: String`** → 同 B 违反

### D5: 保留 `LoopContext.iteration: usize`（不升级为 `TurnId`）

- **选择**：`iteration: usize` 字段保留，零变更
- **理由**：
  1. `iteration` 是"per-LLM-call 计数器"（语义：第 N 次 LLM 调用），与"turn_id"概念有别
  2. `loop_context.rs:99` 的 `should_reflect()` 用 `iteration.is_multiple_of(5)`，依赖 `usize` 类型
  3. `turn-id-mvp` 解冻时新增 `current_turn_id: Option<TurnId>` 字段，不替换 `iteration`
- **已考虑 alternative**：
  - **B. 升级为 `TurnId(Uuid)`** → 提前 3 个月引入新类型，4 派拒绝（YAGNI）

### D6: 保留 `PrefixStabilityEvent.turn_id: u64`（不升级为 `TurnId`）

- **选择**：`PrefixStabilityEvent.turn_id: u64` 字段保留，零变更
- **理由**：
  1. `PrefixStabilityEvent` 是 `PrefixTracker` 的内部遥测事件，不暴露 hook
  2. `u64` 类型对 `VecDeque<(u64, String)>` 滚动窗口的内存布局友好（对齐 8 字节）
  3. `turn-id-mvp` 解冻时 `u64` 升级为 `TurnId` 的工作量 < 5 行（`PrefixStabilityEvent` 重命名字段 + 调整 emit 函数）
- **已考虑 alternative**：
  - **B. 升级为 `TurnId`** → 提前 3 个月引入新类型，4 派拒绝

### D7: `format_turn_id` 函数签名 `(usize) -> String`，不提供其他重载

- **选择**：仅 1 个函数，无 `From` impl、无 `Display` impl
- **理由**：
  1. 当前唯一 caller（`builder.rs:360`）使用 `format!("turn-{}", iter)`
  2. `turn-id-mvp` 解冻时此函数被删除（`AgentContext::new` 直接接收 `ctx.current_turn_id`）
  3. 0 个 caller 就不应预定义 API
- **已考虑 alternative**：
  - **B. 加 `pub fn format_turn_id_str(s: &str) -> String`** → 0 caller，4 派拒绝
  - **C. 加 `From<usize> for String` impl** → 与标准库 `String::from` 冲突，命名空间混乱

---

## Risks / Trade-offs

### R1: `network_access` 构造函数破坏性变更导致外部编译失败

- **Risk**: 外部用户调用 `ApprovalRequest::network_access(id, turn_id, target, host, protocol, port)` 时，编译期参数数量错误
- **Mitigation**:
  1. 项目内 grep 0 处使用 6 参版本
  2. 外部用户通过 `synthia-guardian` re-export 影响范围有限
  3. Changelog 标注："ApprovalRequest::network_access() 构造函数从 6 参数简化为 5 参数（删除 unused turn_id 字段）"
- **接受理由**: 与项目记忆"删 dead code"硬约束一致；变体内不一致的字段是历史遗留，不是设计意图

### R2: 集中函数 `format_turn_id` 未来被 `turn-id-mvp` 删除

- **Risk**: `turn-id-mvp` 解冻后，`AgentContext.turn_id: String` 升级为 `Option<TurnId>`，`format_turn_id` 函数 0 caller
- **Mitigation**: `turn-id-mvp/tasks.md` 已规划"删除 `format_turn_id` 函数，替换为 `ctx.current_turn_id` 字段读取"
- **接受理由**: 本 change 是过渡状态，3 个月后函数被删除符合预期

### R3: `format_turn_id` 路径与未来 `turn.rs::TurnId` 命名混淆

- **Risk**: `synthia_agent::turn_id::format_turn_id` vs `synthia_agent::turn::TurnId` 概念混淆
- **Mitigation**:
  1. 路径分离（`turn_id` 模块 vs `turn` 模块）
  2. 函数名 `format_turn_id` 是 verb（动作），`TurnId` 是 noun（类型），语义清晰
  3. `turn-id-mvp` 解冻时如发现混淆，可重命名 `turn_id` 模块为 `legacy_turn_id` 或 `iteration_format`

### T1: 最小变更 vs 完全类型统一

- **Trade-off**: 接受 4 个表示（`usize` / `String` / `u64` / 孤儿删除）vs 引入 `TurnId(Uuid)` 统一
- **接受理由**:
  1. 4 派共识：当前 codebase 0 个"按 turn 维度查询"真实 caller
  2. 提前引入 `TurnId` 增加 5.5 个表示风险（与 `turn-id-mvp` 协调成本）
  3. YAGNI 原则：等到有真实 caller 需求时再统一

---

## Migration Plan

本 change **不涉及部署变更**（纯 helper 函数新增 + 字段删除，零 endpoint / DB / wire format 变化）。

### 部署顺序

1. **PR 1**：基础重构
   - 新增 `crates/synthia-agent/src/turn_id.rs`（~5 行 `format_turn_id` 函数）
   - `crates/synthia-agent/src/lib.rs` 添加 `pub mod turn_id;`
   - `crates/synthia-agent/src/stream_builder/builder.rs:360` 替换 `format!("turn-{}", ctx.iteration)` 为 `crate::turn_id::format_turn_id(ctx.iteration)`
   - `crates/synthia-guardian/src/approval_request.rs` 删除 `NetworkAccess.turn_id` 字段 + 简化 `network_access()` 构造函数
   - `crates/synthia-guardian/src/guardian_coordinator.rs:113` 更新测试调用
2. 跨 workspace `cargo check` 验证
3. 跨 workspace `cargo test` 验证
4. `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all` 修复警告
5. OpenSpec verify → archive

### Rollback 策略

- 本 change 100% 向后兼容（`format_turn_id` 函数纯新增，行为等价于 `format!`）
- 唯一破坏性：`network_access` 构造函数少 1 参数（项目内 0 caller）
- 如发现新 bug，revert PR 即可
- 无数据迁移（无 JSON 格式变化）

### 验收条件

- [ ] `crates/synthia-agent/src/turn_id.rs` 文件 < 30 行
- [ ] `synthia-agent::turn_id::format_turn_id(0) == "turn-0"`
- [ ] `synthia-agent::turn_id::format_turn_id(42) == "turn-42"`
- [ ] `builder.rs:360` 调用 `format_turn_id` 函数（非 `format!` 字面量）
- [ ] `grep "format!(\"turn-" crates/synthia-agent/` 仅返回 `crates/synthia-agent/src/turn_id.rs` 1 处
- [ ] `ApprovalRequest::NetworkAccess.turn_id` 字段 0 处
- [ ] `network_access()` 构造函数 5 参数（不是 6 参数）
- [ ] `cargo check --workspace` 0 错误
- [ ] `cargo test --workspace` 100% 通过
- [ ] `cargo clippy` 0 警告
- [ ] `crates/synthia-guardian` grep `"turn_id"` 仅返回 `synthia-hook` import 路径（如果 `synthia-guardian` 依赖 `synthia-hook`）或 0 处
