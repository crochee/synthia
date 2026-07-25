## ADDED Requirements

### Requirement: Interceptor trait
```rust
trait Interceptor: Send + Sync {
    fn name(&self) -> &str;
    async fn intercept(&self, ctx: &mut InterceptorContext, event: InterceptorEvent, next: NextInterceptor) -> Result<()>;
}
```

### Requirement: InterceptorEvent enum
InterceptorEvent 变体: BeforeLlm, AfterLlm, BeforeTool{tool_name, input}, AfterTool{tool_name, output}, IterationEnd{iteration}, SessionEnd

### Requirement: InterceptorChain struct
InterceptorChain 持有 Vec<Arc<dyn Interceptor>>，按序执行 dispatch()。
每个 interceptor 可短路（return Err）或委托给 next（call next.intercept()）。

### Requirement: concrete interceptors
- TraceInterceptor — OTel 埋点
- ApprovalInterceptor — 审批拦截（包装 ApprovalService）
- RetryInterceptor — 重试拦截（max_retries + backoff）
- CompactInterceptor — 压缩拦截（CompactionProvider + threshold）
- LoopDetectInterceptor — 循环检测（适配现有 LoopDetector）

### Requirement: HookBuilder removal
HookBuilder deprecated 方法删除，迁移到 Interceptor Chain。
UnifiedHookDispatcher 保留兼容层（Interceptor 可包装 Hook），Phase 2 再完全替换。

### Requirement: EnhancedToolDispatcher removal
EnhancedToolDispatcher 删除，重试逻辑迁入 RetryInterceptor。
