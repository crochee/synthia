## Context

### Background

上轮 `agent-bug-fix-and-dedup` 阶段（2026-06-11）已修复 5 个 critical bug、删除 3 套重复代码、推迟 4 个 trait 抽象。本阶段聚焦遗留项：**`LoopDetectorSet` 双实现统一**。

### 当前两套实现

| 维度 | `synthia-guardian::LoopDetectorSet` | `synthia-agent::stream_builder::loop_detection::LoopDetectorSet` |
|------|------------------------------------|------------------------------------------------------------------|
| **API 风格** | 字符串 `(tool_name, args)` | 哈希 `(tool_id, args_hash)` |
| **返回类型** | `LoopDetectionResult { detected, detector, count, severity }` | `LoopStatus { Ok, Warning, Detected }` |
| **检测器** | 4 个：GenericRepeat / PollNoProgress / PingPong / GlobalCircuit | 4 个：GenericRepeat / NoProgress / GlobalCircuit / DoomLoop |
| **GenericRepeat 算法** | O(N) `Vec<u64>` 扫描 | O(1) `HashMap<(u64, u64), u32>` |
| **GenericRepeat 阈值** | warn=10 / block=20（双档） | Detected=3（单档） |
| **早退信号** | ❌ 仅返回 result | ❌ 仅返回 status |
| **调用方** | **0 个**（孤儿） | `builder.rs:228,431`, `dependencies.rs:32,71` |
| **test 覆盖** | 9 个内联测试 | 9 个内联测试（+ 哈希工具 1 个） |

### 6 专家对抗性审查遗留

上轮 D1 (`LoopDetector` trait) 推迟的 3 个 re-evaluation 条件之一是 **"≥3 distinct loop detection strategies need to coexist"**——而两套实现共 6 个检测器已满足此条件。但 R1/R4 当时也指出：**「如果只是做 trait，封装的就是不安全的现状」**。本阶段重新评估，结论转变：

- 上一阶段的目标是"消除 bug + 删除重复"，抽象层暂缓
- 本阶段目标转为"补强能力 + 收口统一"，可以引入轻量 trait 抽象
- 关键转变：**opencode 的 doom_loop 早退机制**暴露了 Synthia 缺失的关键能力，迫使我们补强而非仅做抽象

### opencode/codex 对照分析

#### opencode 的 doom_loop（关键启发）

```typescript
// packages/opencode/src/session/processor.ts:24
const DOOM_LOOP_THRESHOLD = 3

// 取最近 3 个 parts，全部满足：
//   - type === "tool"
//   - tool === value.toolName
//   - state.status !== "pending"
//   - JSON.stringify(part.state.input) === JSON.stringify(value.input)
// → 触发 permission.ask({ permission: "doom_loop", ... })
// → return（不执行工具，请求用户决策）
```

**opencode 的两层循环架构：**
- L1: doom_loop（连续 3 次同工具同输入）→ permission.ask 早退
- L2: compact（token 阈值兜底）→ 上下文压缩
- doom_loop 是**权限类别**而非错误：触发用户决策

#### Synthia 现状对照

| 能力 | opencode | codex | Synthia 当前 | 差距 |
|------|---------|-------|-------------|------|
| 连续 3 次检测 | ✅ `DOOM_LOOP_THRESHOLD=3` | ❌ | ✅ `DoomLoopDetector` (agent 版) | **缺早退信号** |
| 早退 permission.ask | ✅ `permission.ask({ permission: "doom_loop" })` | ❌ | ❌ | **未实现** |
| 滑动窗口比对 | ✅ `slice(-3)` | ❌ | ✅ `recent_calls: VecDeque<..>` | 已对齐 |
| Hash 相同的累积 | ❌ | ❌ | ✅ `GenericRepeatDetector` (warn=10/block=20) | 已对齐 |
| 工具交替循环 | ❌ | ❌ | ✅ `PingPongDetector` (guardian 版) | 独有 |
| 全局迭代上限 | ❌ | ❌ | ✅ `GlobalCircuitDetector` | 独有 |
| 轮询无进展 | ❌ | ❌ | ✅ `PollNoProgressDetector` | 独有 |
| Token 兜底 compact | ✅ | ✅ | ❌ | **未实现**（独立 change） |

