# Spec: permission-guard-single-path

## ADDED Requirements

### Requirement: 统一安全守卫路径 — SHALL enforce single mandatory permission check path

系统 SHALL 将三套并行安全机制统一为单一不可绕过的守卫路径。

- **R1.1**: 权限检查硬编码为工具调用执行链的第一步，不可通过配置或注册绕过
- **R1.2**: `PermissionInterceptor` 是 InterceptorChain 中第一个拦截器（位置 0）
- **R1.3**: `PermissionInterceptor` 在 `BeforeTool` 事件中执行 `PermissionChecker::security_check()`
- **R1.4**: 检查结果为 `Block` 时返回 `InterceptorError::ShortCircuited`，立即终止调用
- **R1.5**: 检查结果为 `RequireConfirm` 时调用 `ApprovalService` 等待用户确认
- **R1.6**: 检查结果为 `AutoApprove` 时继续执行链

#### Scenario: permission check is mandatory first step
- **WHEN** a tool call is initiated
- **THEN** PermissionInterceptor at position 0 runs `security_check()` as the first step and the result cannot be bypassed by configuration

### Requirement: 废弃 ToolProvider 安全钩子 — SHALL deprecate and migrate before/after_execute to interceptors

`ToolProvider::before_execute()` 和 `after_execute()` SHALL 标记为废弃并迁移到 InterceptorChain。
- **R2.2**: 6 个月过渡期后移除这两个方法
- **R2.3**: 在过渡期内，`before_execute`/`after_execute` 的调用包装为 Interceptor 自动注册

#### Scenario: deprecated hooks auto-registered as interceptors
- **WHEN** a ToolProvider still implements `before_execute`/`after_execute` during the transition period
- **THEN** these hooks are automatically wrapped and registered as interceptors in InterceptorChain

### Requirement: ToolExtensionRegistry 安全钩子保留 — SHALL reposition extension hooks as data transforms only

`ToolExtensionRegistry::fire_before`/`fire_after` SHALL 保留，但定位为"数据变换"而非"安全检查"。
- **R3.2**: 安全检查职责从 ToolExtensionRegistry 迁移到 PermissionInterceptor
- **R3.3**: ToolExtensionRegistry 的 handler 不再被允许返回 `Action::Skip` 用于安全拒绝（安全拒绝只能通过 PermissionInterceptor）

#### Scenario: extension handler cannot skip for security
- **WHEN** a ToolExtensionRegistry handler attempts to return `Action::Skip` for a security reason
- **THEN** the skip is rejected; only PermissionInterceptor may perform security rejection

### Requirement: 工具调用执行流顺序 — SHALL define execution chain with PermissionInterceptor first

工具调用执行流 SHALL 按 PermissionInterceptor 优先的顺序执行。

```
LLM 请求 → PermissionInterceptor(硬编码) → InterceptorChain(BeforeTool) → ToolExtension(before) → Execute → ToolExtension(after) → InterceptorChain(AfterTool) → 返回
```

#### Scenario: execution chain follows defined order
- **WHEN** a tool call is executed
- **THEN** the execution flows in order: PermissionInterceptor → InterceptorChain(BeforeTool) → ToolExtension(before) → Execute → ToolExtension(after) → InterceptorChain(AfterTool) → return
