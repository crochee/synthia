use std::sync::Arc;

use synthia_provider::traits::ModelProvider;
use synthia_tool::{ToolEntry, ToolRegistry};

use crate::{
    agent_tools::{
        AgentTool,
        SendMessageTool,
        SubagentManager,
        TeamCreateTool,
        TeamDeleteTool,
    },
    ask_user::AskUserQuestionTool,
    tools::{CompactContextTool, SelfReflectTool},
};

pub fn register_agent_tools(
    registry: &mut ToolRegistry,
    manager: Arc<SubagentManager>,
    provider: Arc<dyn ModelProvider>,
    model: String,
) {
    registry.register(ToolEntry::new(Arc::new(AgentTool::new(
        manager.clone(),
        true,
    ))));
    registry.register(ToolEntry::new(Arc::new(SendMessageTool::new(
        manager.clone(),
    ))));
    registry.register(ToolEntry::new(Arc::new(TeamCreateTool::new(
        manager.clone(),
    ))));
    registry.register(ToolEntry::new(Arc::new(TeamDeleteTool::new(manager))));
    registry.register(ToolEntry::new(Arc::new(AskUserQuestionTool)));
    registry.register(ToolEntry::new(Arc::new(SelfReflectTool::new(
        provider, model,
    ))));
    registry.register(ToolEntry::new(Arc::new(CompactContextTool)));
}
