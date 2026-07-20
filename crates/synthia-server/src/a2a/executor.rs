//! SynthiaExecutor — 桥接 A2A AgentExecutor 到 synthia SessionController。
//!
//! 当 A2A 客户端发送 `message/send` 或 `message/stream` 时，
//! `SynthiaExecutor::execute()` 将请求转换为 Synthia prompt 并提交到 SessionController，
//! 然后将 EventBroadcaster 的 AgentEvent 流映射为 A2A StreamResponse。

use std::sync::Arc;

use a2a::{A2AError, Message, StreamResponse, TaskState};
use a2a_server::{AgentExecutor, ExecutorContext};
use futures::{
    StreamExt,
    stream::{self, BoxStream},
};
use synthia_a2a::mapping::{
    agent_event_to_stream_responses,
    extract_text_from_message,
    task_with_state,
};

use crate::state::AppState;

/// A2A user_id — 固定标识 A2A 协议发起的请求。
const A2A_USER_ID: &str = "a2a";

/// 实现 A2A `AgentExecutor` trait，桥接 Synthia SessionController。
pub struct SynthiaExecutor {
    state: Arc<AppState>,
}

impl SynthiaExecutor {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl AgentExecutor for SynthiaExecutor {
    fn execute(
        &self,
        ctx: ExecutorContext,
    ) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        let state = self.state.clone();
        let task_id = ctx.task_id.clone();
        let context_id = ctx.context_id.clone();

        // 从 A2A Message 提取文本 prompt
        let prompt_text = ctx
            .message
            .as_ref()
            .and_then(extract_text_from_message)
            .unwrap_or_default();

        if prompt_text.is_empty() {
            return stream::once(async move {
                Ok(StreamResponse::Task(task_with_state(
                    task_id,
                    context_id,
                    TaskState::Failed,
                    Some(empty_error_message("empty prompt")),
                )))
            })
            .boxed();
        }

        // 使用 task_id 作为 session_id
        let session_id = task_id.clone();

        Box::pin(async_stream::try_stream! {
            // 获取或创建 SessionController
            let controller = state
                .get_or_create_session_controller(A2A_USER_ID, &session_id)
                .await
                .map_err(|e| A2AError::internal(format!("failed to create session: {e:?}")))?;

            // 提交 prompt
            controller
                .submit(crate::session::controller::SessionOp::Prompt {
                    content: prompt_text,
                    priority: 1,
                })
                .await
                .map_err(|e| A2AError::internal(format!("failed to submit prompt: {e:?}")))?;

            // 订阅事件流
            let mut rx = controller.subscribe();

            // 将 AgentEvent 流映射为 StreamResponse
            while let Ok(event) = rx.recv().await {
                let responses = agent_event_to_stream_responses(
                    &event,
                    &task_id,
                    &context_id,
                );
                for resp in responses {
                    yield resp?;
                }

                // 终端状态时结束流
                if is_terminal_event(&event) {
                    break;
                }
            }
        })
    }

    fn cancel(
        &self,
        ctx: ExecutorContext,
    ) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        let state = self.state.clone();
        let task_id = ctx.task_id.clone();
        let context_id = ctx.context_id.clone();

        Box::pin(async_stream::stream! {
            // 尝试取消 session
            if let Ok(controller) = state
                .get_or_create_session_controller(A2A_USER_ID, &task_id)
                .await
            {
                let _ = controller.cancel().await;
            }

            yield Ok(StreamResponse::Task(task_with_state(
                task_id,
                context_id,
                TaskState::Canceled,
                None,
            )));
        })
    }
}

/// 判断事件是否代表终端状态。
fn is_terminal_event(event: &synthia_agent::AgentEvent) -> bool {
    matches!(
        event,
        synthia_agent::AgentEvent::SessionEnded { .. }
            | synthia_agent::AgentEvent::SessionInterrupted { .. }
            | synthia_agent::AgentEvent::Finish { .. }
    )
}

/// 构建一个空 prompt 错误消息。
fn empty_error_message(reason: &str) -> Message {
    Message::new(
        a2a::Role::Agent,
        vec![a2a::Part::text(format!("error: {reason}"))],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_terminal_event_detects_session_ended() {
        let event = synthia_agent::AgentEvent::SessionEnded {
            reason: synthia_agent::events::SessionEndReason::Completed,
        };
        assert!(is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_detects_session_interrupted() {
        let event = synthia_agent::AgentEvent::SessionInterrupted {
            reason: "test".to_string(),
        };
        assert!(is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_detects_finish() {
        let event = synthia_agent::AgentEvent::Finish {
            output: "done".to_string(),
        };
        assert!(is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_rejects_non_terminal() {
        let event = synthia_agent::AgentEvent::LlmStreamDelta {
            content: "hi".to_string(),
        };
        assert!(!is_terminal_event(&event));
    }
}
