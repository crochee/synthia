# Spec: interceptor-actual-impl

## ADDED Requirements

### Requirement: PermissionInterceptor 实际实现 — SHALL enforce permission checks on tool calls

`PermissionInterceptor` SHALL 在 `BeforeTool` 事件中执行权限检查。
- **R1.2**: `intercept()` 在 `BeforeTool` 事件中调用 `security_check()`
- **R1.3**: Block 级别返回 `InterceptorError::ShortCircuited { name: "permission" }`
- **R1.4**: RequireConfirm 级别调用 `ApprovalService::request_approval()` 并等待用户响应
- **R1.5**: 用户拒绝时返回 `InterceptorError::ShortCircuited`
- **R1.6**: AutoApprove 级别直接调用 `next.run()`

#### Scenario: permission block short-circuits tool call
- **WHEN** PermissionInterceptor intercepts a BeforeTool event with a Block-level security result
- **THEN** it returns `InterceptorError::ShortCircuited { name: "permission" }` and the tool call is aborted

### Requirement: LoopDetectInterceptor 实际实现 — SHALL detect and short-circuit repetition cycles

`LoopDetectInterceptor` SHALL 检测重复循环并短路拦截。
- **R2.2**: `intercept()` 在 `AfterTool` 和 `AfterLlm` 事件中调用 `LoopDetectorSet::check()`
- **R2.3**: 检测到循环时返回 `InterceptorError::ShortCircuited { name: "loop_detect" }`
- **R2.4**: 将现有的 `LoopDetectorSet` 实例迁移为 Interceptor 注册

#### Scenario: loop detection short-circuits on cycle
- **WHEN** LoopDetectInterceptor detects a repetition cycle in AfterTool or AfterLlm events
- **THEN** it returns `InterceptorError::ShortCircuited { name: "loop_detect" }`

### Requirement: ApprovalInterceptor 实际实现 — SHALL request user approval for confirm-required tools

`ApprovalInterceptor` SHALL 对 RequireConfirm 级别的工具请求用户审批。
- **R3.2**: `intercept()` 在 `BeforeTool` 事件中，对 RequireConfirm 级别的工具调用 `request_approval()`
- **R3.3**: 与 `PermissionInterceptor` 协调：PermissionInterceptor 检查权限级别，ApprovalInterceptor 执行审批流程

#### Scenario: approval requested for confirm-required tool
- **WHEN** ApprovalInterceptor intercepts a BeforeTool event for a RequireConfirm-level tool
- **THEN** it calls `request_approval()` and awaits the user's response before proceeding

### Requirement: RetryInterceptor 实际实现 — SHALL retry failed tool calls with exponential backoff

`RetryInterceptor` SHALL 在工具执行失败时以指数退避重试。
- **R4.2**: `intercept()` 在 `AfterTool` 事件中，如果工具执行失败且未超过 max_retries，则等待指数退避后重试
- **R4.3**: 重试次数记录在 `InterceptorContext::data` 中
- **R4.4**: 超过 max_retries 后调用 `next.run()` 传递错误

#### Scenario: retry with exponential backoff
- **WHEN** a tool execution fails and retry count is below max_retries
- **THEN** RetryInterceptor waits with exponential backoff and retries, up to max_retries before propagating the error

### Requirement: CompactInterceptor 实际实现 — SHALL compact context on token threshold

`CompactInterceptor` SHALL 在 token 使用量超过阈值时触发上下文压缩。
- **R5.2**: `intercept()` 在 `IterationEnd` 事件中，检查当前 token 使用量是否超过阈值
- **R5.3**: 超过阈值时触发上下文压缩
- **R5.4**: 压缩结果写入 `InterceptorContext::data` 供主循环读取

#### Scenario: context compaction on token threshold
- **WHEN** token usage exceeds the threshold at IterationEnd
- **THEN** CompactInterceptor triggers context compaction and stores the result in `InterceptorContext::data`

### Requirement: InterceptorChain 初始组装 — SHALL assemble default interceptor chain with fixed PermissionInterceptor

`InterceptorChain` SHALL 组装包含默认拦截器的链，PermissionInterceptor 固定在位置 0。
- **R6.2**: 用户可通过 `InterceptorChain::add()` 在 PermissionInterceptor 之后插入自定义拦截器
- **R6.3**: PermissionInterceptor 始终位于位置 0，不可移除或重排

#### Scenario: PermissionInterceptor at fixed position zero
- **WHEN** InterceptorChain is created via `default_with_guard()` or custom interceptors are added
- **THEN** PermissionInterceptor remains at position 0 and cannot be removed or reordered
