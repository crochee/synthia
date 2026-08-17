//! SynthiaExecutor — 桥接 A2A AgentExecutor 到 synthia SessionController。
//!
//! 当 A2A 客户端发送 `message/send` 或 `message/stream` 时，
//! `SynthiaExecutor::execute()` 将请求转换为 Synthia prompt 并提交到 SessionController，
//! 然后将 EventBroadcaster 的 AgentEvent 流映射为 A2A StreamResponse。
//!
//! ## 历史持久化 (Task.history)
//!
//! 上游 `a2a-server-lf@0.4.1` 的 `apply_event_to_task` 在收到
//! `StreamResponse::Message` 时**丢弃**该事件，模型文本、reasoning
//! 等内容因此不会进入 `Task.history`，后续通过 `GET /api/v1/tasks/{id}`
//! 拉取详情时只能看到 user 消息（由 `prepare_task_for_execution`
//! 初始化时写入）和 tool 调用的 artifacts，缺少完整的 agent 回复
//! 文本。本执行器在流式处理过程中自维护一个 [`TaskHistoryBuilder`]
//! 累加器，遵守 A2A 协议约定：
//!
//! - `Task.history` 装对话消息（user 文本、agent 文本、tool_use /
//!   tool_result 的 `Part::data` 引用），由 `Message::new` 构造，
//!   不污染 [`Artifact`] 通道。
//! - `Task.artifacts` 仅装工具副作用 / 附件，保持原语义。
//!
//! 流结束后（[`AgentEvent::System(SystemEvent::SessionEnded)`]）调用
//! [`TaskStore::update`] 把累加的 history 写回 task，使后续
//! `GET /api/v1/tasks/{id}` 拿到完整会话原文。

use std::{sync::Arc, time::Duration};

use a2a::{A2AError, StreamResponse, Task, TaskState};
use a2a_server::{AgentExecutor, ExecutorContext, TaskStore};
use futures::{
    StreamExt,
    stream::{self, BoxStream},
};
use synthia_agent::AgentEvent;
use tokio::{
    sync::broadcast::error::RecvError,
    time::{Instant, interval},
};
use tracing::{debug, info, warn};

use super::{
    mapping::{
        agent_event_to_stream_responses,
        extract_text_from_message,
        task_with_state,
        working_status_update,
    },
    task_history::TaskHistoryBuilder,
};
use crate::state::AppState;

/// A2A streaming heartbeat interval.
///
/// Synthia's executor yields a heartbeat `StatusUpdate` every
/// `HEARTBEAT_INTERVAL` while waiting on the agent's event stream so
/// intermediaries (nginx, enterprise proxies, browser idle timers)
/// don't silently close the SSE connection during long quiet phases
/// (LLM thinking, multi-minute tool runs). 15s is short enough that
/// nginx's default `proxy_read_timeout` (60s) and most enterprise
/// proxies' idle timers never fire, and long enough not to drown
/// legitimate event flow.
pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// Decide whether a heartbeat should be emitted at the current
/// `Instant`. The rule: skip if the most recent real event
/// happened strictly less than one `HEARTBEAT_INTERVAL` ago.
/// Equivalent to "no quiet gap to bridge".
///
/// Extracted so the decision can be unit-tested without
/// standing up a full agent loop + broadcaster + SSE stream.
fn should_emit_heartbeat(last_event_at: Instant, now: Instant) -> bool {
    now.duration_since(last_event_at) >= HEARTBEAT_INTERVAL
}

