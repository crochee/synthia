## Why

上轮 `agent-bug-fix-and-dedup` 已修复 5 个 critical bug、删除 3 套重复代码、推迟 4 个 trait 抽象，但遗留 1 项未收口：**`LoopDetectorSet` 双实现未统一**。本阶段通过 4 专家对抗性审查 + opencode/codex 横向对照，确认 3 个新的工作项：

1. **补强 doom_loop 早退信号**（opencode 启发）：Synthia 现有 `DoomLoopDetector` 只检测不阻断，缺 opencode 风格的 `permission.ask` 早退机制
2. **统一 8 个检测器到 5 个**：guardian(孤儿,4) + agent(生产,4) 共 8 个，合并去重到 5 个（doom_loop+generic_repeat+ping_pong+poll_no_progress+global_circuit）
3. **统一 API 风格**：字符串 + severity vs 哈希 + 3-state，下游调用方难以泛化

**预期收益**：
- 消灭 1 套孤儿代码（`synthia-guardian::LoopDetectorSet`），符合 P10「文件即记忆」
- 新增 1 个关键能力（doom_loop 早退），对齐 opencode 生产级标准
- 检测算法 O(1) 统一（已在 agent 版实现），guardian 升到 O(1) 后性能对齐

**约束**：
- 不引入 `LoopDetector` trait（D3.1 推迟决策未变）
- 不实现 compact 兜底（opencode L2，独立 change）
- 不修改 permission 系统（`LoopAction::RequirePermission` 是信号，不直调 permission）

**当前优先级**：
- P1 补强 doom_loop 早退（关键能力） > P2 算法统一 > P3 API 收敛 > P4 删孤儿

## What Changes

### 1. DoomLoopDetector 早退信号 (P1.1)
- **From**: `DoomLoopDetector::check()` 返回 `LoopStatus::Detected` 供上层 block
- **To**: 检测到 3 次连续相同后返回 `LoopStatus::Detected` + 调用方获取 `LoopAction::RequirePermission`
- **Reason**: opencode 风格的早退信号——阻断 LLM 自动循环，要求用户决策
- **Impact**: Non-breaking（仅新增 `LoopAction` 枚举，调用方可忽略新字段保持原行为）

### 2. `GenericRepeatDetector` O(1) 统一 (P2.1)
- **From**: guardian 用 O(N) `Vec<u64>` 扫描；agent 用 O(1) `HashMap<(u64, u64), u32>`
- **To**: 两边都用 HashMap 方案（agent 版），阈值统一为 warn=2/block=3
- **Reason**: 算法性能对齐（guardian 升到 O(1)），阈值与 opencode 早退哲学一致
- **Impact**: Non-breaking（guardian 9 个内联测试需全部通过，但 API 不变）

### 3. 检测器集合 8 → 5 (P2.2)
- **From**: guardian(4) + agent(4) = 8 个检测器，语义重叠（PollNoProgress vs NoProgress）
- **To**: 5 个：`DoomLoopDetector` + `GenericRepeatDetector` + `PingPongDetector` + `PollNoProgressDetector` + `GlobalCircuitDetector`
- **Reason**: 消除 3 套语义重叠（NoProgress 被 PollNoProgress 替代）
- **Impact**: Non-breaking（被淘汰的 NoProgressDetector 是 agent 内部类型，无外部 caller）

### 4. `hash_tool_args` 公共函数 (P3.1)
- **From**: agent 版有 `hash_tool_args(tool_name, args_json) -> (u64, u64)`；guardian 没有
- **To**: 在 guardian 公开 `hash_tool_args`，agent 版删除
- **Reason**: 单一权威实现，避免双份
- **Impact**: Non-breaking（agent 版 `hash_tool_args` 也是内部，无外部 caller）

### 5. `LoopAction` 新枚举 (P1.1 配套)
- **From**: 只有 `LoopStatus { Ok, Warning, Detected }`
- **To**: 新增 `LoopAction { Continue, Warn, Block, RequirePermission, HardBlock }`
- **Reason**: 区分 doom_loop（用户决策） vs standard block（直接终止） vs critical（HardBlock）
- **Impact**: Non-breaking（`LoopStatus` 保持，新枚举是 `Option<LoopAction>` 附加字段）

