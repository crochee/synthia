## Why

Synthia 现有 5 个 critical bug（C1-C4, C6）和 3 套重复代码（LoopDetector 3 套、PermissionPolicy 4 套、Sandbox 命名误导），影响安全、正确性、性能。原 D1-D4 trait 抽象提案在 6 专家对抗性审查中被认为"过度设计"——共识是先修 bug/删重复，6 个月后再讨论 trait。

**预期收益**：消除 fail-open 默认（C2）、消除静默丢记录（C3）、消除 O(N) 热路径（C6）、减少 3 套重复代码。**约束**：不引入新 trait 抽象（违反 P6 不信任原则）。**当前优先级**：P1 bug 修复 > P2 重复删除 > P3 trait 推迟。

## What Changes

### 1. `cache_control_hash` 独立 hash (P1.1)
- From: `cache_control_hash = compute_hash(system_content)` （与 system_hash 相同，违反独立信号）
- To: 引入 `CacheControlMark { ttl, scope, pinned }`，独立 hash；`scope` 强制含 `user_id`
- Reason: C1 bug — `CacheBreakDetector` 无法检测 cache_control 变更
- Impact: Non-breaking. `CacheControlMark` 是新 struct，旧调用方默认构造即可

### 2. `MergedPolicy::evaluate` fail-closed (P1.2)
- From: 未知 pattern → `PermissionAction::Allow` （fail-open）
- To: 未知 pattern → `PermissionAction::Ask` （fail-closed）
- Reason: C2 bug — CVE 级别漏洞，未注册工具默认放行
- Impact: **Breaking**. 用户需为常用工具显式注册 `Allow` 规则

### 3. `LoopDetector` 改用 `Mutex` (P1.3)
- From: `Arc<RwLock<LoopDetector>>` + `try_write` （静默丢记录）
- To: `Arc<Mutex<LoopDetector>>` + `lock().expect()` （无丢记录）
- Reason: C3 bug — `try_write` 失败时 record 模式被吞，违反 P8（不丢信息）
- Impact: Non-breaking（API 不变）。语义变化：write lock 阻塞而非静默失败

### 4. `synthia-tool::exec::permission` 编译错误修复 (P1.4)
- From: 引用不存在的 `crate::types::PermissionLevel` （隐藏编译错误）
- To: 迁移到 `synthia_permission::Permission`
- Reason: C4 bug — 4 套 PermissionPolicy 中最隐蔽的一套
- Impact: Non-breaking. `synthia_permission::Permission` 是新权威类型

### 5. `GenericRepeatDetector` O(1) 算法 (P1.5)
- From: `VecDeque<(String, u64)>` + O(N) filter + JSON clone （3-5 ms/任务）
- To: `HashMap<(u64, u64), u32>` + O(1) 查询 + 零 String 分配
- Reason: C6 perf bug — 主循环热路径不必要开销
- Impact: Non-breaking. 行为变化：decay 模型 vs. window 模型（成功 -1 而非清零）

### 6. 删除 `synthia-agent::agent::loop_detector::LoopDetector` (P2.1)
- From: 3 套 `LoopDetector` 实现 （guardian、agent、stream_builder）
- To: 1 套（统一用 `synthia-guardian::LoopDetectorSet`）
- Reason: agent 版是 frozen snapshot，3-detector；guardian 版 4-detector 且活跃
- Impact: Non-breaking（API re-export）。删除 1 个文件 + 移动 30+ 测试

### 7. 删除 `synthia-permission::policy::PermissionPolicy` + `RuleSet` (P2.2)
- From: 4 套 PermissionPolicy（policy/merged/tool/fork）+ RuleSet 兼容垫片
- To: 1 套（`MergedPolicy`）
- Reason: `RuleSet` 已是"Backward Compatibility Adapter"技术债的证据
- Impact: Non-breaking（type alias）。18+ 测试迁移