/// Map an A2A (`task_id`, `context_id`) pair to the Synthia
/// `session_id` that owns the long-lived conversation state.
///
/// **Protocol contract** (A2A v1.0, `a2a.proto:254-259`):
/// - `context_id` is "the contextual collection of interactions
///   (tasks and messages)" — i.e. an ongoing conversation.
/// - `task_id` is a per-`message:send` invocation. The client
///   may supply its own `task_id` to *continue* an existing
///   task, or omit it to let the server mint a new one; in
///   either case every individual message:send is its own A2A
///   task with its own lifecycle.
///
/// **Mapping rule**:
/// - If `context_id` is non-empty, the Synthia session is
///   bound to the context (the conversation). This is the
///   normal multi-turn chat path: the frontend keeps one
///   `sessionId` (which it forwards as `contextId`) and every
///   user message — even when the server mints a fresh
///   `task_id` for each — lands in the same Synthia
///   `SessionController`.
/// - If `context_id` is empty, the very first message of a
///   brand-new chat has no established context yet, so we
///   fall back to `task_id`. The A2A proto explicitly allows
///   this: "If only `task_id` is provided, the server will
///   infer `context_id` from it" — meaning the server-side
///   task_store will populate `context_id` from the
///   `task_id` we use here, and subsequent follow-up
///   messages from the same client will arrive with a
///   non-empty `context_id` and route here as well.
///
/// Exposed as a free function (not an inline `if`) so the
/// rule can be unit-tested without spinning up a full
/// `ExecutorContext` / `AppState`.
fn session_id_for_task(task_id: &str, context_id: &str) -> String {
    if !context_id.is_empty() {
        context_id.to_string()
    } else {
        task_id.to_string()
    }
}

/// Truncate a string for log output without leaking very long prompts.
fn preview(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// Record an `AgentEvent` into the per-task history builder so that
/// `Task.history` ends up with the full conversation transcript
/// (user prompt, agent text, tool_use, tool_result).
///
/// A2A's upstream `apply_event_to_task` discards
/// `StreamResponse::Message` events, so this function is the only
/// path that preserves the agent's text. It mirrors the
/// `agent_event_to_stream_responses` mapping 1:1 so the streamed
/// and the persisted transcripts stay in sync.
fn record_event_into_history(
    history: &mut TaskHistoryBuilder,
    event: &synthia_agent::AgentEvent,
) {
    use synthia_agent::{AgentEvent, SystemEvent};
    use synthia_provider::ContentPart;

    match event {
        AgentEvent::Model(ContentPart::Text(text)) => {
            history.record_text_delta(&text.text);
        }
        AgentEvent::Model(ContentPart::ToolUse(tool_use)) => {
            history.record_tool_use(tool_use);
        }
        AgentEvent::Model(ContentPart::ToolResult(tool_result)) => {
            history.record_tool_result(tool_result);
        }
        AgentEvent::Model(ContentPart::Resource(link)) => {
            // ResourceLink is a tangible deliverable — mirror
            // the live stream's `ContentPart::Resource` arm by
            // appending to the artifact accumulator. Nothing
            // goes onto `history` for this variant; per A2A
            // v1.0 §3.7 the resource pointer rides on
            // `Task.artifacts`.
            history.record_resource_link(link);
        }
        AgentEvent::Model(ContentPart::Image(image)) => {
            history.record_image(image);
        }
        AgentEvent::Model(ContentPart::Audio(audio)) => {
            history.record_audio(audio);
        }
        AgentEvent::Model(ContentPart::Reasoning(_)) => {
            // Ephemeral provider hint, not user-visible.
        }
        AgentEvent::System(SystemEvent::SessionStarted { .. }) => {
            // Session lifecycle events other than the start are
            // not chat content; they're recorded via the terminal
            // event handling instead.
        }
        AgentEvent::System(_) => {
            // Session lifecycle events other than the start are not
            // chat content; they're recorded via the terminal
            // event handling instead.
        }
        AgentEvent::Agent(_, _) => {
            // Sub-agent traces are intentionally not recorded as
            // separate history messages — they are diagnostic only
            // and would otherwise duplicate the parent session's
            // transcript.
        }
        AgentEvent::ModelDone(result) => {
            history.record_model_done(result);
        }
    }
}

/// Classify an `AgentEvent` for log output. Distinct variants carry
/// distinct diagnostic value (a `ToolUse` model part means the LLM
/// asked for a tool; a `Text` model part is a text chunk;
/// `SessionEnded` is terminal).
fn event_kind(event: &AgentEvent) -> &'static str {
    use synthia_agent::SystemEvent;
    use synthia_provider::ContentPart;
    match event {
        AgentEvent::Model(ContentPart::Text(_)) => "Message",
        AgentEvent::Model(ContentPart::ToolUse(_)) => "ToolCall",
        AgentEvent::Model(ContentPart::ToolResult(_)) => "ToolResult",
        AgentEvent::Model(_) => "Model",
        AgentEvent::ModelDone(_) => "ModelDone",
        AgentEvent::System(SystemEvent::SessionStarted { .. }) => {
            "SessionStarted"
        }
        AgentEvent::System(SystemEvent::SessionEnded { .. }) => "SessionEnded",
        AgentEvent::System(_) => "System",
        AgentEvent::Agent(_, _) => "Agent",
    }
}