### 6. 删 `synthia-agent::stream_builder::loop_detection` (P4.1)
- **From**: agent 内部 `LoopDetectorSet`（4-detector）
- **To**: 全部改用 `synthia_guardian::LoopDetectorSet`（5-detector）
- **Reason**: 唯一权威实现，agent 删本地副本
- **Impact**: Non-breaking（agent 内部 `LoopDetectionConfig` 是 1 release type alias 兼容）

### 7. 公共 API 调整 (P3.2)
- **From**: `check_tool_call(&mut self, tool_name, args) -> LoopDetectionResult`
- **To**: `check(&mut self, tool_name, args_json, iteration) -> (LoopStatus, Option<LoopAction>)`
- **Reason**: 与 agent 风格一致；`iteration` 参数让 `GlobalCircuitDetector` 不再需要外部 state
- **Impact**: **Breaking**（签名变化）。通过 type alias 1 release 兼容期缓解

## Capabilities

### New Capabilities

- **loop-detector-unified**: 定义唯一 `synthia_guardian::LoopDetectorSet`，包含 5 个检测器，hash-based API，3-state status + LoopAction 早退信号
- **doom-loop-early-exit**: 暴露 `LoopAction::RequirePermission` 信号，供 agent 调用 `permission.ask` 阻断 LLM 循环
- **loop-action-enum**: 定义 `LoopAction` 枚举，区分 5 种检测响应（Continue / Warn / Block / RequirePermission / HardBlock）

### Modified Capabilities

- **stream-builder-v2**: 删 `synthia-agent::stream_builder::loop_detection` 子模块，改用 `synthia_guardian::LoopDetectorSet`；`StepExecutor` 适配新 API（含 `RequirePermission` 处理路径）
- **guardian-loop-detector**（隐式）：`synthia_guardian::LoopDetectorSet` 从孤儿转为唯一公开

## Impact

### 受影响代码

- `crates/synthia-guardian/src/loop_detector.rs`（完整重写：5-detector + hash API + LoopAction）
- `crates/synthia-guardian/src/types.rs`（新增 `LoopAction` 枚举）
- `crates/synthia-guardian/src/lib.rs`（保持 `pub use loop_detector::LoopDetectorSet`）
- `crates/synthia-agent/src/stream_builder/loop_detection.rs`（删除整个文件）
- `crates/synthia-agent/src/dependencies.rs`（改 import 到 `synthia_guardian::LoopDetectorSet`）
- `crates/synthia-agent/src/stream_builder/builder.rs`（适配新 API + 处理 `RequirePermission`）
- `crates/synthia-e2e/src/scenarios/loop_detection.rs`（现有 5 个测试保持，新增 1 个 doom_loop 早退测试）

### 受影响 API

- `LoopDetectorSet::check_tool_call` → `check`（**breaking**）
- 新增 `pub fn hash_tool_args(&str, &str) -> (u64, u64)`
- 新增 `pub enum LoopAction { Continue, Warn, Block, RequirePermission, HardBlock }`
- `LoopDetectionConfig` 从 `synthia-agent` 迁到 `synthia-guardian`（**breaking**）

### 受影响测试

- `synthia-guardian`: 9 → 25 个内联测试（新增 16 个）
- `synthia-agent`: 删除 9 个内联测试（迁移到 guardian）
- `synthia-e2e`: 5 → 6 个 scenario

### 依赖

- 无新外部 crate 依赖
- `ahash` 已存在（沿用 agent 版）
- `serde` 已存在（`LoopDetectionConfig` 序列化）

### 部署

- **Phase 1**: PR 形式合并算法核心 O(1)，独立部署
- **Phase 2**: PR 形式合并 API 收敛，**需 1 release type alias 兼容期**
- **Phase 3**: PR 形式合并检测器集合迁入 + 删孤儿，**breaking change**

### 风险

- P3.2 API breaking change 影响所有 3rd-party 直接调用 `LoopDetectorSet::check_tool_call` 的代码（grep crates.io 验证）
- P1.1 DoomLoop 早退改变用户感知行为（需 CHANGELOG 明确标注）
- 删孤儿（`synthia-agent::stream_builder::loop_detection`）可能影响 plugin 作者

## Out of Scope

- Token 驱动的 compact 兜底（opencode L2）→ 独立 change
- `LoopDetector` trait 抽象 → 仍按 D3.1 推迟
- permission 系统的 `doom_loop` 类别注册 → 独立 change
- UI 层 doom_loop 通知 → 独立 change
- 跨 session 共享 loop detection 状态 → 反 P10 原则，不做