#### codex 启发

codex 没有显式 loop detection，完全依赖 token 驱动的 compact 兜底（待后续 change）。本阶段不涉及。

### 关键问题陈述

1. **能力补强**：Synthia 的 `DoomLoopDetector` 只检测不阻断——缺 opencode 风格的「早退信号」机制
2. **代码统一**：6 个检测器分布在两套实现（实际调用 4 + 孤儿 4），概念重复
3. **API 不一致**：guardian 用字符串 + severity，agent 用 hash + 3-state，下游调用方难以泛化
4. **孤儿代码**：`synthia-guardian::LoopDetectorSet` 自上一阶段 D2.1 后公开但 0 调用方，违反 P10「文件即记忆」原则

## Goals / Non-Goals

### Goals

1. **补强 doom_loop 早退信号**：检测到 doom_loop 时返回 `LoopAction::RequirePermission` 供调用方触发 permission.ask
2. **统一 8 个检测器到 4 个**：保留 opencode 启发+Synthia 独有，丢弃语义重叠
3. **统一 API 风格**：所有检测器采用 hash-based 输入 + `LoopStatus` 3-state 输出（agent 风格）
4. **迁入 synthia-guardian**：`synthia-guardian::LoopDetectorSet` 作为唯一公开类型，agent 删本地实现
5. **分阶段迁移**：先算法核心（O(1) HashMap），再 API 收敛，最后删除孤儿
6. **保留 severity 字段**：从 `LoopStatus` 派生 `LoopDetectionResult` 以兼容 guardian 现有调用

### Non-Goals

1. **不引入 LoopDetector trait**：4 个检测器全部用具体 struct，原因同上一阶段 D3.1 共识
2. **不实现 compact 兜底**：opencode/codex 的 L2 compact 是独立 change，不在本阶段
3. **不修改 permission 系统**：`LoopAction::RequirePermission` 是新枚举值，不影响现有 `PermissionAction` 流
4. **不公开 detector 子模块**：检测器保持 `pub(crate)`，仅 `LoopDetectorSet` 公开

## Decisions

### D1: 检测器组合（4 个保留）

| 保留 | 来源 | 阈值 | 触发动作 |
|------|------|------|---------|
| **DoomLoopDetector** | agent 版 | window=3 | `LoopAction::RequirePermission`（早退） |
| **GenericRepeatDetector** | agent 版（O(1) HashMap） | warn=2 / block=3 | `LoopStatus::Warning` / `Detected` |
| **PingPongDetector** | guardian 版 | pattern=4 (A-B-A-B) | `LoopStatus::Detected`（High severity） |
| **PollNoProgressDetector** | guardian 版（保留 result 哈希语义） | threshold=10 | `LoopStatus::Detected`（High severity） |
| **GlobalCircuitDetector** | 两版语义相同 | max_iterations=30 | `LoopStatus::Detected`（Critical severity） |

#### 淘汰

- **NoProgressDetector**（agent 版）：语义与 `PollNoProgressDetector` 重叠（都检测"连续 N 步无进展"）；result 哈希更直接有效，工具子集比对计算成本高但收益微弱
- **guardian 版 GenericRepeatDetector**（O(N) Vec）：被 agent 版 O(1) HashMap 完全替代
- **agent 版 DoomLoopDetector**：升级为早退信号版本（原版只有 check）

#### 阈值统一说明

- DoomLoop = 3（沿用 opencode + agent 现状）
- GenericRepeat warn=2/block=3：agent 现状（缩短累积次数以匹配 opencode 早退哲学）
- PollNoProgress = 10（guardian 现状）
- GlobalCircuit = 30（两版一致）
- **PingPong pattern=4**：A-B-A-B 4 个连续调用（guardian 现状）

### D2: API 收敛

#### 2.1 输入参数

```rust
// 统一为 hash-based（agent 风格）
pub fn check(&mut self, tool_name: &str, args_json: &str, iteration: usize) -> LoopStatus

// 内部用 hash_tool_args() 一次性 hash，避免每个 detector 重复计算
let (tool_id, args_hash) = hash_tool_args(tool_name, args_json);
```

