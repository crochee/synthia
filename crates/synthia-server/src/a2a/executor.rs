//! SynthiaExecutor — 桥接 A2A AgentExecutor 到 synthia SessionController。
//!
//! 当 A2A 客户端发送 `message/send` 或 `message/stream` 时，
//! `SynthiaExecutor::execute()` 将请求转换为 Synthia prompt 并提交到 SessionController，
//! 然后将 EventBroadcaster 的 AgentEvent 流映射为 A2A StreamResponse。

use std::sync::Arc;

use a2a::{A2AError, Message, StreamResponse, Task, TaskState};
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

        // 使用 task_id 作为 Synthia session_id。A2A 客户端通常先发
        // SendMessage 让 server 生成 task_id,再带同一 task_id 继续回合。
        let session_id = task_id.clone();

        // 协议契约 (A2A v1.0.0 + a2a-server-lf 0.4.1):
        // stream 的第 1 项必须是 `StreamResponse::Task(initial_task)`。
        // 否则 DefaultRequestHandler::send_message 在 last_event=None 时
        // 仅返回 task_store 里的当前快照,客户端永远看不到
        // Submitted → Working → Completed 的状态过渡。
        // 优先复用 a2a-server-lf 已经在 prepare_task_for_execution 中
        // 创建并持久化的 stored_task(状态 Submitted)。
        let initial_task: Task = ctx.stored_task.clone().unwrap_or_else(|| {
            task_with_state(
                task_id.clone(),
                context_id.clone(),
                TaskState::Submitted,
                None,
            )
        });

        Box::pin(async_stream::try_stream! {
            // ─── 1. yield initial Task(Submitted) ─────────────────────────
            yield StreamResponse::Task(initial_task);

            // ─── 2. 获取或创建 SessionController ────────────────────────
            let controller = state
                .get_or_create_session_controller(A2A_USER_ID, &session_id)
                .await
                .map_err(|e| A2AError::internal(format!("failed to create session: {e:?}")))?;

            // ─── 3. 提交 prompt ──────────────────────────────────────────
            controller
                .submit(crate::session::controller::SessionOp::Prompt {
                    content: prompt_text,
                    priority: 1,
                })
                .await
                .map_err(|e| A2AError::internal(format!("failed to submit prompt: {e:?}")))?;

            // ─── 4. 订阅事件流,映射为 A2A StreamResponse ───────────────
            let mut rx = controller.subscribe();

            while let Ok(event) = rx.recv().await {
                for resp in agent_event_to_stream_responses(&event, &task_id, &context_id) {
                    yield resp?;
                }

                // 终端状态时结束流;handler 基于最后一个事件保存终态 task。
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
///
/// 仅 [`SystemEvent::SessionEnded`] 与 [`SystemEvent::SessionInterrupted`] 是
/// 真正的终止事件 — 它们之后代理流程会停止并清理资源。
///
/// 历史上 `ModelDone` 也被视作终止事件,但它只是“当前 LLM 采样完成”,
/// 并不代表 A2A 任务结束: 代理仍可能进行反思 / 自动压缩 /
/// 子会话广播等步骤, 之后才会发出 `SessionEnded`。若在此处提前
/// 断开 `broadcast::Receiver`, 随后到来的 `SessionEnded` 状态更新
/// 会因 “无订阅者” 而丢失, 前端永远停留在 `working`。
fn is_terminal_event(event: &synthia_agent::AgentEvent) -> bool {
    matches!(
        event,
        synthia_agent::AgentEvent::System(
            synthia_agent::events::SystemEvent::SessionEnded { .. }
        ) | synthia_agent::AgentEvent::System(
            synthia_agent::events::SystemEvent::SessionInterrupted { .. }
        )
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
    use synthia_agent::events::{SessionEndReason, SystemEvent};
    use synthia_provider::ContentPart;

    use super::*;

    #[test]
    fn is_terminal_event_detects_session_ended() {
        let event =
            synthia_agent::AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            });
        assert!(is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_detects_session_interrupted() {
        let event = synthia_agent::AgentEvent::System(
            SystemEvent::SessionInterrupted {
                reason: "test".to_string(),
            },
        );
        assert!(is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_does_not_detect_model_done() {
        // ModelDone 表示“当前 LLM 采样完成”，并不代表 A2A 任务结束。
        // 后续代理仍可能继续运行（反思 / 自动压缩等）并最终发出 SessionEnded，
        // 因此 ModelDone 不应让执行器提前断开 broadcast::Receiver。
        let event = synthia_agent::AgentEvent::ModelDone(
            synthia_provider::SamplingResult {
                text: "done".to_string(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: synthia_provider::types::TokenUsage::default(),
            },
        );
        assert!(!is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_rejects_non_terminal() {
        let event = synthia_agent::AgentEvent::Model(ContentPart::Text(
            synthia_provider::TextContent {
                text: "hi".to_string(),
                cache_control: None,
            },
        ));
        assert!(!is_terminal_event(&event));
    }
}
