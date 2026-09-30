# Deep Review: `EventStream`

**Location**: `crates/synthia-server/src/event_stream.rs:64`
**Signals**: 1 impl / 1 methods / 0 generics / 0 call sites / 0 dyn

## 目的
将 `broadcast::Receiver<AgentEvent>` 转换为 HTTP 响应(SSE 或 WebSocket)的策略抽象。1 个方法 `to_response(rx) -> impl IntoResponse`。

## 存在价值
- 1 impl: `SseEventStream`
- 0 dyn 引用: 直接构造 SseEventStream
- doc: "implementations handle the transport-level details"

## 替代方案
- **A) 直接用 `sse_event_stream()` 函数**: 当前 `SseEventStream::to_response` 仅是函数包装
- **B) 保留 trait**: 1 方法无法简化;trait 体现"SSE vs WebSocket 切换"的设计意图
- **C) 拆 trait**: 1 方法无法拆

## 推荐
**REMOVE_CANDIDATE** (移除 trait, 暴露 `SseEventStream::to_response` 公开函数)

## 理由
1 impl + 0 dyn 同样是 YAGNI 模式。但**此 trait 略好于 AuditWriter**:doc 明确说"SSE vs WebSocket",且 trait 注释了未来会加 `WebSocketEventStream`。当 WebSocket 支持进入开发时,这个 trait 就是合理的。但当前 0 dyn + 0 实际切换需求,trait 仍是"预留"。

## 4-party 检查

- **怀疑派**: 1 impl + 0 dyn,YAGNI。trait 在没有切换需求时是负担。**REMOVE_CANDIDATE**。
- **架构派**: trait 名/位置正确,但缺少"使用方"。**REMOVE_CANDIDATE**。
- **生产派**: 当前生产仅 SSE,移除不影响。**REMOVE_CANDIDATE**。
- **简化派**: 直接函数 `pub fn sse_event_stream(rx: ...) -> impl IntoResponse` 更简单。**REMOVE_CANDIDATE**。

**共识**: 4 派一致 (4-0) — **REMOVE_CANDIDATE**。

### 实现建议
```rust
// 替换为:
pub fn sse_event_stream(rx: broadcast::Receiver<AgentEvent>) -> impl IntoResponse + Send { ... }
// 当实现 WebSocketEventStream 时,提取 trait (SSE/WS 不同 impl,dyn 才需要)
```

### 风险
- WebSocket 支持开发中?若**是**,保留 trait 可能是合理前置设计
- 需要询问产品意图;若近期无 WS plan,移除
