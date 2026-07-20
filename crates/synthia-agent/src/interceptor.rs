//! Interceptor Chain — 统一横切关注点。
//!
//! 中间件模式，每个 interceptor 可短路或委托给 next。
//! 替代分散的 HookBuilder + ApprovalService + EnhancedToolDispatcher。

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use thiserror::Error;

/// Interceptor 错误。
#[derive(Debug, Error)]
pub enum InterceptorError {
    #[error("interceptor '{name}' failed: {reason}")]
    Failed { name: String, reason: String },
    #[error("interceptor chain short-circuited at '{name}'")]
    ShortCircuited { name: String },
}

/// 拦截事件类型。
#[derive(Debug, Clone)]
pub enum InterceptorEvent {
    /// LLM 调用前。
    BeforeLlm,
    /// LLM 调用后。
    AfterLlm,
    /// Tool 调用前。
    BeforeTool { tool_name: String },
    /// Tool 调用后。
    AfterTool { tool_name: String },
    /// 迭代结束。
    IterationEnd { iteration: usize },
    /// Session 结束。
    SessionEnd,
}

/// 拦截器上下文 — 在 interceptor 之间传递的可变状态。
#[derive(Debug, Clone)]
pub struct InterceptorContext {
    /// 当前 agent id。
    pub agent_id: String,
    /// 当前 session id。
    pub session_id: String,
    /// 当前迭代。
    pub iteration: usize,
    /// 自定义数据（interceptor 可读写）。
    pub data: serde_json::Value,
}

impl InterceptorContext {
    /// 创建新的 InterceptorContext。
    pub fn new(
        agent_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            session_id: session_id.into(),
            iteration: 0,
            data: serde_json::Value::Null,
        }
    }

    /// 确保 data 是 Object，返回可变引用。
    pub fn ensure_data_object(
        &mut self,
    ) -> &mut serde_json::Map<String, serde_json::Value> {
        if self.data.is_null() {
            self.data = serde_json::Value::Object(serde_json::Map::new());
        }
        self.data
            .as_object_mut()
            .expect("data is guaranteed to be Object")
    }
}

/// 下一个 interceptor 的委托函数。
pub struct NextInterceptor<'a> {
    remaining: &'a [Arc<dyn Interceptor>],
}

impl<'a> NextInterceptor<'a> {
    fn new(remaining: &'a [Arc<dyn Interceptor>]) -> Self {
        Self { remaining }
    }

    /// 调用下一个 interceptor。
    pub async fn run(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
    ) -> Result<(), InterceptorError> {
        if self.remaining.is_empty() {
            return Ok(());
        }
        let interceptor = &self.remaining[0];
        let next = NextInterceptor::new(&self.remaining[1..]);
        interceptor.intercept(ctx, event, next).await
    }
}

/// 拦截器 trait — 统一横切关注点。
#[async_trait]
pub trait Interceptor: Send + Sync {
    /// Interceptor 名称。
    fn name(&self) -> &str;

    /// 拦截方法。
    ///
    /// 每个 interceptor 可：
    /// - 检查/修改 ctx
    /// - 短路（return Err）
    /// - 委托给 next（call next.run()）
    async fn intercept(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
        next: NextInterceptor<'_>,
    ) -> Result<(), InterceptorError>;
}

/// 拦截器链 — 中间件模式。
pub struct InterceptorChain {
    interceptors: Vec<Arc<dyn Interceptor>>,
}

impl InterceptorChain {
    /// 创建空的 InterceptorChain。
    pub fn new() -> Self {
        Self {
            interceptors: Vec::new(),
        }
    }

    /// 添加 interceptor。
    pub fn add(&mut self, interceptor: Arc<dyn Interceptor>) {
        self.interceptors.push(interceptor);
    }

    /// 获取 interceptor 数量。
    pub fn len(&self) -> usize {
        self.interceptors.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.interceptors.is_empty()
    }

    /// 分发事件到所有 interceptor。
    ///
    /// 按序执行，每个 interceptor 可短路或委托给 next。
    pub async fn dispatch(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
    ) -> Result<(), InterceptorError> {
        if self.interceptors.is_empty() {
            return Ok(());
        }
        let first = &self.interceptors[0];
        let next = NextInterceptor::new(&self.interceptors[1..]);
        first.intercept(ctx, event, next).await
    }

