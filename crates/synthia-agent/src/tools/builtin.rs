//! Builtin tools registration
//!
//! This module provides organized registration of all builtin tools,
//! grouped by functionality with clear concurrency safety categorization.

use std::sync::Arc;

use crate::{
    shell::LocalShellExecutor,
    tools::{
        Tool,
        ToolRegistry,
        exec::ExecTool,
        fs,
        search,
        send_user_message,
        thinking,
        todo,
        tom,
        web,
    },
};

type ToolFactory = fn() -> Arc<dyn Tool>;

const READ_ONLY_TOOLS: &[ToolFactory] = &[
    || Arc::new(fs::ReadTool::new()) as Arc<dyn Tool>,
    || Arc::new(fs::GrepTool::new()),
    || Arc::new(fs::GlobTool::new()),
    || Arc::new(fs::ListDirectoryTool::new()),
    || Arc::new(fs::DirectoryTreeTool::new()),
    || Arc::new(web::WebSearchTool::new()),
    || Arc::new(web::WebFetchTool::new()),
    || Arc::new(send_user_message::SendUserMessageTool::new()),
];

const WRITE_TOOLS: &[ToolFactory] = &[
    || Arc::new(fs::EditTool::new()),
    || Arc::new(fs::WriteTool::new()),
    || Arc::new(fs::CreateDirectoryTool::new()),
    || Arc::new(fs::DeleteTool::new()),
    || Arc::new(fs::MoveFileTool::new()),
    || Arc::new(ExecTool::new(Arc::new(LocalShellExecutor::new()))),
];

const OTHER_TOOLS: &[ToolFactory] = &[
    || Arc::new(todo::TodoWriteTool::new()),
    || Arc::new(thinking::SequentialThinkingTool::new_with_stdout()),
    || Arc::new(tom::ContextInjectTool::new()),
];

pub async fn register_builtin_tools(registry: &Arc<ToolRegistry>) {
    let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

    for make_tool in READ_ONLY_TOOLS {
        tools.push(make_tool());
    }

    for make_tool in WRITE_TOOLS {
        tools.push(make_tool());
    }

    for make_tool in OTHER_TOOLS {
        tools.push(make_tool());
    }

    // ToolSearchTool needs the actual registry, so we create it here
    tools.push(Arc::new(search::ToolSearchTool::new(
        std::sync::Arc::downgrade(registry),
    )));

    registry.registers(tools.into_iter()).await;
}
