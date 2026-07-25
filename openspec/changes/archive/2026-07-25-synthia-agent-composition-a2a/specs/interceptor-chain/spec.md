# Spec: interceptor-chain

## ADDED Requirements

### Requirement: Interceptor trait
The system SHALL define an `Interceptor` trait:

```rust
trait Interceptor: Send + Sync {
    fn name(&self) -> &str;
    async fn intercept(&self, ctx: &mut InterceptorContext, event: InterceptorEvent, next: NextInterceptor) -> Result<()>;
}
```

#### Scenario: interceptor receives event and context
- **WHEN** an interceptor's `intercept` method is invoked
- **THEN** it receives a mutable context, the event type, and a `next` handler to delegate to

### Requirement: InterceptorEvent enum
`InterceptorEvent` SHALL have variants: `BeforeLlm`, `AfterLlm`, `BeforeTool{tool_name, input}`, `AfterTool{tool_name, output}`, `IterationEnd{iteration}`, `SessionEnd`.

#### Scenario: event variants cover agent lifecycle
- **WHEN** an agent loop runs through an LLM call, a tool invocation, and session end
- **THEN** the interceptor chain receives `BeforeLlm`, `AfterLlm`, `BeforeTool`, `AfterTool`, `IterationEnd`, and `SessionEnd` events in sequence

### Requirement: InterceptorChain struct
`InterceptorChain` SHALL hold a `Vec<Arc<dyn Interceptor>>` and execute `dispatch()` in order. Each interceptor MAY short-circuit (return `Err`) or delegate to `next` (call `next.intercept()`).

#### Scenario: chain dispatches in order
- **WHEN** `chain.dispatch(ctx, event)` is called with three interceptors
- **THEN** each interceptor is invoked in insertion order, and each may delegate to the next or short-circuit

### Requirement: concrete interceptors
The system SHALL provide the following concrete interceptors:
- `TraceInterceptor` — OpenTelemetry instrumentation
- `ApprovalInterceptor` — approval gating (wrapping `ApprovalService`)
- `RetryInterceptor` — retry logic (`max_retries` + backoff)
- `CompactInterceptor` — context compaction (`CompactionProvider` + threshold)
- `LoopDetectInterceptor` — loop detection (adapting existing `LoopDetector`)

#### Scenario: retry interceptor retries on failure
- **WHEN** a tool call fails and `RetryInterceptor` is configured with `max_retries = 2`
- **THEN** the interceptor retries the call up to 2 times with backoff before propagating the error

### Requirement: HookBuilder removal
`HookBuilder` deprecated methods SHALL be removed and migrated to the Interceptor Chain. `UnifiedHookDispatcher` SHALL be preserved as a compatibility layer (Interceptor wrapping Hook) until Phase 2 full replacement.

#### Scenario: hook builder deprecated methods removed
- **WHEN** the codebase is compiled after this change
- **THEN** `HookBuilder` deprecated methods no longer exist and hooks are wrapped as interceptors

### Requirement: EnhancedToolDispatcher removal
`EnhancedToolDispatcher` SHALL be removed. Its retry logic SHALL be migrated into `RetryInterceptor`.

#### Scenario: enhanced tool dispatcher removed
- **WHEN** the codebase is compiled after this change
- **THEN** `EnhancedToolDispatcher` no longer exists and retry behavior is provided by `RetryInterceptor`
