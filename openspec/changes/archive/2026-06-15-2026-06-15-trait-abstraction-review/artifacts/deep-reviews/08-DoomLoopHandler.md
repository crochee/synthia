# Deep Review: `DoomLoopHandler`

**Location**: `crates/synthia-agent/src/doom_loop_handler.rs:71`
**Signals**: 1 impl / 1 methods / 0 generics / 0 call sites / 0 dyn

## 目的
Agent doom loop (无限循环调用同一工具) 检测的策略抽象,1 个方法 `handle_doom_loop(tool_name, input_json, iteration) -> DoomLoopResponse`。

## 存在价值
- 1 impl: `DefaultDoomLoopHandler`
- 0 dyn 引用
- 防止 LLM agent 进入死循环是 P0 安全特性,扩展策略(默认 abort vs retry-with-feedback)是合理设计点

## 替代方案
- **A) 直接用 `DefaultDoomLoopHandler`**: 失去策略可替换性
- **B) 保留 trait**: 1 方法已最小
- **C) 拆 trait**: 1 方法无法拆

## 推荐
**REMOVE_CANDIDATE** (移除 trait, 未来需要时再引入)

## 理由
1 impl + 0 dyn + 1 方法 = 纯预留抽象。但**doom loop 处理是 agent 核心安全特性**,未来可能需要:
- `LlmFeedbackDoomLoopHandler`(让 LLM 看到错误并重新规划)
- `UserConfirmDoomLoopHandler`(询问用户)
- `RateLimitDoomLoopHandler`(限制后续调用频率)

这些是合理的扩展点,trait 不是 YAGNI。**但当前 0 dyn 意味着没有真实的"切换"需求**,trait 仍是预留。

## 4-party 检查

- **怀疑派**: 0 dyn,纯预留。**REMOVE_CANDIDATE**。
- **架构派**: doom loop 是策略点,trait 合理。但缺少"使用方"。**KEEP (with 风险警告)**。
- **生产派**: 当前生产仅 1 handler,但未来多 handler 概率高。**KEEP**。
- **简化派**: 1 方法 + 0 dyn,可直接用具体类型。**REMOVE_CANDIDATE**。

**共识**: 2-2 分歧,需要 product 意图对齐。

### 决议
**REMOVE_CANDIDATE** (推迟到未来 change),理由:0 dyn 的 trait 永远没有"切换成本"收益,等到第二 impl 出现时再引入(30 秒工作量)。

### 实现建议
```rust
// 替换为:
pub struct DefaultDoomLoopHandler;
impl DefaultDoomLoopHandler {
    pub async fn handle_doom_loop(&self, ...) -> DoomLoopResponse { ... }
}
// 当第二策略出现时,提取 trait
```