    /// 创建包含默认守护拦截器的 InterceptorChain。
    ///
    /// PermissionInterceptor 始终在位置 0（如果提供），
    /// 其余拦截器按顺序添加：Trace → LoopDetect → Approval → Retry → Compact。
    pub fn default_with_guard(
        permission: Option<PermissionInterceptor>,
        loop_detector: Option<
            Arc<parking_lot::Mutex<synthia_guardian::LoopDetectorSet>>,
        >,
        approval: Option<ApprovalInterceptor>,
    ) -> Self {
        let mut chain = Self::new();

        // PermissionInterceptor 始终在位置 0
        if let Some(perm) = permission {
            chain.add(Arc::new(perm));
        }

        chain.add(Arc::new(TraceInterceptor));

        if let Some(detector) = loop_detector {
            chain.add(Arc::new(LoopDetectInterceptor::new(detector)));
        }

        if let Some(approval) = approval {
            chain.add(Arc::new(approval));
        }

        chain.add(Arc::new(RetryInterceptor::default()));
        chain.add(Arc::new(CompactInterceptor::default()));

        chain
    }
}

impl Default for InterceptorChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InterceptorChain {
    fn clone(&self) -> Self {
        Self {
            interceptors: self.interceptors.clone(),
        }
    }
}

// ─── 具体 Interceptor 实现 ───

/// Trace Interceptor — OTel 埋点。
pub struct TraceInterceptor;

#[async_trait]
impl Interceptor for TraceInterceptor {
    fn name(&self) -> &str {
        "trace"
    }

    async fn intercept(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
        next: NextInterceptor<'_>,
    ) -> Result<(), InterceptorError> {
        tracing::debug!(agent_id = %ctx.agent_id, event = ?event, "TraceInterceptor: before");
        let result = next.run(ctx, event).await;
        tracing::debug!(agent_id = %ctx.agent_id, event = ?event, "TraceInterceptor: after");
        result
    }
}

/// Permission Interceptor — 权限检查拦截。
///
/// 基于 closure 的权限检查，在 `BeforeTool` 事件时检查工具权限级别。
/// `Block` 级别直接短路；`RequireConfirm`/`RequireExplicit` 记录日志后通过；
/// `AutoApprove` 直接通过。
pub struct PermissionInterceptor {
    check_fn:
        Arc<dyn Fn(&str) -> synthia_guardian::PermissionLevel + Send + Sync>,
}

impl PermissionInterceptor {
    /// 创建新的 PermissionInterceptor。
    ///
    /// `check_fn` 接收工具名，返回对应的权限级别。
    pub fn new(
        check_fn: Arc<
            dyn Fn(&str) -> synthia_guardian::PermissionLevel + Send + Sync,
        >,
    ) -> Self {
        Self { check_fn }
    }

    /// 创建始终自动批准的 PermissionInterceptor（用于测试）。
    pub fn auto_approve_all() -> Self {
        Self::new(Arc::new(|_| synthia_guardian::PermissionLevel::AutoApprove))
    }

    /// 创建阻止指定工具的 PermissionInterceptor。
    pub fn blocking(tool_names: &[&str]) -> Self {
        let blocked: Vec<String> =
            tool_names.iter().map(|s| (*s).to_string()).collect();
        Self::new(Arc::new(move |name| {
            if blocked.iter().any(|b| b == name) {
                synthia_guardian::PermissionLevel::Block
            } else {
                synthia_guardian::PermissionLevel::AutoApprove
            }
        }))
    }
}

#[async_trait]
impl Interceptor for PermissionInterceptor {
    fn name(&self) -> &str {
        "permission"
    }

    async fn intercept(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
        next: NextInterceptor<'_>,
    ) -> Result<(), InterceptorError> {
        if let InterceptorEvent::BeforeTool { tool_name } = event {
            let level = (self.check_fn)(tool_name);
            match level {
                synthia_guardian::PermissionLevel::Block => {
                    tracing::warn!(
                        agent_id = %ctx.agent_id,
                        tool = %tool_name,
                        "PermissionInterceptor: tool blocked"
                    );
                    return Err(InterceptorError::ShortCircuited {
                        name: self.name().to_string(),
                    });
                }
                synthia_guardian::PermissionLevel::RequireConfirm => {
                    tracing::info!(
                        agent_id = %ctx.agent_id,
                        tool = %tool_name,
                        "PermissionInterceptor: tool requires confirmation (passing through)"
                    );
                }
                synthia_guardian::PermissionLevel::RequireExplicit => {
                    tracing::info!(
                        agent_id = %ctx.agent_id,
                        tool = %tool_name,
                        "PermissionInterceptor: tool requires explicit approval (passing through)"
                    );
                }
                synthia_guardian::PermissionLevel::AutoApprove => {}
            }
        }
        next.run(ctx, event).await
    }
}

/// LoopDetect Interceptor — 循环检测拦截。
///
/// 使用 `synthia_guardian::LoopDetectorSet` 检测工具调用循环模式。
/// 在 `BeforeTool` 事件时检查是否出现循环，如果检测到循环则短路。
/// 在 `SessionEnd` 事件时重置检测器状态。
///
/// 工具参数从 `InterceptorContext::data["tool_args"]` 读取（可选），
/// 如果未设置则使用空字符串。
pub struct LoopDetectInterceptor {
    detector: Arc<parking_lot::Mutex<synthia_guardian::LoopDetectorSet>>,
}

