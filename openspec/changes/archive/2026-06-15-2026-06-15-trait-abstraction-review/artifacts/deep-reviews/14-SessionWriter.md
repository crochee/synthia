# Deep Review: `SessionWriter`

**Location**: `crates/synthia-context/src/session_writer.rs:6`
**Signals**: 1 impl / 2 methods / 0 generics / 1 call sites / 1 dyn

## 目的
Context 系统中 session 写入抽象,2 个方法: `write_summary(summary)` 和 `log_compaction_event(event)`。`NoOpSessionWriter` 是默认 no-op 实现。

## 存在价值
- 1 impl: `NoOpSessionWriter`
- 1 dyn 引用
- trait + noop 是常见的"默认禁用扩展点"模式

## 替代方案
- **A) 直接用 `NoOpSessionWriter`**: 失去真实 writer 的可替换性
- **B) 保留 trait**: 2 方法不可简化
- **C) 拆 trait**: 可拆为 `SummaryWriter` + `CompactionEventLogger`。但当前 2 方法语义同源,拆分无收益

## 推荐
**REMOVE_CANDIDATE** (移除 trait, 暴露 NoOpSessionWriter 即可)

## 理由
**1 impl + 1 dyn + 2 方法**是小型但显式的"扩展点"模式。1 dyn 表明**当前生产**也用 `dyn SessionWriter` (作为 generic bound),这是真实价值。但 NoOp impl 暗示"默认禁用" — 当真实 writer (`FileSessionWriter` / `DbSessionWriter`) 出现时,trait 价值才完整。当前 0 个真实 writer,trait 是预留。

## 4-party 检查

- **怀疑派**: 1 真实 impl (NoOp) + 1 dyn,本质是"开关",YAGNI。**REMOVE_CANDIDATE**。
- **架构派**: noop + trait 是合理 "feature flag" 模式。但当前所有调用方都是 noop。**KEEP (低优先级)**。
- **生产派**: 1 dyn 但全是 noop,生产价值 0。**REMOVE_CANDIDATE**。
- **简化派**: 2 方法 trait + 1 dyn noop,简化为 `Option<NoOpSessionWriter>` 或具体 `NoOpSessionWriter` 即可。**REMOVE_CANDIDATE**。

**共识**: 3 派 REMOVE,1 派 KEEP。最终:**REMOVE_CANDIDATE**。

### 实现建议
```rust
// 替换为:
pub struct NoOpSessionWriter;
impl NoOpSessionWriter {
    pub async fn write_summary(&self, _: &SummaryMessage) -> Result<(), ContextError> { Ok(()) }
    pub async fn log_compaction_event(&self, _: &CompactionEvent) -> Result<(), ContextError> { Ok(()) }
}
// 调用方直接用 NoOpSessionWriter,移除 Option<dyn> 包装
// 当真实 writer 出现时,提取 trait(2 impl 自然出现)
```

### 风险
- 1 dyn 引用方 (`Option<Box<dyn SessionWriter>>`?) 需要改具体类型
- 改后失去 "enable real writer" 能力,直到 trait 重引入
- 平衡: 当前生产 noop,价值接近 0,移除是合理的
