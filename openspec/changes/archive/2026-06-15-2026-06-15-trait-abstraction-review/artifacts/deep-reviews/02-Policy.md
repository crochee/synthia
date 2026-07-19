# Deep Review: `Policy`

**Location**: `crates/synthia-core/src/pbac/policy.rs:346`
**Signals**: 1 impl / 4 methods / 0 generics / 3 call sites / 3 dyn

## 目的
PBAC (Policy-Based Access Control) 抽象:对 `AccessRequest` 进行评估,返回 `PolicyResult::Match/NoMatch/Indeterminate`。4 个方法:`name` (标识)、`matches` (核心评估)、`conditions` (附带约束)、`priority` (冲突解决时的优先级)。

## 存在价值
3 处 `Arc<dyn Policy>` 引用集中在 `synthia-core/src/pbac/policy.rs` 的 `PolicySet`(组合多 policy)、evaluator、测试代码中。1 impl (`DefaultPolicy` 等) 是当前实现,但**PBAC 的设计就是允许多个策略叠加**(allow + deny + 角色匹配等),后续添加 `RolePolicy`/`TimeBasedPolicy` 是预期内的。trait 是"开放-封闭"原则的体现。

## 替代方案
- **A) 直接用具体类型**: 失去多 policy 组合能力,违反 OCP
- **B) 保留 trait + 简化方法集**: 4 个方法都已用,`conditions` 可选 (返回 None) 但提供时是必要 API
- **C) 拆为多个小 trait**: `Policy` vs `PrioritizedPolicy`(可默认 priority=0)。但当前粒度合适

## 推荐
**KEEP**

## 理由
3 处 dyn 引用全部位于策略评估核心,1 impl 是合理的"基础实现 + 可扩展"模式。`AsyncPolicy` (子 trait) 表明设计已规划好异步路径,这是"为多 impl 预留接口"的标准做法。PBAC 模式 (类似 AWS IAM policy) 本身就需要多策略叠加,移除 trait 会让系统退化为硬编码单策略。**synthia-core** 核心抽象,KEEP。

## 4-party 检查

- **怀疑派**: impl=1 看似 YAGNI。但 PBAC 设计就是 multi-policy,1 impl 是起点。KEEP。
- **架构派**: 完美符合 DIP,`synthia-core` 定义抽象,evaluator 依赖抽象。`Send + Sync` 约束合理。KEEP。
- **生产派**: 移除会改变 PBAC 核心架构,影响面大。`AsyncPolicy` 子 trait 已有规划,需要保持父 trait 稳定。KEEP。
- **简化派**: 4 个方法不可简化。KEEP。

**共识**: 4 派一致 (4-0) — **KEEP**。
