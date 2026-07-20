//! SynthiaA2aHandler — A2A 请求桥接到 AgentHandle。
//!
//! 把 A2A 协议的 on_send_message / on_send_streaming_message
//! 桥接到 AgentHandle::run / AgentHandle::run_stream。

use std::sync::Arc;

use synthia_agent::AgentHandle;

use crate::transport::AgentCard;

/// A2A Handler — 把 A2A 请求桥接到 AgentHandle。
///
/// 当其他 agent 通过 A2A 协议调用此 agent 时，
/// SynthiaA2aHandler 将请求桥接到 AgentHandle 的 run/run_stream。
pub struct SynthiaA2aHandler {
    /// 底层 AgentHandle。
    handle: Arc<AgentHandle>,
    /// 此 agent 的 AgentCard。
    card: AgentCard,
}

impl SynthiaA2aHandler {
    /// 创建新的 SynthiaA2aHandler。
    pub fn new(handle: Arc<AgentHandle>, card: AgentCard) -> Self {
        Self { handle, card }
    }

    /// 获取 AgentCard。
    pub fn card(&self) -> &AgentCard {
        &self.card
    }

    /// 获取底层 AgentHandle。
    pub fn handle(&self) -> &AgentHandle {
        &self.handle
    }

    /// 处理 A2A send_message 请求 — 桥接到 AgentHandle::run。
    ///
    /// 当前返回占位结果。完整实现需要 A2A Task 状态管理。
    pub async fn on_send_message(&self, prompt: &str) -> String {
        // TODO: Phase 2 — 实际调用 handle.run(session, prompt)
        // 当前返回占位结果
        tracing::info!(
            agent_id = %self.handle.id,
            prompt_len = prompt.len(),
            "A2A send_message received"
        );
        format!("[A2A] agent {} received: {prompt}", self.handle.id)
    }

    /// 处理 A2A send_streaming_message 请求 — 桥接到 AgentHandle::run_stream。
    ///
    /// 当前返回占位结果。完整实现需要 A2A StreamEvent 流。
    pub async fn on_send_streaming_message(&self, prompt: &str) -> String {
        // TODO: Phase 2 — 实际调用 handle.run_stream(session, prompt)
        // 当前返回占位结果
        tracing::info!(
            agent_id = %self.handle.id,
            prompt_len = prompt.len(),
            "A2A send_streaming_message received"
        );
        format!("[A2A stream] agent {} received: {prompt}", self.handle.id)
    }
}

/// 将 AgentOutput 文本转换为 A2A 流式事件序列。
///
/// Phase 2 实现将接收 `AgentOutputStream`（`Pin<Box<dyn Stream<Item = AgentEvent>>>`）
/// 并将每个 `AgentEvent` 映射为 A2A `StreamEvent`：
/// - `AgentEvent::LlmText` → `StreamEvent::TaskStatusUpdate`
/// - `AgentEvent::ToolResult` → `StreamEvent::ArtifactUpdate`
/// - `AgentEvent::SessionEnd` → `StreamEvent::Completed`
///
/// 当前返回简单的单事件占位流。
pub fn agent_output_to_a2a_stream(text: String) -> Vec<String> {
    // Phase 2: return Pin<Box<dyn Stream<Item = Result<StreamEvent>>>>
    // 当前返回简单的文本分块
    vec![text]
}

#[cfg(test)]
mod tests {
    // 完整测试需要真实 AgentHandle，在集成测试中补充。
    // 当前只验证 struct 构造逻辑。
    #[test]
    fn handler_card_accessor() {
        // 占位 — 需要真实 AgentHandle 才能构造 SynthiaA2aHandler
    }
}