impl LoopDetectInterceptor {
    /// 创建新的 LoopDetectInterceptor。
    pub fn new(
        detector: Arc<parking_lot::Mutex<synthia_guardian::LoopDetectorSet>>,
    ) -> Self {
        Self { detector }
    }

    /// 创建使用默认 LoopDetectorSet 的 LoopDetectInterceptor。
    pub fn with_default_detector() -> Self {
        Self::new(Arc::new(parking_lot::Mutex::new(
            synthia_guardian::LoopDetectorSet::new(),
        )))
    }
}

#[async_trait]
impl Interceptor for LoopDetectInterceptor {
    fn name(&self) -> &str {
        "loop_detect"
    }

    async fn intercept(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
        next: NextInterceptor<'_>,
    ) -> Result<(), InterceptorError> {
        match event {
            InterceptorEvent::BeforeTool { tool_name } => {
                let args_json = ctx
                    .data
                    .get("tool_args")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let (status, action) = {
                    let mut detector = self.detector.lock();
                    detector.check(tool_name, args_json, ctx.iteration)
                };

                match status {
                    synthia_guardian::LoopStatus::Ok => {}
                    synthia_guardian::LoopStatus::Warning => {
                        tracing::warn!(
                            agent_id = %ctx.agent_id,
                            tool = %tool_name,
                            action = ?action,
                            "LoopDetectInterceptor: loop warning"
                        );
                    }
                    synthia_guardian::LoopStatus::Detected => {
                        tracing::warn!(
                            agent_id = %ctx.agent_id,
                            tool = %tool_name,
                            action = ?action,
                            "LoopDetectInterceptor: loop detected, short-circuiting"
                        );
                        return Err(InterceptorError::ShortCircuited {
                            name: self.name().to_string(),
                        });
                    }
                }
            }
            InterceptorEvent::SessionEnd => {
                self.detector.lock().reset();
            }
            _ => {}
        }
        next.run(ctx, event).await
    }
}

/// 审批决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// 审批通过。
    Approved,
    /// 审批拒绝。
    Denied,
}

/// Approval Interceptor — 审批拦截。
///
/// 基于 closure 的审批检查，在 `BeforeTool` 事件时检查工具是否需要审批。
/// 如果审批被拒绝，则短路。
pub struct ApprovalInterceptor {
    approve_fn: Arc<dyn Fn(&str) -> ApprovalDecision + Send + Sync>,
}

impl ApprovalInterceptor {
    /// 创建新的 ApprovalInterceptor。
    pub fn new(
        approve_fn: Arc<dyn Fn(&str) -> ApprovalDecision + Send + Sync>,
    ) -> Self {
        Self { approve_fn }
    }

    /// 创建始终批准的 ApprovalInterceptor（用于测试）。
    pub fn approve_all() -> Self {
        Self::new(Arc::new(|_| ApprovalDecision::Approved))
    }

    /// 创建拒绝指定工具的 ApprovalInterceptor。
    pub fn denying(tool_names: &[&str]) -> Self {
        let denied: Vec<String> =
            tool_names.iter().map(|s| (*s).to_string()).collect();
        Self::new(Arc::new(move |name| {
            if denied.iter().any(|d| d == name) {
                ApprovalDecision::Denied
            } else {
                ApprovalDecision::Approved
            }
        }))
    }
}

#[async_trait]
impl Interceptor for ApprovalInterceptor {
    fn name(&self) -> &str {
        "approval"
    }

    async fn intercept(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
        next: NextInterceptor<'_>,
    ) -> Result<(), InterceptorError> {
        if let InterceptorEvent::BeforeTool { tool_name } = event {
            let decision = (self.approve_fn)(tool_name);
            match decision {
                ApprovalDecision::Approved => {
                    tracing::debug!(
                        agent_id = %ctx.agent_id,
                        tool = %tool_name,
                        "ApprovalInterceptor: approved"
                    );
                }
                ApprovalDecision::Denied => {
                    tracing::warn!(
                        agent_id = %ctx.agent_id,
                        tool = %tool_name,
                        "ApprovalInterceptor: denied"
                    );
                    return Err(InterceptorError::ShortCircuited {
                        name: self.name().to_string(),
                    });
                }
            }
        }
        next.run(ctx, event).await
    }
}

/// Retry Interceptor — 重试拦截。
///
/// 在工具执行失败后，按指数退避策略进行重试。
/// 从 `InterceptorContext::data["tool_error"]` 读取工具执行失败标志，
/// 将重试计数写入 `data["retry_counts"]`，设置 `data["needs_retry"]`
/// 和 `data["retry_tool"]` 通知主循环执行重试。
pub struct RetryInterceptor {
    /// 最大重试次数。
    pub max_retries: u32,
    /// 基础退避延迟。
    pub base_delay: Duration,
}

