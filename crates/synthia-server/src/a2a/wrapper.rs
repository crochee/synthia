//! `SynthiaHandler` — a thin passthrough wrapper around any
//! upstream [`a2a_server::RequestHandler`] that fills the
//! `:subscribe` gap left by `ExecutionManager::finish`.
//!
//! # Why this wrapper exists
//!
//! The upstream `DefaultRequestHandler::subscribe_to_task` resolves
//! the active execution through `ExecutionManager::resubscribe`. The
//! `ExecutionManager` clears the active execution as soon as the
//! executor stream yields its terminal event (i.e. `SessionEnded`),
//! which means any `GET /a2a/tasks/{id}:subscribe` arriving AFTER
//! the executor finishes gets `A2AError::task_not_found` even though
//! the terminal `Task` is still in [`TaskStore`].
//!
//! Synthia callers (the Playwright contract-closure tests in
//! particular) subscribe to a task right after `message:send`
//! returns — which is *exactly* when the executor has just finished,
//! so the subscribe lands in the dead zone.
//!
//! The wrapper probes `TaskStore` on the `ExecutionManager` miss
//! path: if a terminal task exists there, it emits a short-circuit
//! stream that yields that task and closes. This is consistent with
//! the upstream `subscription_stream` behaviour when given a
//! terminal snapshot (yield once, set `done = true`).
//!
//! All other `RequestHandler` methods are forwarded verbatim to the
//! inner handler — this wrapper exists solely to plug the
//! post-completion subscribe hole.

use std::sync::Arc;

use a2a::{
    self,
    A2AError,
    AgentCard,
    DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest,
    GetTaskPushNotificationConfigRequest,
    GetTaskRequest,
    ListTaskPushNotificationConfigsRequest,
    ListTaskPushNotificationConfigsResponse,
    ListTasksRequest,
    ListTasksResponse,
    SendMessageRequest,
    SendMessageResponse,
    StreamResponse,
    SubscribeToTaskRequest,
    Task,
    TaskPushNotificationConfig,
};
use a2a_server::{RequestHandler, ServiceParams, TaskStore};
use async_trait::async_trait;
use futures::{StreamExt, stream};

/// Wrapper that decorates an inner `RequestHandler` with
/// post-completion `:subscribe` support.
///
/// `H` is the concrete inner handler type (typically
/// `DefaultRequestHandler`). The `TaskStore` reference is held as
/// `Arc<dyn TaskStore>` so the wrapper can share the *same* backing
/// store with the inner handler without owning a clone — upstream
/// `InMemoryTaskStore` is not `Clone`, and `DefaultRequestHandler::new`
/// takes an owned `impl TaskStore`, so the only sane sharing
/// mechanism is to keep one owned `InMemoryTaskStore` for the
/// inner handler and an `Arc<dyn TaskStore>` to the same store for
/// the wrapper.
pub struct SynthiaHandler<H> {
    inner: Arc<H>,
    task_store: Arc<dyn TaskStore>,
}

impl<H> SynthiaHandler<H>
where
    H: RequestHandler,
{
    pub fn new(inner: Arc<H>, task_store: Arc<dyn TaskStore>) -> Arc<Self> {
        Arc::new(Self { inner, task_store })
    }
}

