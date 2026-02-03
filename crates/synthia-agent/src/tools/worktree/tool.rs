//! Worktree tools implementation

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use super::WorktreeManager;
use crate::tools::Tool;

#[derive(Debug, Deserialize)]
struct WorktreeCreateRequest {
    name: String,
    #[serde(rename = "taskId")]
    task_id: Option<i64>,
    #[serde(rename = "baseRef")]
    base_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorktreeRunRequest {
    name: String,
    command: String,
}

#[derive(Debug, Deserialize)]
struct WorktreeRemoveRequest {
    name: String,
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize)]
struct WorktreeEventsRequest {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug)]
pub(crate) struct WorktreeCreateTool {
    manager: Arc<WorktreeManager>,
}

impl WorktreeCreateTool {
    pub(crate) fn new(repo_root: PathBuf) -> Self {
        Self {
            manager: Arc::new(WorktreeManager::new(repo_root)),
        }
    }
}

#[async_trait]
impl Tool for WorktreeCreateTool {
    fn name(&self) -> &str {
        "worktree_create"
    }

    fn description(&self) -> &str {
        "Create git worktree for isolated execution."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Worktree name"
                },
                "taskId": {
                    "type": "integer",
                    "description": "Task ID"
                },
                "baseRef": {
                    "type": "string",
                    "description": "Git ref",
                    "default": "HEAD"
                }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: WorktreeCreateRequest = match serde_json::from_value(args)
        {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let base_ref = request.base_ref.unwrap_or_else(|| "HEAD".to_string());

        match self
            .manager
            .create(&request.name, request.task_id, &base_ref)
        {
            Ok(entry) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&entry).unwrap_or_default(),
            )]),
            Err(e) => CallToolResult::error(vec![Content::text(e)]),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WorktreeListTool {
    manager: Arc<WorktreeManager>,
}

impl WorktreeListTool {
    pub(crate) fn new(repo_root: PathBuf) -> Self {
        Self {
            manager: Arc::new(WorktreeManager::new(repo_root)),
        }
    }
}

#[async_trait]
impl Tool for WorktreeListTool {
    fn name(&self) -> &str {
        "worktree_list"
    }

    fn description(&self) -> &str {
        "List all worktrees."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        let worktrees = self.manager.list();

        if worktrees.is_empty() {
            return CallToolResult::success(vec![Content::text(
                "No worktrees.",
            )]);
        }

        let lines: Vec<String> = worktrees
            .iter()
            .map(|wt| {
                let task_suffix = wt
                    .task_id
                    .map(|id| format!(" task={id}"))
                    .unwrap_or_default();
                format!(
                    "[{}] {} -> {} ({}){}",
                    wt.status, wt.name, wt.path, wt.branch, task_suffix
                )
            })
            .collect();

        CallToolResult::success(vec![Content::text(lines.join("\n"))])
    }
}

#[derive(Debug)]
pub(crate) struct WorktreeStatusTool {
    manager: Arc<WorktreeManager>,
}

impl WorktreeStatusTool {
    pub(crate) fn new(repo_root: PathBuf) -> Self {
        Self {
            manager: Arc::new(WorktreeManager::new(repo_root)),
        }
    }
}

#[async_trait]
impl Tool for WorktreeStatusTool {
    fn name(&self) -> &str {
        "worktree_status"
    }

    fn description(&self) -> &str {
        "Show git status for a worktree."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Worktree name"
                }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        #[derive(Deserialize)]
        struct Request {
            name: String,
        }