/// A2A user_id — 固定标识 A2A 协议发起的请求。
const A2A_USER_ID: &str = "a2a";

/// 实现 A2A `AgentExecutor` trait，桥接 Synthia SessionController。
pub struct SynthiaExecutor {
    state: Arc<AppState>,
    /// Shared task store. The executor maintains a per-task
    /// [`TaskHistoryBuilder`] during a run and writes the
    /// accumulated `Vec<Message>` back to `task.history` once the
    /// session terminates. A2A's wire-level protocol does not
    /// carry the model text inside `Task.history` (only
    /// `Message`s emitted by the executor end up persisted, and
    /// `apply_event_to_task` discards them), so this is the
    /// minimum-invasive way to give the task detail endpoint
    /// the full session transcript without abusing
    /// [`a2a::Artifact`] for chat content.
    task_store: Arc<dyn TaskStore>,
}

impl SynthiaExecutor {
    pub fn new(state: Arc<AppState>, task_store: Arc<dyn TaskStore>) -> Self {
        Self { state, task_store }
    }
}

#[async_trait::async_trait]
impl AgentExecutor for SynthiaExecutor {
    fn execute(
        &self,
        ctx: ExecutorContext,
    ) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        let state = self.state.clone();
        let task_store = self.task_store.clone();
        let task_id = ctx.task_id.clone();
        let context_id = ctx.context_id.clone();

        // 从 A2A Message 提取文本 prompt
        let prompt_text = ctx
            .message
            .as_ref()
            .and_then(extract_text_from_message)
            .unwrap_or_default();

        // ── Lifecycle: start ──────────────────────────────────────────
        // Logged at the entry of `execute` so an external `:subscribe`
        // request can be cross-referenced by `task_id` against the
        // stream that produced it. The `message:send` HTTP request
        // itself is logged upstream by `RequestTracingLayer` (with
        // `trace_id` / `span_id` already populated by the
        // `trace_context` middleware); these executor logs add the
        // task-level timeline that the HTTP layer cannot see.
        if prompt_text.is_empty() {
            info!(
                task_id = %task_id,
                context_id = %context_id,
                prompt_preview = "",
                "a2a.execute.start",
            );
        } else {
            info!(
                task_id = %task_id,
                context_id = %context_id,
                prompt_preview = %preview(&prompt_text, 120),
                "a2a.execute.start",
            );
        }

        if prompt_text.is_empty() {
            // A2A protocol contract: client validation errors are
            // surfaced as `A2AError::InvalidRequest`, not as a
            // Failed task. Returning a Failed task here would
            // (a) create a stored task for an input that never
            // produced a real run, and (b) hide the validation
            // failure behind the streaming API contract.
            return stream::once(async move {
                warn!(
                    task_id = %task_id,
                    "a2a.execute.empty_prompt",
                );
                Err(A2AError::invalid_request(
                    "message contained no text parts",
                ))
            })
            .boxed();
        }

        // Map A2A `context_id` (not `task_id`) to Synthia
        // `session_id`. Per the A2A v1.0 protocol (`a2a.proto:254-259`):
        // `context_id` is the "contextual collection of interactions
        // (tasks and messages)" — i.e. an ongoing conversation —
        // while `task_id` is a per-`message:send` invocation. Two
        // consecutive user messages in the same chat MUST share a
        // `context_id` (frontend passes it through as the chat
        // session id) but each one creates its own A2A `task_id`.
        //
        // Using `task_id` here previously caused every user
        // message to land in a *new* `SessionController`, so
        // consecutive questions looked like separate sessions
        // to the UI even though they shared a `context_id`. The
        // Synthia session state (messages, tool calls, in-flight
        // pending blocks) is what the chat UI actually cares
        // about, and that belongs to the context, not the task.
        //
        // Falls back to `task_id` only when `context_id` is empty
        // — that's the very first message of a brand-new chat
        // where the client hasn't yet established a context.
        // Using `task_id` keeps that case functional (each first
        // message is its own session) without poisoning the
        // follow-up-message path.
        let session_id = session_id_for_task(&task_id, &context_id);

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
            debug!(
                task_id = %task_id,
                "a2a.execute.yield_initial",
            );
            yield StreamResponse::Task(initial_task);

