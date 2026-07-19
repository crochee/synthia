# Deep Review: `SkillActivator`

**Location**: `crates/synthia-task/src/dispatcher.rs:29`
**Signals**: 1 impl / 1 methods / 0 generics / 2 call sites / 2 dyn

## 目的
任务执行前激活技能的抽象,1 个方法 `activate_skill(name) -> Result<(), SkillActivationError>`。

## 存在价值
- 1 impl: 默认实现 (file?)
- 2 dyn 引用 — **真实的运行时使用**
- 任务派发核心步骤

## 替代方案
- **A) 直接用具体类型**: 失去激活策略可替换性
- **B) 保留 trait**: 1 方法已最小
- **C) 拆 trait**: 1 方法无法拆

## 推荐
**KEEP** (活跃 trait, 有真实使用)

## 理由
**1 impl + 2 dyn** 与 0-dyn REMOVE 候选不同 — 此 trait 在运行时**确实被 dyn dispatch 使用**。2 个 dyn 引用表明:
- `Option<Arc<dyn SkillActivator>>` 或类似
- 调度器接受 dyn trait 作为依赖注入

这是健康的 plugin 模式,1 impl 是合理起点。**活跃使用的 1-impl trait** 是 KEEP 的标准例子。

## 4-party 检查

- **怀疑派**: 1 impl + 2 dyn = 活跃使用,KEEP 候选。**KEEP**。
- **架构派**: 任务派发的依赖注入点,DIP 完美体现。**KEEP**。
- **生产派**: 2 dyn 表明生产使用频繁。**KEEP**。
- **简化派**: 1 方法已最小,无可简化。**KEEP**。

**共识**: 4 派一致 (4-0) — **KEEP**。

### 备注
此 trait 与 `SkillProvider` (deep-review 12) 的拆分建议相关 — 如果 `SkillProvider` 拆为 3 trait,`SkillActivator` 可考虑合并入 `SkillWriter` trait (因为 activate 实际就是 skill 状态的修改)。但**当前形态合理**,独立 KEEP。