impl RetryInterceptor {
    /// 创建新的 RetryInterceptor。
    pub fn new(max_retries: u32, base_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
        }
    }

    /// 获取指定工具的当前重试次数。
    fn get_retry_count(data: &serde_json::Value, tool_name: &str) -> u32 {
        data.get("retry_counts")
            .and_then(|v| v.get(tool_name))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32
    }

    /// 增加指定工具的重试次数。
    fn increment_retry_count(ctx: &mut InterceptorContext, tool_name: &str) {
        let current = Self::get_retry_count(&ctx.data, tool_name);
        let map = ctx.ensure_data_object();
        let retry_counts = map.entry("retry_counts").or_insert_with(|| {
            serde_json::Value::Object(serde_json::Map::new())
        });
        retry_counts
            .as_object_mut()
            .expect("retry_counts is guaranteed to be Object")
            .insert(tool_name.to_string(), serde_json::json!(current + 1));
    }

    /// 计算指数退避延迟。
    fn backoff_delay(&self, retry_count: u32) -> Duration {
        let exp = 1u32.checked_shl(retry_count).unwrap_or(u32::MAX);
        self.base_delay * exp
    }
}

impl Default for RetryInterceptor {
    fn default() -> Self {
        Self::new(3, Duration::from_millis(500))
    }
}

#[async_trait]
impl Interceptor for RetryInterceptor {
    fn name(&self) -> &str {
        "retry"
    }

    async fn intercept(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
        next: NextInterceptor<'_>,
    ) -> Result<(), InterceptorError> {
        next.run(ctx, event).await?;

        if let InterceptorEvent::AfterTool { tool_name } = event {
            // 检查工具是否执行失败（data["tool_error"] 被设置）
            let has_error = ctx.data.get("tool_error").is_some();
            if has_error {
                let retry_count = Self::get_retry_count(&ctx.data, tool_name);
                if retry_count < self.max_retries {
                    let delay = self.backoff_delay(retry_count);
                    tracing::info!(
                        agent_id = %ctx.agent_id,
                        tool = %tool_name,
                        retry = retry_count + 1,
                        max = self.max_retries,
                        delay_ms = delay.as_millis(),
                        "RetryInterceptor: scheduling retry"
                    );
                    Self::increment_retry_count(ctx, tool_name);
                    tokio::time::sleep(delay).await;
                    let map = ctx.ensure_data_object();
                    map.insert(
                        "needs_retry".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    map.insert(
                        "retry_tool".to_string(),
                        serde_json::Value::String(tool_name.clone()),
                    );
                } else {
                    tracing::warn!(
                        agent_id = %ctx.agent_id,
                        tool = %tool_name,
                        retries = retry_count,
                        "RetryInterceptor: max retries exhausted"
                    );
                    let map = ctx.ensure_data_object();
                    map.insert(
                        "needs_retry".to_string(),
                        serde_json::Value::Bool(false),
                    );
                }
            }
        }
        Ok(())
    }
}

/// Compact Interceptor — 压缩拦截。
///
/// 在迭代结束时检查 token 使用量是否超过阈值，
/// 如果超过则在 `InterceptorContext::data["needs_compaction"]` 中设置压缩标志。
/// Token 使用量从 `data["token_usage"]` 读取。
pub struct CompactInterceptor {
    /// Token 使用量阈值，超过则触发压缩。
    pub threshold_tokens: usize,
}

impl CompactInterceptor {
    /// 创建新的 CompactInterceptor。
    pub fn new(threshold_tokens: usize) -> Self {
        Self { threshold_tokens }
    }
}

impl Default for CompactInterceptor {
    fn default() -> Self {
        Self::new(100_000)
    }
}

#[async_trait]
impl Interceptor for CompactInterceptor {
    fn name(&self) -> &str {
        "compact"
    }