### 8. 重命名 `synthia_exec::sandbox` → `command_blacklist` (P2.3)
- From: `synthia_exec::sandbox::Sandbox` （"sandbox" 是虚假宣传）
- To: `synthia_exec::command_blacklist::CommandBlacklist`
- Reason: 当前是 25-pattern 字符串黑名单，不是 OS sandbox
- Impact: Non-breaking（提供 1 release type alias 兼容期）

### 9. 推迟 4 个 trait 抽象（P3.1-P3.4）
- From: 计划 6 个月内抽 4 个 trait （LoopDetector/PermissionPolicy/OsSandbox/CacheBreakpoint）
- To: 推迟 ≥6 个月，带 re-evaluation 门槛条件
- Reason: 6 专家对抗性审查共识 — 在不完整理解现有实现语义边界前抽 trait 是"把不安全的现状封装成看起来安全的接口"
- Impact: 不引入新公共类型

## Capabilities

### New Capabilities

- `cache-control-mark`: 定义 `CacheControlMark` 结构体，独立 hash cache control 状态；强制 `CacheScope` 含 `user_id` 防止跨 session 串台
- `command-blacklist`: 替代原 sandbox 命名，明确为字符串黑名单（不是 OS sandbox）
- `loop-detector-algorithm`: 重新设计的 O(1) `GenericRepeatDetector` 算法，统一 3 套实现到 1 套 `LoopDetectorSet`
- `permission-fail-closed`: 统一 4 套 PermissionPolicy 到 1 套 `MergedPolicy`，fail-closed 默认

### Modified Capabilities

- `loop-detection`: 由 trait 化方案改为删除重复 + 算法优化 + `Mutex` 替换
- `permission-policy`: 由 sub-trait 拆分方案改为删除旧实现 + fail-closed 默认
- `sandbox-abstraction`: 由 `OsSandbox` trait 方案改为重命名 + 推迟 OS-level 抽象
- `prompt-cache-control`: 由 `Message::cache_control` 字段方案改为 `CacheBreakDetector` 内部独立 hash

## Impact

### 受影响代码

- `crates/synthia-context/src/prompt/cache.rs:233-237` (C1 fix)
- `crates/synthia-permission/src/merged_policy.rs:53-64` (C2 fix)
- `crates/synthia-agent/src/agent/step.rs:489-491` (C3 fix)
- `crates/synthia-agent/src/agent/core.rs:77` (C3 fix)
- `crates/synthia-tool/src/exec/permission.rs` (C4 fix, full file)
- `crates/synthia-agent/src/stream_builder/loop_detection.rs:53-57, 215` (C6 fix)
- `crates/synthia-agent/src/agent/loop_detector.rs` (P2.1 delete)
- `crates/synthia-permission/src/policy.rs:1-157` (P2.2 delete)
- `crates/synthia-exec/src/sandbox.rs` (P2.3 rename)

### 受影响 API

- `MergedPolicy::evaluate` 默认返回 `PermissionAction::Ask`（breaking）
- `LoopDetectorSet` 改为 `pub`，从 `synthia-guardian` re-export 到 `synthia-agent`
- `synthia_exec::sandbox` 模块改名 `command_blacklist`（提供 1 release type alias）

### 受影响测试

- `synthia-permission`: 18+ 测试迁移
- `synthia-agent`: 30+ 测试迁移到 `synthia-guardian`
- `synthia-context`: 新增 `CacheControlMark` 单元测试

### 依赖

- 无新外部 crate 依赖
- `ahash` crate 已存在（用于 `DefaultHasher` 替代）

### 部署

- **Phase 1**: PR 形式合并 5 个 bug fix，可独立部署
- **Phase 2**: PR 形式合并 3 个 dedup，需要 code review 关注向后兼容
- **Phase 3**: 不需要部署；仅为日历触发器

### 风险

- P1.2 是 breaking change（fail-open → fail-closed）
- P2.1/P2.2 删除可能影响 3rd-party 插件（需 grep crates.io 验证）
- P3 deferral 可能在 6 个月内错过真正的需求
