//! Transfer — 双向对称转移模式。

use crate::handle::AgentHandle;

/// 双向对称转移 — 双方互相注入 SendMessage/Stream Tool。
///
/// 与旧版 HandoffTool 的区别：
/// - Handoff 是单向消息（fire-and-forget）
/// - Transfer 是控制权转移（对方执行后可转回）
///
/// 需要 synthia-a2a crate 的支持。当前为占位实现。
pub fn transfer_bidirectional(agent_a: &mut AgentHandle, agent_b_url: &str) {
    // TODO: Phase 2 — 需要 synthia-a2a 的 SendMessageTool / SendMessageStreamTool
    // agent_a.tool_registry.register(SendMessageTool::for_url(agent_b_url, transport));
    // agent_a.tool_registry.register(SendMessageStreamTool::for_url(agent_b_url, transport));
    // agent_b 端由自己启动时配置对 a 的引用（对称）

    tracing::info!(
        agent_a_id = %agent_a.id,
        agent_b_url = %agent_b_url,
        "transfer_bidirectional: placeholder, requires synthia-a2a integration"
    );
}

#[cfg(test)]
mod tests {
    // 完整测试需要真实 AgentHandle + synthia-a2a，在集成测试中补充。
}