        let request: Request = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        match self.manager.status(&request.name) {
            Ok(status) => CallToolResult::success(vec![Content::text(status)]),
            Err(e) => CallToolResult::error(vec![Content::text(e)]),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WorktreeRunTool {
    manager: Arc<WorktreeManager>,
}

impl WorktreeRunTool {
    pub(crate) fn new(repo_root: PathBuf) -> Self {
        Self {
            manager: Arc::new(WorktreeManager::new(repo_root)),
        }
    }
}

#[async_trait]
impl Tool for WorktreeRunTool {
    fn name(&self) -> &str {
        "worktree_run"
    }

    fn description(&self) -> &str {
        "Run a shell command in a worktree directory."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Worktree name"
                },
                "command": {
                    "type": "string",
                    "description": "Command"
                }
            },
            "required": ["name", "command"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: WorktreeRunRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        match self.manager.run(&request.name, &request.command) {
            Ok(output) => CallToolResult::success(vec![Content::text(output)]),
            Err(e) => CallToolResult::error(vec![Content::text(e)]),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WorktreeRemoveTool {
    manager: Arc<WorktreeManager>,
}

impl WorktreeRemoveTool {
    pub(crate) fn new(repo_root: PathBuf) -> Self {
        Self {
            manager: Arc::new(WorktreeManager::new(repo_root)),
        }
    }
}

#[async_trait]
impl Tool for WorktreeRemoveTool {
    fn name(&self) -> &str {
        "worktree_remove"
    }

    fn description(&self) -> &str {
        "Remove a git worktree."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Worktree name"
                },
                "force": {
                    "type": "boolean",
                    "description": "Force removal",
                    "default": false
                }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: WorktreeRemoveRequest = match serde_json::from_value(args)
        {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        match self.manager.remove(&request.name, request.force) {
            Ok(msg) => CallToolResult::success(vec![Content::text(msg)]),
            Err(e) => CallToolResult::error(vec![Content::text(e)]),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WorktreeKeepTool {
    manager: Arc<WorktreeManager>,
}

impl WorktreeKeepTool {
    pub(crate) fn new(repo_root: PathBuf) -> Self {
        Self {
            manager: Arc::new(WorktreeManager::new(repo_root)),
        }
    }
}

#[async_trait]
impl Tool for WorktreeKeepTool {
    fn name(&self) -> &str {
        "worktree_keep"
    }

    fn description(&self) -> &str {
        "Mark a worktree as kept without removing it."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Worktree name"
                }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        #[derive(Deserialize)]
        struct Request {
            name: String,
        }

        let request: Request = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        match self.manager.keep(&request.name) {
            Ok(entry) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&entry).unwrap_or_default(),
            )]),
            Err(e) => CallToolResult::error(vec![Content::text(e)]),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WorktreeEventsTool {
    manager: Arc<WorktreeManager>,
}

impl WorktreeEventsTool {
    pub(crate) fn new(repo_root: PathBuf) -> Self {
        Self {
            manager: Arc::new(WorktreeManager::new(repo_root)),
        }
    }
}

#[async_trait]
impl Tool for WorktreeEventsTool {
    fn name(&self) -> &str {
        "worktree_events"
    }

    fn description(&self) -> &str {
        "List recent worktree events."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Number of events",
                    "default": 20
                }
            }
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: WorktreeEventsRequest = serde_json::from_value(args)
            .unwrap_or(WorktreeEventsRequest { limit: 20 });

        let events = self.manager.events(request.limit);
        CallToolResult::success(vec![Content::text(events)])
    }
}

pub async fn register_worktree_tools(
    registry: &crate::tools::ToolRegistry,
    repo_root: PathBuf,
) {
    use std::sync::Arc;

    use crate::tools::Tool;

    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(WorktreeListTool::new(repo_root.clone())),
        Arc::new(WorktreeStatusTool::new(repo_root.clone())),
        Arc::new(WorktreeCreateTool::new(repo_root.clone())),
        Arc::new(WorktreeRunTool::new(repo_root.clone())),
        Arc::new(WorktreeRemoveTool::new(repo_root.clone())),
        Arc::new(WorktreeKeepTool::new(repo_root.clone())),
        Arc::new(WorktreeEventsTool::new(repo_root)),
    ];

    registry.registers(tools.into_iter()).await;
}