#### 2.2 输出类型

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopStatus {
    Ok,
    Warning,    // 软信号：调用方决定是否提示
    Detected,   // 硬信号：调用方决定是否 block
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAction {
    Continue,           // 未触发，正常执行
    Warn,               // GenericRepeat 阈值前 1 步
    Block,              // 标准 block（severity=High）
    RequirePermission,  // DoomLoop 早退信号：调用方触发 permission.ask
    HardBlock,          // GlobalCircuit 触发（severity=Critical）
}
```

#### 2.3 兼容 adapter

```rust
// guardian 现存类型，保留 re-export 不破坏
pub use crate::types::LoopDetectionResult;

// 新增转换
impl From<LoopStatus> for LoopDetectionResult { ... }
impl From<(LoopStatus, LoopAction)> for LoopDetectionResult { ... }
```

### D3: 早退信号（D2 后的核心新增能力）

#### 3.1 DoomLoop 检测到时返回 `RequirePermission`

```rust
impl DoomLoopDetector {
    pub fn check(&mut self, tool_name: &str, args_json: &str) -> LoopStatus {
        // ... 原有 3 次连续检测逻辑
        if /* 3 consecutive identical (tool, args) */ {
            LoopStatus::Detected  // 标记硬信号
            // LoopDetectorSet::check 中转为 LoopAction::RequirePermission
        }
    }
}

impl LoopDetectorSet {
    pub fn check(&mut self, ...) -> (LoopStatus, Option<LoopAction>) {
        if doom_loop.detected { return (Detected, Some(RequirePermission)); }
        if global_circuit.detected { return (Detected, Some(HardBlock)); }
        if ping_pong.detected { return (Detected, Some(Block)); }
        // ...
    }
}
```

#### 3.2 调用方（agent/builder.rs）转换逻辑

```rust
match loop_detectors.check(...) {
    (LoopStatus::Ok, _) => continue,
    (LoopStatus::Warning, Some(LoopAction::Warn)) => log::warn!(...),
    (LoopStatus::Detected, Some(LoopAction::RequirePermission)) => {
        // 触发 permission.ask
        let decision = permission.ask(DoomLoop { tool: tu.name, args: tu.input }).await?;
        if decision.is_allow() { /* 执行 */ } else { break; }
    }
    (LoopStatus::Detected, Some(action)) => {
        // 其他 block 类型
        break;
    }
}
```

#### 3.3 与 opencode 的差异

- opencode 的 doom_loop 默认 permission 是 `"ask"`——我们同样要求用户决策
- opencode 的 doom_loop `always: [value.toolName]`——我们保持等价（permission 记录含 tool name）
- opencode 的 doom_loop 不区分 severity——我们保持 DoomLoop=RequirePermission（决策权），GlobalCircuit=HardBlock（直接终止）

### D4: 迁入策略（分 3 阶段）

#### Phase 1：算法核心统一（最小破坏）
- **目标**：在 `synthia-guardian::LoopDetectorSet` 内部用 O(1) HashMap 重写 GenericRepeat
- **修改**：`synthia-guardian/src/loop_detector.rs:46-100`（替换 `call_hashes: Vec<u64>` 为 `HashMap<(u64, u64), u32>`）
- **验证**：`cargo test -p synthia-guardian` 全绿
- **回退**：如果 guardian 的 9 个内联测试失败，回滚到原 Vec<u64>

#### Phase 2：API 收敛到 hash-based
- **目标**：guardian 的 `check_tool_call` 改为接受 `args: &str` 但内部用 `hash_tool_args`
- **新增**：`hash_tool_args()` 函数（从 agent 版复制）
- **修改**：guardian 的 `check_tool_call` 签名不变，外部 API 保持兼容
- **验证**：`cargo test -p synthia-guardian` + 静态分析确认公共 API 不变

#### Phase 3：检测器集合迁入 + 删孤儿
- **目标**：guardian `LoopDetectorSet` 4-detector 集合对齐（含 DoomLoop + PingPong + PollNoProgress + GenericRepeat + GlobalCircuit）
- **修改**：
  - guardian 删自己的 PingPong 字符串版本（agent 无 PingPong，guardian 版的迁入）
  - guardian 删自己的 PollNoProgress 字符串版本
  - 新增 DoomLoop（从 agent 版复制 + 早退信号）
  - 新增 LoopAction 枚举
- **修改**：`synthia-agent/src/stream_builder/loop_detection.rs` → 删文件，改用 `synthia_guardian::LoopDetectorSet`
- **修改**：`synthia-agent/src/dependencies.rs` 改 import 路径
- **修改**：`synthia-agent/src/stream_builder/builder.rs` 适配新 API（含 `RequirePermission` 处理）
- **删除孤儿**：`synthia-guardian::LoopDetectorSet` 公开但仍无 caller，确认所有调用方都迁入后保持公开（revert 公开）OR 重新内部化（视 Phase 3 后状态决定）
- **验证**：`cargo test --workspace` 全绿，benchmark 确认 O(1) 性能维持

### D5: 与 permission 系统的集成边界

`LoopAction::RequirePermission` 是**信号类型**，不直接调用 permission 系统。理由：

1. **职责单一**：loop detection 只负责"检测"，不负责"决策"
2. **解耦**：permission 系统在 `synthia-permission`，loop detection 在 `synthia-guardian`，跨 crate 直接调用违反分层
3. **可测试性**：`LoopDetectorSet` 可独立测试，不依赖 permission crate

调用方（agent/builder.rs）收到 `RequirePermission` 后**主动**调用 `synthia-permission::Permission::ask(...)`，这一逻辑与现有 `StepExecutor` 的 permission 调用一致。

### D6: 性能预算

| 操作 | 当前（agent 版） | 目标 | 测量方法 |
|------|------------------|------|---------|
| `check()` 单次调用 | ~50ns（HashMap O(1)） | ≤100ns | criterion benchmark |
| `hash_tool_args()` | ~20ns（2× DefaultHasher） | ≤50ns | criterion benchmark |
| 内存占用（稳态） | O(unique tools) | O(unique tools) | 内存 profile |
| DoomLoop 滑动窗口 | 3 entries | 3 entries | code review |

## Risks

### R1: DoomLoop 早退信号改变用户感知行为

- **现象**：原 `DoomLoopDetector` 仅返回 `Detected` 让上层 block；新版会触发 `permission.ask` 让用户决策
- **缓解**：phase 3 实施前先在 e2e 跑现有 doom_loop scenario，确保新行为与用户预期一致；CHANGELOG 明确标注「DoomLoop 现在会触发用户授权」
- **回退**：保留 `LoopAction::Block` 作为 fallback，调用方可配置

### R2: Guardian 检测器从 4 个增加到 5 个后内存增长

- **现象**：原 guardian 4-detector + agent 4-detector；合并后仍是 5-detector（doom_loop+generic_repeat+ping_pong+poll_no_progress+global_circuit），但只保留一份
- **缓解**：实际内存使用 = 之前的 1 套 + 新增 1 个 DoomLoop（仅 3 entries），影响 < 1KB
- **回退**：N/A（结构性合并，不引入额外开销）

### R3: Agent 删本地 LoopDetectorSet 后回归风险

- **现象**：`synthia-agent/src/stream_builder/loop_detection.rs` 整体删除
- **缓解**：phase 3 末要求 100% 测试迁移 + workspace 级别 `cargo test` 全绿
- **回退**：保留 type alias 1 个 release 周期

### R4: 6 专家推迟 trait 抽象的判断被推翻

- **现象**：本阶段恢复部分 trait 风格设计（`LoopAction` 枚举类似 trait 抽象）
- **缓解**：`LoopAction` 是**枚举**而非 trait，单态化，零运行时开销；R1 当时反对的是 trait vtable 开销，枚举不触发该问题
- **回退**：N/A

## Open Questions

无。本阶段已通过 4 项关键决策问答锁定方向：

1. ✅ 统一方向：agent → guardian
2. ✅ 检测器组合：4 检测器（合并 8 → 4）
3. ✅ 迁移方式：分阶段（算法 → API → 集合）
4. ✅ opencode/codex 启发：补强早退信号

## Verification Strategy

### 单元测试（迁移即测试）

- `synthia-guardian/src/loop_detector.rs` 现有 9 个测试全部保留
- 新增 `doom_loop_early_exit` 测试：3 次相同输入 → 验证返回 `RequirePermission`
- 新增 `pingpong` 测试迁移自 guardian 版 4 个测试
- 新增 `poll_no_progress` 测试迁移自 guardian 版 4 个测试
- 新增 `hash_tool_args` 测试迁移自 agent 版 3 个测试

### 集成测试

- `cargo test -p synthia-guardian` 全绿
- `cargo test -p synthia-agent` 全绿
- `cargo test --workspace` 全绿

### 端到端测试

- `synthia-e2e/src/scenarios/loop_detection.rs` 现有 5 个测试（test_loop_detection_soft_block 等）继续通过
- 新增 `doom_loop_triggers_permission_ask` e2e 测试

### Benchmark

- `criterion` benchmark 在 `synthia-guardian/benches/loop_detector_bench.rs`
- 对比：Vec<u64> 旧实现 vs HashMap 新实现，确认 O(1) 性能

### Code Review 检查清单

- [ ] 没有引入 `pub trait LoopDetector`
- [ ] 所有检测器保持 `pub(crate)`
- [ ] `LoopAction::RequirePermission` 文档说明调用方职责
- [ ] 现有 1161+ 单元测试全绿
- [ ] CHANGELOG.md 增加 "LoopDetectorSet unified" 章节

## Migration Plan

### Phase 1（PR #1）：算法核心 O(1)

文件：`crates/synthia-guardian/src/loop_detector.rs`

```diff
-pub(crate) struct GenericRepeatDetector {
-    call_hashes: Vec<u64>,
-}
+pub(crate) struct GenericRepeatDetector {
+    counts: HashMap<(u64, u64), u32>,
+    warn_threshold: u32,   // 2
+    block_threshold: u32,  // 3
+}
```

依赖：新增 `use std::collections::HashMap;`，复用 `ahash` 已存在的 `AHasher`。

测试：现有 9 个测试不需修改即通过（API 不变）。

### Phase 2（PR #2）：API 收敛

文件：`crates/synthia-guardian/src/loop_detector.rs`

新增：
```rust
pub fn hash_tool_args(tool_name: &str, args_json: &str) -> (u64, u64) { ... }
```

修改 `check_tool_call` 内部 hash 计算（外部 API 不变）。

### Phase 3（PR #3）：检测器集合迁入 + 删孤儿

文件：
- `crates/synthia-guardian/src/loop_detector.rs`：完整重写为 5-detector
- `crates/synthia-guardian/src/types.rs`：新增 `LoopAction` 枚举
- `crates/synthia-agent/src/stream_builder/loop_detection.rs`：删除整个文件
- `crates/synthia-agent/src/dependencies.rs`：改 import
- `crates/synthia-agent/src/stream_builder/builder.rs`：适配新 API + 处理 `RequirePermission`
- `crates/synthia-guardian/src/lib.rs`：保持 `pub use loop_detector::LoopDetectorSet`

测试：5-detector 集成测试 + e2e 测试。

## Out of Scope

以下事项明确**不**在本阶段范围：

1. **Token 驱动的 compact 兜底**（opencode/codex L2）→ 独立 change
2. **LoopDetector trait 抽象** → 仍按上一阶段 D3.1 推迟
3. **permission 系统的 doom_loop 类别注册** → 独立 change（需要与 `MergedPolicy` 集成）
4. **UI 层 doom_loop 通知** → 独立 change（前端路由）
5. **跨 session 共享 loop detection 状态** → 反 P10 原则，不做

## References

- 上轮设计：[agent-bug-fix-and-dedup/design.md](file:///home/crochee/workspace/synthia/openspec/changes/archive/2026-06-11-agent-bug-fix-and-dedup/design.md)
- opencode 源码：[processor.ts:24, 296-331](file:///home/crochee/workspace/opencode/packages/opencode/src/session/processor.ts#L24)
- opencode 文档：[permissions.mdx:143](file:///home/crochee/workspace/opencode/packages/web/src/content/docs/zh-cn/permissions.mdx#L143)
- Synthia 当前实现：
  - [synthia-guardian/loop_detector.rs](file:///home/crochee/workspace/synthia/crates/synthia-guardian/src/loop_detector.rs)
  - [synthia-agent/stream_builder/loop_detection.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/stream_builder/loop_detection.rs)
- 关联 spec：
  - [stream-builder-v2/spec.md](file:///home/crochee/workspace/synthia/openspec/specs/stream-builder-v2/spec.md) (agent 循环)