            // ─── 1.5. Initialize the per-task history builder ──────────
            // The builder accumulates `Vec<Message>` representing
            // the full conversation (user prompt + agent text +
            // tool calls + tool results). On terminal, we
            // attach it to `task.history` via `task_store.update`
            // so the task detail endpoint returns a complete
            // transcript instead of just the user message that
            // `prepare_task_for_execution` seeded.
            let mut history = TaskHistoryBuilder::new();
            history.record_user_prompt(&prompt_text);

            // ─── 2. 获取或创建 SessionController ────────────────────────
            let controller = state
                .get_or_create_session_controller(A2A_USER_ID, &session_id)
                .await
                .map_err(|e| {
                    warn!(
                        task_id = %task_id,
                        error = ?e,
                        "a2a.execute.session_create_failed",
                    );
                    A2AError::internal(format!("failed to create session: {e:?}"))
                })?;

            // ─── 3. 提交 prompt ──────────────────────────────────────────
            controller
                .submit(crate::session::controller::SessionOp::Prompt {
                    content: prompt_text,
                    priority: 1,
                })
                .await
                .map_err(|e| {
                    warn!(
                        task_id = %task_id,
                        error = ?e,
                        "a2a.execute.submit_failed",
                    );
                    A2AError::internal(format!("failed to submit prompt: {e:?}"))
                })?;
            debug!(
                task_id = %task_id,
                "a2a.execute.prompt_submitted",
            );

            // ─── 4. 订阅事件流,映射为 A2A StreamResponse ───────────────
            let mut rx = controller.subscribe();
            let mut event_count: u32 = 0;

            // Heartbeat ticker — keeps the SSE connection emitting bytes
            // during long quiet phases (LLM thinking, long-running tools)
            // so intermediaries don't drop the connection.
            let mut heartbeat = interval(HEARTBEAT_INTERVAL);
            // First tick fires immediately; skip it so the very first
            // heartbeat lands at +15s rather than at t=0.
            heartbeat.tick().await;
            let mut last_event_at = Instant::now();

