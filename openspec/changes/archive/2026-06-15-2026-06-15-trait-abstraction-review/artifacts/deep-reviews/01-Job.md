# Deep Review: `Job`

**Location**: `crates/synthia-job/src/job.rs:8`
**Signals**: 1 impl / 3 methods / 0 generics / 9 call sites / 9 dyn

## 目的
定义可调度的异步任务抽象,使不同来源的定时任务 (cron, 一次性, 周期性) 可在统一调度器中执行。`description()`/`key()` 用于注册/调试,`execute()` 是核心钩子。

## 存在价值
**`Arc<dyn Job>` 在 4 个核心位置被引用** (避免重复):
- `crates/synthia-job/src/job.rs:15,21` — `ScheduledJob` 包装
- `crates/synthia-job/src/time_wheel/wheel.rs:86,100` — 时间轮调度
- `crates/synthia-job/src/time_wheel/entry.rs:14,23` — 时间轮条目
- `crates/synthia-server/src/scheduler/mod.rs:10,26,30` — 全局任务注册表

单 impl (`CronJobWrapper` in `synthia-agent/src/tools/cron_wrapper.rs:150`) 看似 YAGNI,但调度器是**按设计需要可扩展点**:未来会接入 `Task` 调度、消息总线任务等。trait 提供"无需修改调度器即可新增任务类型"的能力,这正是 plugin/strategy 模式的核心理由。

## 替代方案
- **A) 直接用具体类型** (无 trait): 调度器变成单态, 失去多任务来源能力
- **B) 保留 trait + 简化方法集**: 当前 3 个方法都已用上 (description/key/execute), 无可削减
- **C) 拆为多个小 trait**: `description`/`key` 可拆为 `JobMetadata`, `execute` 单独 trait; 但当前 3 个方法语义上属于同一抽象 (一个 job = 一段可描述/标识/执行的工作), 拆分无收益

## 推荐
**KEEP**

## 理由
虽然 impl_count=1,但 9 个 `Arc<dyn Job>` 调用站点全部依赖 `dyn` 形态 (即 trait 的多态能力),证明抽象在系统层面有价值,而非仅为一个具体类型服务。这是"silent 1-impl trait"的合理例子 — 设计预留扩展点,符合 plugin 模式。job 调度是 LLM agent 的核心能力,未来扩展可期 (Task, EventBus job 等)。若移除 trait,4 个核心文件的 `Arc<dyn Job>` 全部需要改具体类型,且失去新增任务类型的灵活性。

## 4-party 检查

- **怀疑派** (默认移除): impl=1 强烈暗示 YAGNI。若仅 cron 调度,直接用 `CronJob` 具体类型更简单。但 dyn 在 9 处使用,这不是"单 impl"而是"单 impl + 多态接口"模式,移除破坏未来扩展。
- **架构派** (依赖倒置): 符合 DIP — `synthia-job` 定义抽象,具体实现在 `synthia-agent` (上层)。依赖方向正确。`Send + Sync` 强制约束是合理的并发边界。
- **生产派** (影响面): 移除需改 4 个文件 (job.rs, time_wheel/{wheel,entry}.rs, scheduler/mod.rs),改动面大但都是机械替换 `Arc<dyn Job>` → `Arc<CronJobWrapper>`。不推荐 — 失去插件能力。
- **简化派** (更简单的抽象): 3 个方法都是必要最小集,无可简化。trait 是合适的抽象粒度,不会让调用者困惑。

**共识**: 4 派一致 (4-0) — **KEEP**。