    async fn intercept(
        &self,
        ctx: &mut InterceptorContext,
        event: &InterceptorEvent,
        next: NextInterceptor<'_>,
    ) -> Result<(), InterceptorError> {
        next.run(ctx, event).await?;

        if let InterceptorEvent::IterationEnd { .. } = event {
            let token_usage = ctx
                .data
                .get("token_usage")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            if token_usage > self.threshold_tokens {
                tracing::info!(
                    agent_id = %ctx.agent_id,
                    token_usage = token_usage,
                    threshold = self.threshold_tokens,
                    "CompactInterceptor: token usage exceeds threshold, flagging compaction"
                );
                let map = ctx.ensure_data_object();
                map.insert(
                    "needs_compaction".to_string(),
                    serde_json::Value::Bool(true),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── InterceptorContext 测试 ───

    #[test]
    fn interceptor_context_new() {
        let ctx = InterceptorContext::new("agent-1", "session-1");
        assert_eq!(ctx.agent_id, "agent-1");
        assert_eq!(ctx.session_id, "session-1");
        assert_eq!(ctx.iteration, 0);
        assert!(ctx.data.is_null());
    }

    #[test]
    fn interceptor_context_ensure_data_object() {
        let mut ctx = InterceptorContext::new("agent-1", "session-1");
        assert!(ctx.data.is_null());
        let map = ctx.ensure_data_object();
        assert!(map.is_empty());
        map.insert("key".to_string(), serde_json::json!("value"));
        assert_eq!(ctx.data["key"], "value");
    }

    // ─── InterceptorChain 基础测试 ───

    #[tokio::test]
    async fn empty_chain_dispatch() {
        let chain = InterceptorChain::new();
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeLlm;
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[test]
    fn chain_default() {
        let chain = InterceptorChain::default();
        assert!(chain.is_empty());
    }

    #[test]
    fn chain_len_and_is_empty() {
        let mut chain = InterceptorChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        chain.add(Arc::new(TraceInterceptor));
        assert!(!chain.is_empty());
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn chain_clone() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(TraceInterceptor));
        let cloned = chain.clone();
        assert_eq!(cloned.len(), 1);
    }

    // ─── TraceInterceptor 测试 ───

    #[tokio::test]
    async fn trace_interceptor_before_llm() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(TraceInterceptor));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeLlm;
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn trace_interceptor_before_tool() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(TraceInterceptor));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    // ─── PermissionInterceptor 测试 ───

    #[tokio::test]
    async fn permission_auto_approve() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(PermissionInterceptor::auto_approve_all()));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "read_file".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn permission_block() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(PermissionInterceptor::blocking(&["bash"])));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        let result = chain.dispatch(&mut ctx, &event).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            InterceptorError::ShortCircuited { name } => {
                assert_eq!(name, "permission");
            }
            InterceptorError::Failed { .. } => {
                panic!("expected ShortCircuited, got Failed")
            }
        }
    }

    #[tokio::test]
    async fn permission_block_only_targeted_tools() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(PermissionInterceptor::blocking(&["bash"])));
        let mut ctx = InterceptorContext::new("test", "session-1");
        // read_file 不在 block 列表中
        let event = InterceptorEvent::BeforeTool {
            tool_name: "read_file".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn permission_require_confirm_passes_through() {
        let interceptor = PermissionInterceptor::new(Arc::new(|_| {
            synthia_guardian::PermissionLevel::RequireConfirm
        }));
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(interceptor));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        // RequireConfirm 应该通过
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn permission_require_explicit_passes_through() {
        let interceptor = PermissionInterceptor::new(Arc::new(|_| {
            synthia_guardian::PermissionLevel::RequireExplicit
        }));
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(interceptor));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn permission_ignores_non_before_tool_events() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(PermissionInterceptor::blocking(&["bash"])));
        let mut ctx = InterceptorContext::new("test", "session-1");
        // BeforeLlm 不是 BeforeTool，不应被拦截
        let event = InterceptorEvent::BeforeLlm;
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    // ─── LoopDetectInterceptor 测试 ───

    #[tokio::test]
    async fn loop_detect_ok() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(LoopDetectInterceptor::with_default_detector()));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "read_file".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn loop_detect_doom_loop_short_circuits() {
        let detector = Arc::new(parking_lot::Mutex::new(
            synthia_guardian::LoopDetectorSet::new(),
        ));
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(LoopDetectInterceptor::new(Arc::clone(&detector))));

        // 前两次调用应该通过
        for i in 0..2 {
            let mut ctx = InterceptorContext::new("test", "session-1");
            ctx.iteration = i;
            let event = InterceptorEvent::BeforeTool {
                tool_name: "tool".to_string(),
            };
            chain.dispatch(&mut ctx, &event).await.unwrap();
        }

        // 第三次调用（doom loop: 3 次连续相同调用）
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.iteration = 2;
        let event = InterceptorEvent::BeforeTool {
            tool_name: "tool".to_string(),
        };
        let result = chain.dispatch(&mut ctx, &event).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            InterceptorError::ShortCircuited { name } => {
                assert_eq!(name, "loop_detect");
            }
            InterceptorError::Failed { .. } => {
                panic!("expected ShortCircuited")
            }
        }
    }

    #[tokio::test]
    async fn loop_detect_session_end_resets() {
        let detector = Arc::new(parking_lot::Mutex::new(
            synthia_guardian::LoopDetectorSet::new(),
        ));
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(LoopDetectInterceptor::new(Arc::clone(&detector))));

        // 两次相同调用
        for i in 0..2 {
            let mut ctx = InterceptorContext::new("test", "session-1");
            ctx.iteration = i;
            let event = InterceptorEvent::BeforeTool {
                tool_name: "tool".to_string(),
            };
            chain.dispatch(&mut ctx, &event).await.unwrap();
        }

        // SessionEnd 重置检测器
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::SessionEnd;
        chain.dispatch(&mut ctx, &event).await.unwrap();

        // 重置后，相同工具应该可以再次通过
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.iteration = 0;
        let event = InterceptorEvent::BeforeTool {
            tool_name: "tool".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn loop_detect_ignores_non_before_tool_events() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(LoopDetectInterceptor::with_default_detector()));
        let mut ctx = InterceptorContext::new("test", "session-1");
        // AfterLlm 不应触发循环检测
        let event = InterceptorEvent::AfterLlm;
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn loop_detect_uses_tool_args_from_context() {
        let detector = Arc::new(parking_lot::Mutex::new(
            synthia_guardian::LoopDetectorSet::new(),
        ));
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(LoopDetectInterceptor::new(Arc::clone(&detector))));

        // 相同 tool_name 但不同 args 不应触发 doom loop
        for i in 0..3 {
            let mut ctx = InterceptorContext::new("test", "session-1");
            ctx.iteration = i;
            ctx.data = serde_json::json!({ "tool_args": format!("{{\"path\": \"file_{i}\"}}") });
            let event = InterceptorEvent::BeforeTool {
                tool_name: "read_file".to_string(),
            };
            chain.dispatch(&mut ctx, &event).await.unwrap();
        }
    }

    // ─── ApprovalInterceptor 测试 ───

    #[tokio::test]
    async fn approval_approve_all() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(ApprovalInterceptor::approve_all()));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn approval_denied() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(ApprovalInterceptor::denying(&["rm"])));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "rm".to_string(),
        };
        let result = chain.dispatch(&mut ctx, &event).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            InterceptorError::ShortCircuited { name } => {
                assert_eq!(name, "approval");
            }
            InterceptorError::Failed { .. } => {
                panic!("expected ShortCircuited")
            }
        }
    }

    #[tokio::test]
    async fn approval_denied_only_targeted_tools() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(ApprovalInterceptor::denying(&["rm"])));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "read_file".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn approval_ignores_non_before_tool_events() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(ApprovalInterceptor::denying(&["rm"])));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::AfterLlm;
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    // ─── ApprovalDecision 测试 ───

    #[test]
    fn approval_decision_equality() {
        assert_eq!(ApprovalDecision::Approved, ApprovalDecision::Approved);
        assert_eq!(ApprovalDecision::Denied, ApprovalDecision::Denied);
        assert_ne!(ApprovalDecision::Approved, ApprovalDecision::Denied);
    }

    // ─── RetryInterceptor 测试 ───

    #[tokio::test]
    async fn retry_no_error_no_retry() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(RetryInterceptor::new(3, Duration::from_millis(1))));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::AfterTool {
            tool_name: "bash".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
        // 没有设置 tool_error，不应设置 needs_retry
        assert!(ctx.data.get("needs_retry").is_none());
    }

    #[tokio::test]
    async fn retry_on_error_sets_retry_flag() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(RetryInterceptor::new(3, Duration::from_micros(1))));
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.data = serde_json::json!({ "tool_error": "command failed" });
        let event = InterceptorEvent::AfterTool {
            tool_name: "bash".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
        assert_eq!(ctx.data["needs_retry"], true);
        assert_eq!(ctx.data["retry_tool"], "bash");
        assert_eq!(ctx.data["retry_counts"]["bash"], 1);
    }

    #[tokio::test]
    async fn retry_max_retries_exhausted() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(RetryInterceptor::new(2, Duration::from_micros(1))));
        let event = InterceptorEvent::AfterTool {
            tool_name: "bash".to_string(),
        };

        // 重试 1
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.data = serde_json::json!({ "tool_error": "failed" });
        chain.dispatch(&mut ctx, &event).await.unwrap();
        assert_eq!(ctx.data["needs_retry"], true);

        // 重试 2
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.data = serde_json::json!({
            "tool_error": "failed",
            "retry_counts": { "bash": 1 }
        });
        chain.dispatch(&mut ctx, &event).await.unwrap();
        assert_eq!(ctx.data["needs_retry"], true);

        // 重试 3 — 超过 max_retries=2
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.data = serde_json::json!({
            "tool_error": "failed",
            "retry_counts": { "bash": 2 }
        });
        chain.dispatch(&mut ctx, &event).await.unwrap();
        assert_eq!(ctx.data["needs_retry"], false);
    }

    #[test]
    fn retry_backoff_delay_calculation() {
        let interceptor = RetryInterceptor::new(3, Duration::from_millis(100));
        assert_eq!(interceptor.backoff_delay(0), Duration::from_millis(100)); // 100 * 1
        assert_eq!(interceptor.backoff_delay(1), Duration::from_millis(200)); // 100 * 2
        assert_eq!(interceptor.backoff_delay(2), Duration::from_millis(400)); // 100 * 4
    }

    #[test]
    fn retry_default_values() {
        let interceptor = RetryInterceptor::default();
        assert_eq!(interceptor.max_retries, 3);
        assert_eq!(interceptor.base_delay, Duration::from_millis(500));
    }

    #[test]
    fn retry_get_retry_count() {
        let data = serde_json::json!({
            "retry_counts": { "bash": 2, "read": 0 }
        });
        assert_eq!(RetryInterceptor::get_retry_count(&data, "bash"), 2);
        assert_eq!(RetryInterceptor::get_retry_count(&data, "read"), 0);
        assert_eq!(RetryInterceptor::get_retry_count(&data, "unknown"), 0);
    }

    #[test]
    fn retry_get_retry_count_null_data() {
        let data = serde_json::Value::Null;
        assert_eq!(RetryInterceptor::get_retry_count(&data, "bash"), 0);
    }

    // ─── CompactInterceptor 测试 ───

    #[tokio::test]
    async fn compact_below_threshold() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(CompactInterceptor::new(100_000)));
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.data = serde_json::json!({ "token_usage": 50_000 });
        let event = InterceptorEvent::IterationEnd { iteration: 1 };
        chain.dispatch(&mut ctx, &event).await.unwrap();
        // 低于阈值，不应设置 needs_compaction
        assert!(ctx.data.get("needs_compaction").is_none());
    }

    #[tokio::test]
    async fn compact_above_threshold() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(CompactInterceptor::new(100_000)));
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.data = serde_json::json!({ "token_usage": 150_000 });
        let event = InterceptorEvent::IterationEnd { iteration: 1 };
        chain.dispatch(&mut ctx, &event).await.unwrap();
        assert_eq!(ctx.data["needs_compaction"], true);
    }

    #[tokio::test]
    async fn compact_no_token_usage_data() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(CompactInterceptor::new(100_000)));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::IterationEnd { iteration: 1 };
        chain.dispatch(&mut ctx, &event).await.unwrap();
        // 无 token_usage 数据，默认为 0，不应触发压缩
        assert!(ctx.data.get("needs_compaction").is_none());
    }

    #[tokio::test]
    async fn compact_ignores_non_iteration_end_events() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(CompactInterceptor::new(0)));
        let mut ctx = InterceptorContext::new("test", "session-1");
        ctx.data = serde_json::json!({ "token_usage": 999_999 });
        let event = InterceptorEvent::BeforeLlm;
        chain.dispatch(&mut ctx, &event).await.unwrap();
        assert!(ctx.data.get("needs_compaction").is_none());
    }

    #[test]
    fn compact_default_threshold() {
        let interceptor = CompactInterceptor::default();
        assert_eq!(interceptor.threshold_tokens, 100_000);
    }

    // ─── InterceptorChain::default_with_guard 测试 ───

    #[test]
    fn default_with_guard_no_optional_interceptors() {
        let chain = InterceptorChain::default_with_guard(None, None, None);
        // TraceInterceptor + RetryInterceptor + CompactInterceptor = 3
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn default_with_guard_with_all_interceptors() {
        let chain = InterceptorChain::default_with_guard(
            Some(PermissionInterceptor::auto_approve_all()),
            Some(Arc::new(parking_lot::Mutex::new(
                synthia_guardian::LoopDetectorSet::new(),
            ))),
            Some(ApprovalInterceptor::approve_all()),
        );
        // Permission + Trace + LoopDetect + Approval + Retry + Compact = 6
        assert_eq!(chain.len(), 6);
    }

    #[tokio::test]
    async fn default_with_guard_permission_first() {
        let chain = InterceptorChain::default_with_guard(
            Some(PermissionInterceptor::blocking(&["bash"])),
            None,
            None,
        );

        // PermissionInterceptor 应该在位置 0，阻止 bash
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        let result = chain.dispatch(&mut ctx, &event).await;
        assert!(result.is_err());
    }

    // ─── 链组合测试 ───

    #[tokio::test]
    async fn chain_ordering() {
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(TraceInterceptor));
        chain.add(Arc::new(ApprovalInterceptor::approve_all()));
        assert_eq!(chain.len(), 2);
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn chain_short_circuit_stops_downstream() {
        // 如果 PermissionInterceptor 阻止了工具，
        // 下游 interceptor（如 ApprovalInterceptor）不应被调用
        let mut chain = InterceptorChain::new();
        chain.add(Arc::new(PermissionInterceptor::blocking(&["rm"])));
        chain.add(Arc::new(ApprovalInterceptor::approve_all()));
        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "rm".to_string(),
        };
        let result = chain.dispatch(&mut ctx, &event).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            InterceptorError::ShortCircuited { name } => {
                assert_eq!(name, "permission");
            }
            InterceptorError::Failed { .. } => {
                panic!("expected ShortCircuited")
            }
        }
    }

    #[tokio::test]
    async fn full_guard_chain_happy_path() {
        let chain = InterceptorChain::default_with_guard(
            Some(PermissionInterceptor::auto_approve_all()),
            Some(Arc::new(parking_lot::Mutex::new(
                synthia_guardian::LoopDetectorSet::new(),
            ))),
            Some(ApprovalInterceptor::approve_all()),
        );

        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "read_file".to_string(),
        };
        chain.dispatch(&mut ctx, &event).await.unwrap();
    }

    #[tokio::test]
    async fn full_guard_chain_blocked_by_permission() {
        let chain = InterceptorChain::default_with_guard(
            Some(PermissionInterceptor::blocking(&["rm"])),
            Some(Arc::new(parking_lot::Mutex::new(
                synthia_guardian::LoopDetectorSet::new(),
            ))),
            Some(ApprovalInterceptor::approve_all()),
        );

        let mut ctx = InterceptorContext::new("test", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "rm".to_string(),
        };
        let result = chain.dispatch(&mut ctx, &event).await;
        assert!(result.is_err());
    }

    // ─── 安全守卫不可绕过测试 ───

    /// 可观测的 interceptor：通过 `Arc<AtomicBool>` 记录是否被调用。
    struct ObservableInterceptor {
        name: &'static str,
        called: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait]
    impl Interceptor for ObservableInterceptor {
        fn name(&self) -> &str {
            self.name
        }

        async fn intercept(
            &self,
            ctx: &mut InterceptorContext,
            event: &InterceptorEvent,
            next: NextInterceptor<'_>,
        ) -> Result<(), InterceptorError> {
            self.called.store(true, std::sync::atomic::Ordering::SeqCst);
            next.run(ctx, event).await
        }
    }

    /// 验证 PermissionInterceptor 在位置 0 时，Block 决策短路执行，
    /// 下游 interceptor 绝不会被调用——安全守卫不可绕过。
    #[tokio::test]
    async fn permission_block_at_position_0_is_unbypassable() {
        let downstream_called =
            Arc::new(std::sync::atomic::AtomicBool::new(false));

        let mut chain = InterceptorChain::new();
        // 位置 0: PermissionInterceptor — 阻止 "dangerous_tool"
        chain.add(Arc::new(PermissionInterceptor::blocking(&[
            "dangerous_tool",
        ])));
        // 位置 1: 可观测 interceptor — 如果被调用则设置 flag
        chain.add(Arc::new(ObservableInterceptor {
            name: "observable",
            called: Arc::clone(&downstream_called),
        }));
        // 位置 2: 另一个可观测 interceptor
        let second_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        chain.add(Arc::new(ObservableInterceptor {
            name: "observable_2",
            called: Arc::clone(&second_called),
        }));

        let mut ctx = InterceptorContext::new("agent-1", "session-1");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "dangerous_tool".to_string(),
        };

        let result = chain.dispatch(&mut ctx, &event).await;

        // PermissionInterceptor 必须短路
        assert!(result.is_err(), "expected ShortCircuited error");
        match result.unwrap_err() {
            InterceptorError::ShortCircuited { name } => {
                assert_eq!(
                    name, "permission",
                    "short-circuit must originate from permission interceptor"
                );
            }
            InterceptorError::Failed { .. } => {
                panic!("expected ShortCircuited, got Failed")
            }
        }

        // 下游 interceptor 绝不能被调用
        assert!(
            !downstream_called.load(std::sync::atomic::Ordering::SeqCst),
            "downstream interceptor after permission block must NOT be called"
        );
        assert!(
            !second_called.load(std::sync::atomic::Ordering::SeqCst),
            "second downstream interceptor after permission block must NOT be called"
        );

        // 非阻止工具应正常通过
        let mut ctx2 = InterceptorContext::new("agent-1", "session-1");
        let event2 = InterceptorEvent::BeforeTool {
            tool_name: "safe_tool".to_string(),
        };
        chain.dispatch(&mut ctx2, &event2).await.unwrap();
        assert!(
            downstream_called.load(std::sync::atomic::Ordering::SeqCst),
            "downstream interceptor SHOULD be called for non-blocked tools"
        );
        assert!(
            second_called.load(std::sync::atomic::Ordering::SeqCst),
            "second downstream interceptor SHOULD be called for non-blocked tools"
        );
    }
}
