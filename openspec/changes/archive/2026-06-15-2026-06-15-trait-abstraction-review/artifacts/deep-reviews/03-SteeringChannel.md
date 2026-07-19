# Deep Review: `SteeringChannel`

**Location**: `crates/synthia-agent/src/steering.rs:69`
**Signals**: 1 impl / 4 methods / 0 generics / 14 call sites / 14 dyn

## 目的
Agent 实时 steering 消息通道(用户中断、引导、优先级消息)。4 个方法:`send`(入队)、`try_recv`(非阻塞出队)、`is_empty`(检查)、`drain`(默认实现,批量出队)。

## 存在价值
**14 处 dyn 引用**,是 agent loop 中的关键基础设施。LLM agent 在工具执行期间可能收到用户中断信号,steering channel 是实现 interruptibility(P7 原则)的核心抽象。`Send + Sync` 约束使其可在多任务间共享。

## 替代方案
- **A) 直接用具体类型**: 失去通道实现可替换性(未来 mpsc 之外的 priority 队列等)
- **B) 保留 trait + 简化方法集**: `drain` 已有默认实现,`is_empty` 可由 `try_recv().is_none()` 派生 → 可删
- **C) 拆为多个小 trait**: `SteeringSender` / `SteeringReceiver` 拆分。channel 本质是双向,拆分反而增加样板

## 推荐
**KEEP** (可选小优化: 移除 `is_empty` 由调用方用 `try_recv` 判断)

## 理由
14 处 dyn 引用是最强证据,这是系统级核心抽象。`drain` 有默认实现已经体现 trait 设计能力,4 个方法都在用,无可削减。1 impl (`MpscSteeringChannel`) 是合理的默认实现。agent 实时 steering 是 LLM agent 差异化能力,移除会破坏 P7 (可中断性) 原则。

## 4-party 检查

- **怀疑派**: 14 dyn 引用 vs 1 impl,典型 plugin 模式,合理。KEEP。
- **架构派**: 符合 DIP。`Send + Sync` 是必要的并发边界。KEEP。
- **生产派**: agent loop 核心依赖,影响面大。KEEP。
- **简化派**: `is_empty` 可由 `try_recv` 派生(删一个方法);其他 3 个必要。KEEP (with minor refactor suggestion)。

**共识**: 4 派一致 (4-0) — **KEEP**。