            loop {
                tokio::select! {
                    biased;
                    event = rx.recv() => {
                        let event = match event {
                            Ok(ev) => ev,
                            Err(RecvError::Closed) => {
                                // Producer side dropped the
                                // sender — exit cleanly.
                                break;
                            }
                            Err(RecvError::Lagged(skipped)) => {
                                // The receiver fell behind and
                                // `EVENT_CHANNEL_CAPACITY`
                                // messages were dropped before
                                // we caught up. Stay connected,
                                // but emit a warning so the
                                // operator can correlate the
                                // gap with downstream
                                // truncation.
                                warn!(
                                    task_id = %task_id,
                                    skipped = skipped,
                                    "A2A event receiver lagged; \
                                     some upstream events were \
                                     dropped before this stream \
                                     caught up"
                                );
                                continue;
                            }
                        };
                        last_event_at = Instant::now();
                        event_count += 1;
                        let kind = event_kind(&event);
                        let terminal = is_terminal_event(&event);
                        debug!(
                            task_id = %task_id,
                            event_index = event_count,
                            event_kind = kind,
                            terminal = terminal,
                            "a2a.execute.event",
                        );

                        // Record into the per-task history builder
                        // BEFORE we forward the event to the A2A
                        // stream. The builder's job is to keep
                        // `Task.history` complete; the upstream
                        // `apply_event_to_task` will drop the
                        // `Message` events we yield next, so this
                        // is the only path that preserves them.
                        record_event_into_history(&mut history, &event);

                        // If the upstream `drive_execution` sees
                        // our next yield as terminal, it will
                        // drop our stream before we get a chance
                        // to write the history back. To make the
                        // history survive, we yield the terminal
                        // event as a `StreamResponse::Task`
                        // carrying the full task (including
                        // history) — upstream's
                        // `apply_event_to_task` on a Task simply
                        // saves the whole task via `save_task`,
                        // which persists our history in the same
                        // store write. The alternative (yield
                        // StatusUpdate + write history before
                        // the yield) loses the history because
                        // upstream's `current_task` is a stale
                        // snapshot that gets saved over our
                        // write.
                        if terminal {
                            let (messages, artifacts) = history.take_transcript();
                            let final_task = build_terminal_task(
                                &task_store,
                                &task_id,
                                &context_id,
                                &event,
                                messages,
                                artifacts,
                            )
                            .await;
                            yield StreamResponse::Task(final_task);
                            info!(
                                task_id = %task_id,
                                event_index = event_count,
                                "a2a.execute.end",
                            );
                            break;
                        }

                        for resp in agent_event_to_stream_responses(
                            &event,
                            event_count,
                            &task_id,
                            &context_id,
                        ) {
                            yield resp?;
                        }
                    }
                    _ = heartbeat.tick() => {
                        // Skip a heartbeat if we just emitted real traffic
                        // (within one interval window) — there is no
                        // quiet gap to bridge.
                        if !should_emit_heartbeat(last_event_at, Instant::now()) {
                            continue;
                        }
                        debug!(
                            task_id = %task_id,
                            "a2a.execute.heartbeat",
                        );
                        // Heartbeat is a transport-level
                        // keep-alive, not an agent event. Emit a
                        // plain Working StatusUpdate directly
                        // — bypassing `agent_event_to_stream_responses`
                        // (which would otherwise turn it into a
                        // fake `AgentEvent::ToolProgress` first,
                        // which is exactly the kind of semantic
                        // borrow we're trying to avoid).
                        let resp = working_status_update(&task_id, &context_id);
                        yield StreamResponse::StatusUpdate(resp);
                    }
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

        info!(
            task_id = %task_id,
            context_id = %context_id,
            "a2a.cancel.start",
        );

        Box::pin(async_stream::stream! {
            // 尝试取消 session
            let cancel_result = match state
                .get_or_create_session_controller(A2A_USER_ID, &task_id)
                .await
            {
                Ok(controller) => controller.cancel().await,
                Err(_) => Ok(()), // session didn't exist — treat as already-cancelled
            };

            if let Err(e) = cancel_result {
                warn!(
                    task_id = %task_id,
                    error = ?e,
                    "a2a.cancel.failed",
                );
                yield Err(A2AError::internal(format!(
                    "failed to cancel session: {e:?}"
                )));
                return;
            }

            debug!(
                task_id = %task_id,
                "a2a.cancel.complete",
            );
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
/// 仅 [`AgentEvent::System`] 携带 [`SystemEvent::SessionEnded`] 时
/// 才是真正的终止事件 — 它之后代理流程会停止并清理资源。
fn is_terminal_event(event: &synthia_agent::AgentEvent) -> bool {
    matches!(
        event,
        synthia_agent::AgentEvent::System(
            synthia_agent::SystemEvent::SessionEnded { .. }
        )
    )
}

/// Build the terminal [`Task`] snapshot to yield on `SessionEnded`.
///
/// The whole point of yielding a `StreamResponse::Task` (rather than
/// a `StatusUpdate`) on terminal is that upstream's
/// [`a2a_server::handler::apply_event_to_task`] saves the entire
/// task via `save_task` for the `Task` arm — including whatever
/// `history` we embed here. A `StatusUpdate` would only persist
/// `status` and would clobber any prior `history` because upstream
/// caches `current_task` in memory and re-saves it without
/// re-reading from the store.
///
/// We therefore:
/// 1. Read the current task from the store (covers the case where
///    earlier StatusUpdates already advanced `status.state`).
/// 2. Replace `history` with the [`TaskHistoryBuilder`] output.
/// 3. Set `status` to the A2A mapping of the terminal reason.
/// 4. Yield the resulting task — upstream's `save_task` writes it
///    whole, and our `history` survives the round-trip.
///
/// On any read failure we fall back to a freshly-constructed
/// terminal task so the executor still terminates cleanly.
async fn build_terminal_task(
    task_store: &Arc<dyn a2a_server::TaskStore>,
    task_id: &str,
    context_id: &str,
    event: &synthia_agent::AgentEvent,
    messages: Vec<a2a::Message>,
    artifacts: Option<Vec<a2a::Artifact>>,
) -> a2a::Task {
    use synthia_agent::{SystemEvent, events::SessionEndReason};

    let terminal_state = match event {
        synthia_agent::AgentEvent::System(SystemEvent::SessionEnded {
            reason,
        }) => match reason {
            SessionEndReason::Completed => a2a::TaskState::Completed,
            SessionEndReason::Cancelled => a2a::TaskState::Canceled,
            SessionEndReason::Error(_) | SessionEndReason::MaxIterations => {
                a2a::TaskState::Failed
            }
        },
        _ => a2a::TaskState::Failed,
    };

    let mut final_task = match task_store.get(task_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            warn!(
                task_id = %task_id,
                "a2a.execute.build_terminal_task: task not found; \
                 synthesising a fresh terminal task"
            );
            task_with_state(
                task_id.to_string(),
                context_id.to_string(),
                terminal_state.clone(),
                None,
            )
        }
        Err(e) => {
            warn!(
                task_id = %task_id,
                error = %e,
                "a2a.execute.build_terminal_task: store read failed; \
                 synthesising a fresh terminal task"
            );
            task_with_state(
                task_id.to_string(),
                context_id.to_string(),
                terminal_state.clone(),
                None,
            )
        }
    };

    final_task.history = if messages.is_empty() {
        None
    } else {
        Some(messages)
    };
    final_task.artifacts = artifacts;
    final_task.status.state = terminal_state;
    final_task
}

#[cfg(test)]
mod tests {
    use synthia_agent::{
        AgentEvent,
        SessionEndReason,
        SystemEvent,
        events::AgentMeta,
    };

    use super::*;

    #[test]
    fn is_terminal_event_detects_session_ended() {
        let event = synthia_agent::AgentEvent::System(
            synthia_agent::SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            },
        );
        assert!(is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_does_not_detect_message() {
        // Message 事件代表代理输出的文本片段,并不代表 A2A 任务结束。
        // 后续代理仍可能继续运行 (反思 / 自动压缩等) 并最终发出
        // SessionEnded,因此 Message 不应让执行器提前断开
        // broadcast::Receiver。
        let event = synthia_agent::AgentEvent::Model(
            synthia_provider::ContentPart::Text(
                synthia_provider::TextContent {
                    text: "done".to_string(),
                    cache_control: None,
                },
            ),
        );
        assert!(!is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_rejects_non_terminal() {
        let event = synthia_agent::AgentEvent::Model(
            synthia_provider::ContentPart::Text(
                synthia_provider::TextContent {
                    text: "hi".to_string(),
                    cache_control: None,
                },
            ),
        );
        assert!(!is_terminal_event(&event));
    }

    #[test]
    fn is_terminal_event_does_not_descend_into_agent_wrapper() {
        // Sub-agent traces are wrapped as
        // `AgentEvent::Agent(meta, Box<AgentEvent>)`. The A2A
        // executor treats the outer wrapper as opaque — even
        // when the inner event is `SessionEnded`, the *outer*
        // event is a sub-agent trace, not the parent session's
        // terminal state. This pins down the MVP non-recursive
        // contract documented at `a2a/mapping.rs` and prevents
        // future "helpful" refactors from closing the
        // broadcast channel prematurely when a peer-agent
        // trace happens to wrap a terminal inner event.
        let inner = AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        });
        let event = AgentEvent::Agent(
            AgentMeta::new("root", "judge", 1),
            Box::new(inner),
        );
        assert!(
            !is_terminal_event(&event),
            "delegated-agent SessionEnded must not be treated as the parent session's terminal"
        );
    }

    #[test]
    fn event_kind_classifies_top_level_variants() {
        assert_eq!(
            event_kind(&AgentEvent::Model(
                synthia_provider::ContentPart::Text(
                    synthia_provider::TextContent {
                        text: "x".into(),
                        cache_control: None,
                    },
                )
            )),
            "Message"
        );
        assert_eq!(
            event_kind(&AgentEvent::System(SystemEvent::SessionStarted {
                session_id: "s".into(),
            })),
            "SessionStarted"
        );
        assert_eq!(
            event_kind(&AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            })),
            "SessionEnded"
        );
    }

    #[test]
    fn event_kind_does_not_descend_into_agent_wrapper() {
        // The classifier intentionally treats every
        // `Agent(meta, inner)` as `"Agent"` regardless of what
        // the inner event is — logs don't need delegated-agent
        // granularity.
        let inner = AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        });
        let wrapped = AgentEvent::Agent(
            AgentMeta::new("root", "judge", 1),
            Box::new(inner),
        );
        assert_eq!(event_kind(&wrapped), "Agent");
    }

    #[test]
    fn invalid_request_error_has_jsonrpc_code_minus_32600() {
        // The empty-prompt path uses `A2AError::invalid_request`,
        // which is mapped to JSON-RPC InvalidRequest (code -32600).
        // Asserting on the JSON-RPC code keeps the test stable even
        // if upstream changes the `A2AError` Display impl.
        let err = A2AError::invalid_request("message contained no text parts");
        let jsonrpc_err = err.to_jsonrpc_error();
        assert_eq!(
            jsonrpc_err.code, -32600,
            "invalid_request must map to JSON-RPC -32600, got {}",
            jsonrpc_err.code
        );
    }

    /// Direct unit tests for [`should_emit_heartbeat`].
    /// The streaming integration path is exercised end-to-end
    /// by `a2a::execute_*` tests, but pinning the skip
    /// decision independently locks down the contract:
    /// "skip when a real event happened strictly less than
    /// one HEARTBEAT_INTERVAL ago".
    mod heartbeat_skip_tests {
        use std::time::Duration;

        use tokio::time::Instant;

        use super::{HEARTBEAT_INTERVAL, should_emit_heartbeat};

        #[test]
        fn emit_when_no_event_yet_and_quiet_for_full_interval() {
            // `last_event_at == now` (zero elapsed) MUST
            // still emit when `now - last_event_at >=
            // HEARTBEAT_INTERVAL`. The very first heartbeat
            // lands at +15s relative to t=0.
            let t0 = Instant::now();
            assert!(
                should_emit_heartbeat(t0, t0 + HEARTBEAT_INTERVAL),
                "heartbeat must fire after one full interval of silence"
            );
        }

        #[test]
        fn skip_when_recent_event_within_one_interval() {
            // If a real event arrived 5s ago and the
            // heartbeat tick fires now (10s after the
            // event), we MUST skip — there is no quiet
            // gap to bridge and the next event is
            // imminent.
            let last = Instant::now();
            let now = last + Duration::from_secs(5);
            assert!(
                !should_emit_heartbeat(last, now),
                "heartbeat must skip when a real event happened 5s ago and HEARTBEAT_INTERVAL is 15s"
            );
        }

        #[test]
        fn emit_when_silence_just_crossed_the_threshold() {
            // Boundary: exactly one interval of silence
            // since the last event. Decision is `>=`, so we
            // MUST emit (not skip).
            let last = Instant::now();
            let now = last + HEARTBEAT_INTERVAL;
            assert!(
                should_emit_heartbeat(last, now),
                "heartbeat must fire at the exact interval boundary (>= not >)"
            );
        }

        #[test]
        fn skip_when_recent_event_at_sub_second_interval() {
            // Tight loop: an event at t and heartbeat
            // tick at t+1ms — must skip.
            let last = Instant::now();
            let now = last + Duration::from_millis(1);
            assert!(
                !should_emit_heartbeat(last, now),
                "heartbeat must skip when an event landed 1ms ago"
            );
        }
    }

    /// `preview` truncates a string for log output,
    /// appending a Unicode ellipsis (`…`) when the
    /// string exceeds `max` chars. Char-count
    /// (not byte-count) is the contract so the
    /// function behaves correctly for multi-byte
    /// text (e.g. 中文 / 日本語).
    ///
    /// No test pins this today; a refactor that
    /// switched to `s.len()` (byte-length) would
    /// truncate non-ASCII user prompts in the
    /// middle of a multi-byte codepoint.
    mod preview_tests {
        use super::preview;

        #[test]
        fn preview_shorter_than_max_is_returned_verbatim() {
            assert_eq!(preview("hi", 5), "hi");
            assert_eq!(preview("hello", 5), "hello");
        }

        #[test]
        fn preview_equal_to_max_is_returned_verbatim() {
            // The boundary case — exactly `max` chars
            // means NO truncation.
            assert_eq!(preview("hello", 5), "hello");
        }

        #[test]
        fn preview_one_over_max_is_truncated_with_ellipsis() {
            // 6 chars, max=5 → first 5 + "…"
            assert_eq!(preview("hello!", 5), "hello…");
        }

        #[test]
        fn preview_empty_string_is_returned_verbatim() {
            // chars().count() == 0 == max (if max=0)
            // is a degenerate case — verify no panic
            // for empty input.
            assert_eq!(preview("", 0), "");
            assert_eq!(preview("", 5), "");
        }

        #[test]
        fn preview_max_zero_with_nonempty_returns_ellipsis_only() {
            // chars().count() == 1 > 0, so the
            // truncation branch runs. `take(0)`
            // produces an empty string, then we
            // append "…". Pin this so a refactor
            // doesn't accidentally swallow the
            // ellipsis on zero-width preview.
            assert_eq!(preview("x", 0), "…");
        }

        #[test]
        fn preview_counts_chars_not_bytes_for_multibyte_text() {
            // "中文" is 2 chars but 6 bytes (UTF-8).
            // With max=2 we MUST return "中文…"
            // rather than byte-truncated garbage.
            assert_eq!(preview("中文", 2), "中文");
            assert_eq!(preview("中文!", 2), "中文…");
            assert_eq!(preview("日本語テスト", 3), "日本語…");
        }
    }

    /// Verifies the A2A `(task_id, context_id)` →
    /// Synthia `session_id` mapping rule. This is the fix that
    /// keeps consecutive user messages in the same chat
    /// landing on the SAME `SessionController` — previously
    /// each `message:send` minted a new `task_id`, and the
    /// executor used `task_id` as the session id, so the chat
    /// UI saw every user message as a fresh session.
    mod session_id_for_task {
        use super::super::session_id_for_task;

        /// `context_id` non-empty → bound to the context, NOT
        /// the task. Two consecutive messages with the same
        /// `context_id` (and different `task_id`s, which is
        /// what A2A produces by default) MUST yield the same
        /// session id. This is the core invariant.
        #[test]
        fn context_id_wins_over_task_id_for_multi_turn_chat() {
            // First user message of a chat: server mints
            // task-A, frontend forwards contextId from URL.
            let session1 = session_id_for_task("task-A", "chat-uuid-1");
            // Second user message of the same chat: server
            // mints task-B (new A2A task per message:send),
            // frontend forwards the SAME contextId.
            let session2 = session_id_for_task("task-B", "chat-uuid-1");

            assert_eq!(session1, "chat-uuid-1");
            assert_eq!(session2, "chat-uuid-1");
            assert_eq!(
                session1, session2,
                "two messages sharing context_id MUST route to the same session"
            );
        }

        /// Empty `context_id` (very first message of a brand-
        /// new chat, before the client has established a
        /// context) MUST fall back to `task_id`. A2A proto
        /// explicitly permits this: "If only `task_id` is
        /// provided, the server will infer `context_id` from
        /// it."
        #[test]
        fn empty_context_id_falls_back_to_task_id() {
            assert_eq!(
                session_id_for_task("task-only", ""),
                "task-only",
                "no context → first message uses its task_id as session seed"
            );
        }

        /// Non-ASCII `context_id` round-trips intact. The
        /// `String::to_string` path inside the helper is a
        /// pure UTF-8 copy; this pins the behavior so a
        /// refactor to a `Cow` or `Arc<str>` doesn't lose
        /// characters on the way through.
        #[test]
        fn context_id_with_unicode_round_trips() {
            assert_eq!(
                session_id_for_task("task-忽略", "会话-🚀-uuid"),
                "会话-🚀-uuid"
            );
        }
    }
}
