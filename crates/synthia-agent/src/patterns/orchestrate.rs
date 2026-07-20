//! Orchestrator — 把 agents 包成 tools 注入 registry，LLM 自己选调谁。

use std::sync::Arc;

use synthia_tool::{ToolEntry, registry::ToolRegistry, traits::Tool};

use crate::{a2t::agent_as_tool, handle::AgentHandle};

/// 把多个 AgentHandle 包成 tool 注入 ToolRegistry。
///
/// LLM 在 run 时看到这些 tools，自己决定调谁。
/// 这是 Orchestrator 模式的实现——不是新概念，是 agent_as_tool() 的自然推论。
pub fn orchestrate(
    agents: Vec<Arc<AgentHandle>>,
    into_registry: &ToolRegistry,
) {
    for agent in agents {
        let tool = agent_as_tool(agent);
        let tool_name = tool.name().to_string();
        let entry = ToolEntry::new(Arc::new(tool) as Arc<dyn Tool>);
        into_registry.register(entry);
        tracing::debug!(agent_id = %tool_name, "Registered agent as tool in registry");
    }
}

/// 把远程 agent URL 包成 SendMessageTool 注入 ToolRegistry。
///
/// 远程 agent 通过 A2A 协议通信，不使用本地 agent_as_tool()。
/// 需要 synthia-a2a crate 的支持，当前为占位签名。
pub fn orchestrate_remote(
    _remote_urls: &[&str],
    _into_registry: &ToolRegistry,
) {
    // TODO: Phase 2 — 需要 synthia-a2a 的 SendMessageTool
    tracing::info!(
        "orchestrate_remote: placeholder, requires synthia-a2a integration"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orchestrate_empty() {
        let registry = ToolRegistry::new();
        orchestrate(vec![], &registry);
    }
}