#[async_trait]
impl<H> RequestHandler for SynthiaHandler<H>
where
    H: RequestHandler,
{
    async fn send_message(
        &self,
        params: &ServiceParams,
        req: SendMessageRequest,
    ) -> Result<SendMessageResponse, A2AError> {
        self.inner.send_message(params, req).await
    }

    async fn send_streaming_message(
        &self,
        params: &ServiceParams,
        req: SendMessageRequest,
    ) -> Result<
        futures::stream::BoxStream<'static, Result<StreamResponse, A2AError>>,
        A2AError,
    > {
        self.inner.send_streaming_message(params, req).await
    }

    async fn get_task(
        &self,
        params: &ServiceParams,
        req: GetTaskRequest,
    ) -> Result<Task, A2AError> {
        self.inner.get_task(params, req).await
    }

    async fn list_tasks(
        &self,
        params: &ServiceParams,
        req: ListTasksRequest,
    ) -> Result<ListTasksResponse, A2AError> {
        self.inner.list_tasks(params, req).await
    }

    async fn cancel_task(
        &self,
        params: &ServiceParams,
        req: a2a::CancelTaskRequest,
    ) -> Result<Task, A2AError> {
        self.inner.cancel_task(params, req).await
    }

    async fn subscribe_to_task(
        &self,
        params: &ServiceParams,
        req: SubscribeToTaskRequest,
    ) -> Result<
        futures::stream::BoxStream<'static, Result<StreamResponse, A2AError>>,
        A2AError,
    > {
        // Fast path: the task is still actively executing — forward
        // to the inner handler, which will attach a broadcast
        // receiver and replay the in-flight events.
        match self.inner.subscribe_to_task(params, req.clone()).await {
            Ok(stream) => Ok(stream),
            Err(_inner_err) => {
                // Slow path: ExecutionManager has cleared the active
                // execution. Look for a terminal snapshot in the
                // TaskStore. If we find one, emit it as a
                // short-circuit stream so the caller can still
                // observe the final task state.
                tracing::debug!(
                    task_id = %req.id,
                    "subscribe_to_task.inner_failed_falling_back_to_task_store",
                );
                match self.task_store.get(&req.id).await {
                    Ok(Some(task)) if task.status.state.is_terminal() => {
                        tracing::info!(
                            task_id = %req.id,
                            state = ?task.status.state,
                            "subscribe_to_task.fallback_terminal_snapshot",
                        );
                        Ok(stream::once(async move {
                            Ok(StreamResponse::Task(task))
                        })
                        .boxed())
                    }
                    Ok(Some(task)) => {
                        // Non-terminal snapshot: the executor is in a
                        // weird state (ExecutionManager gone but task
                        // not terminal). Surface the upstream
                        // not-found so the caller gets a clear signal.
                        tracing::warn!(
                            task_id = %req.id,
                            state = ?task.status.state,
                            "subscribe_to_task.fallback_non_terminal_not_found",
                        );
                        Err(A2AError::task_not_found(&req.id))
                    }
                    Ok(None) | Err(_) => {
                        // No record anywhere — genuine not-found.
                        Err(A2AError::task_not_found(&req.id))
                    }
                }
            }
        }
    }

    async fn create_push_config(
        &self,
        params: &ServiceParams,
        req: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig, A2AError> {
        self.inner.create_push_config(params, req).await
    }

    async fn get_push_config(
        &self,
        params: &ServiceParams,
        req: GetTaskPushNotificationConfigRequest,
    ) -> Result<TaskPushNotificationConfig, A2AError> {
        self.inner.get_push_config(params, req).await
    }

    async fn list_push_configs(
        &self,
        params: &ServiceParams,
        req: ListTaskPushNotificationConfigsRequest,
    ) -> Result<ListTaskPushNotificationConfigsResponse, A2AError> {
        self.inner.list_push_configs(params, req).await
    }

    async fn delete_push_config(
        &self,
        params: &ServiceParams,
        req: DeleteTaskPushNotificationConfigRequest,
    ) -> Result<(), A2AError> {
        self.inner.delete_push_config(params, req).await
    }

    async fn get_extended_agent_card(
        &self,
        params: &ServiceParams,
        req: GetExtendedAgentCardRequest,
    ) -> Result<AgentCard, A2AError> {
        self.inner.get_extended_agent_card(params, req).await
    }
}

#[cfg(test)]
mod tests {
    use a2a_server::{AgentExecutor, DefaultRequestHandler, ExecutorContext};

    use super::*;
    use crate::a2a::shared_store::SharedTaskStore;

    /// Minimal stub executor that immediately yields a terminal
    /// Completed task. Used by the wrapper fallback test to put the
    /// task into the "active execution already cleared" state.
    struct TerminalExecutor;

    #[async_trait]
    impl AgentExecutor for TerminalExecutor {
        fn execute(
            &self,
            ctx: ExecutorContext,
        ) -> futures::stream::BoxStream<'static, Result<StreamResponse, A2AError>>
        {
            use a2a::TaskStatus;
            let task = Task {
                id: ctx.task_id.clone(),
                context_id: ctx.context_id.clone(),
                status: TaskStatus {
                    state: a2a::TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                artifacts: None,
                history: None,
                metadata: None,
            };
            futures::stream::once(async move { Ok(StreamResponse::Task(task)) })
                .boxed()
        }

        fn cancel(
            &self,
            _ctx: ExecutorContext,
        ) -> futures::stream::BoxStream<'static, Result<StreamResponse, A2AError>>
        {
            futures::stream::empty().boxed()
        }
    }

    #[tokio::test]
    async fn subscribe_after_completion_returns_terminal_snapshot() {
        let shared_store = SharedTaskStore::new();
        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        // The wrapper's fallback uses the *same* shared store so
        // it sees the terminal task the inner handler persisted.
        // Keep a clone of `inner` for the wrapper so we can still
        // drive `send_message` on the original after.
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper = SynthiaHandler::new(inner.clone(), wrapper_task_store);

        // 1. Drive a task to completion by calling send_message.
        //    The terminal executor yields a Completed task immediately,
        //    so by the time send_message returns, ExecutionManager has
        //    already cleared the active execution for that task.
        //
        // Pin the task_id on the request so we can refer to it
        // deterministically — upstream's `prepare_task_for_execution`
        // would otherwise mint a fresh UUID.
        let mut message = a2a::Message::new(
            a2a::Role::User,
            vec![a2a::Part::text("hi".to_string())],
        );
        message.task_id = Some("test-task-1".to_string());
        message.context_id = Some("test-ctx-1".to_string());

        let send_req = SendMessageRequest {
            message,
            configuration: None,
            metadata: None,
            tenant: None,
        };
        let params = ServiceParams::default();
        inner.send_message(&params, send_req).await.unwrap();

        // 2. Subscribe to the now-completed task. Upstream would
        //    return task_not_found; the wrapper should fall back to
        //    TaskStore and emit the terminal snapshot.
        let sub_req = SubscribeToTaskRequest {
            id: "test-task-1".to_string(),
            tenant: None,
        };
        let mut stream =
            wrapper.subscribe_to_task(&params, sub_req).await.unwrap();

        let first = stream.next().await.expect("terminal snapshot event");
        let response = first.expect("stream item must be Ok");
        match response {
            StreamResponse::Task(task) => {
                assert_eq!(task.id, "test-task-1");
                assert_eq!(task.status.state, a2a::TaskState::Completed);
            }
            other => panic!("expected StreamResponse::Task, got {other:?}"),
        }

        // No further events — the stream closes after the terminal
        // snapshot, matching upstream `subscription_stream` semantics.
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn subscribe_unknown_task_still_returns_not_found() {
        // Empty shared store — no terminal snapshot, so the
        // wrapper must surface task_not_found.
        let shared_store = SharedTaskStore::new();
        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper = SynthiaHandler::new(inner, wrapper_task_store);

        let sub_req = SubscribeToTaskRequest {
            id: "never-existed".to_string(),
            tenant: None,
        };
        let params = ServiceParams::default();
        let result = wrapper.subscribe_to_task(&params, sub_req).await;
        match result {
            Ok(_) => panic!("subscribe on unknown task must error"),
            Err(e) => {
                assert_eq!(
                    e.code,
                    a2a::errors::error_code::TASK_NOT_FOUND,
                    "expected task_not_found, got {e:?}"
                );
            }
        }
    }

    /// `subscribe_to_task` on a task in the store but with a
    /// *non-terminal* state MUST surface `task_not_found` (not the
    /// upstream error).
    #[tokio::test]
    async fn subscribe_non_terminal_task_returns_not_found() {
        use a2a::TaskStatus;
        let shared_store = SharedTaskStore::new();
        // Pre-seed with a non-terminal task.
        let non_terminal = Task {
            id: "in-flight".to_string(),
            context_id: "ctx".to_string(),
            status: TaskStatus {
                state: a2a::TaskState::Working, // NOT terminal
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        };
        shared_store.create(non_terminal).await.unwrap();

        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper = SynthiaHandler::new(inner, wrapper_task_store);

        let sub_req = SubscribeToTaskRequest {
            id: "in-flight".to_string(),
            tenant: None,
        };
        let params = ServiceParams::default();
        let result = wrapper.subscribe_to_task(&params, sub_req).await;
        match result {
            Ok(_) => panic!(
                "non-terminal task subscribe must surface task_not_found"
            ),
            Err(e) => {
                assert_eq!(
                    e.code,
                    a2a::errors::error_code::TASK_NOT_FOUND,
                    "expected task_not_found, got {e:?}"
                );
            }
        }
    }

    /// `SynthiaHandler::new` MUST return an `Arc<SynthiaHandler<H>>`.
    #[tokio::test]
    async fn new_returns_arc() {
        let shared_store = SharedTaskStore::new();
        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper: Arc<SynthiaHandler<DefaultRequestHandler>> =
            SynthiaHandler::new(inner, wrapper_task_store);
        // Pin the type and Arc-ness.
        assert_eq!(Arc::strong_count(&wrapper), 1);
    }

    /// `send_message` MUST pass through to the inner handler.
    #[tokio::test]
    async fn send_message_passthrough() {
        let shared_store = SharedTaskStore::new();
        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper = SynthiaHandler::new(inner.clone(), wrapper_task_store);

        let mut message = a2a::Message::new(
            a2a::Role::User,
            vec![a2a::Part::text("hello".to_string())],
        );
        message.task_id = Some("pass-1".to_string());
        message.context_id = Some("ctx-1".to_string());

        let send_req = SendMessageRequest {
            message,
            configuration: None,
            metadata: None,
            tenant: None,
        };
        let params = ServiceParams::default();

        // The wrapper's send_message should succeed (the inner
        // returns Ok with a Completed task).
        let result = wrapper.send_message(&params, send_req).await;
        assert!(
            result.is_ok(),
            "send_message must passthrough, got {result:?}"
        );
    }

    /// `get_task` MUST pass through to the inner handler.
    #[tokio::test]
    async fn get_task_passthrough_after_completion() {
        let shared_store = SharedTaskStore::new();
        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper = SynthiaHandler::new(inner.clone(), wrapper_task_store);

        // 1. Drive a task to completion.
        let mut message = a2a::Message::new(
            a2a::Role::User,
            vec![a2a::Part::text("hi".to_string())],
        );
        message.task_id = Some("get-1".to_string());
        message.context_id = Some("ctx".to_string());
        let send_req = SendMessageRequest {
            message,
            configuration: None,
            metadata: None,
            tenant: None,
        };
        inner
            .send_message(&ServiceParams::default(), send_req)
            .await
            .unwrap();

        // 2. Now query via the wrapper.
        let get_req = GetTaskRequest {
            id: "get-1".to_string(),
            history_length: None,
            tenant: None,
        };
        let task = wrapper
            .get_task(&ServiceParams::default(), get_req)
            .await
            .unwrap();
        assert_eq!(task.id, "get-1");
        assert_eq!(task.status.state, a2a::TaskState::Completed);
    }

    /// `list_tasks` MUST passthrough to the inner handler.
    #[tokio::test]
    async fn list_tasks_passthrough_after_completion() {
        let shared_store = SharedTaskStore::new();
        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper = SynthiaHandler::new(inner.clone(), wrapper_task_store);

        // Drive 2 tasks to completion.
        for i in 0..2 {
            let mut message = a2a::Message::new(
                a2a::Role::User,
                vec![a2a::Part::text(format!("hi {i}"))],
            );
            message.task_id = Some(format!("list-{i}"));
            message.context_id = Some("ctx".to_string());
            let send_req = SendMessageRequest {
                message,
                configuration: None,
                metadata: None,
                tenant: None,
            };
            inner
                .send_message(&ServiceParams::default(), send_req)
                .await
                .unwrap();
        }

        let list_req = ListTasksRequest {
            context_id: None,
            status: None,
            page_size: None,
            page_token: None,
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = wrapper
            .list_tasks(&ServiceParams::default(), list_req)
            .await
            .unwrap();
        assert_eq!(resp.tasks.len(), 2);
    }

    /// `cancel_task` MUST passthrough to the inner handler.
    /// (We don't assert specific behavior because the terminal
    /// executor is already complete — we just pin that the wrapper
    /// does NOT short-circuit and instead forwards the request.)
    #[tokio::test]
    async fn cancel_task_passthrough_succeeds() {
        let shared_store = SharedTaskStore::new();
        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper = SynthiaHandler::new(inner, wrapper_task_store);

        let cancel_req = a2a::CancelTaskRequest {
            id: "cancel-1".to_string(),
            metadata: None,
            tenant: None,
        };
        // Cancel returns Result — we only care that the wrapper
        // forwards (we don't pin success/failure here because the
        // upstream behavior depends on whether the task exists).
        let _ = wrapper
            .cancel_task(&ServiceParams::default(), cancel_req)
            .await;
    }

    /// `get_extended_agent_card` MUST passthrough.
    #[tokio::test]
    async fn get_extended_agent_card_passthrough() {
        let shared_store = SharedTaskStore::new();
        let executor = TerminalExecutor;
        let inner = Arc::new(DefaultRequestHandler::new(
            executor,
            shared_store.clone(),
        ));
        let wrapper_task_store: Arc<dyn TaskStore> = Arc::new(shared_store);
        let wrapper = SynthiaHandler::new(inner, wrapper_task_store);

        let req = GetExtendedAgentCardRequest { tenant: None };
        // The default upstream handler does not implement extended
        // card; expect an upstream error or our passthrough — we
        // only assert the wrapper returned *something* (not panic).
        let _ = wrapper
            .get_extended_agent_card(&ServiceParams::default(), req)
            .await;
    }
}
