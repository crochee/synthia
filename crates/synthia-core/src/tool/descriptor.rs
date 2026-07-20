//! ToolDescriptor + ToolProvenance + supporting enums.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::tool::{
    tool_name::ToolName,
    types::{ToolContext, ToolError, ToolInput, ToolOutput},
};

/// Unified Tool trait — 3 methods only.
///
/// Every LLM-invokable capability implements this trait.
/// The legacy 11-method `Tool` trait in `synthia-tool` is deprecated.
#[async_trait]
pub trait Tool: Send + Sync + 'static {
    /// Tool name (used as LLM function name).
    fn name(&self) -> &str;

    /// Execute the tool.
    async fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError>;

    /// Tool descriptor (cached, cheap to call).
    fn descriptor(&self) -> &ToolDescriptor;
}

/// Full tool metadata for LLM tool_choice and orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Tool name (may be namespaced: `plugin:<id>:<name>`).
    pub name: ToolName,
    /// Human-readable description for LLM.
    pub description: String,
    /// JSON Schema for tool parameters.
    pub parameters: serde_json::Value,
    /// Tool category.
    pub category: ToolCategory,
    /// Where this tool comes from.
    pub provenance: ToolProvenance,
    /// Execution mode (parallel/sequential).
    pub execution_mode: ExecutionMode,
    /// Cancellation behavior.
    pub cancel_behavior: CancelBehavior,
    /// Usage examples for LLM.
    #[serde(default)]
    pub examples: Vec<ToolExample>,
    /// Whether this tool requires permission.
    #[serde(default)]
    pub permission_required: bool,
    /// Whether provenance prefix is visible to LLM.
    #[serde(default = "default_true")]
    pub prompt_visible_provenance: bool,
    /// Whether this tool is hidden from /help listings.
    #[serde(default)]
    pub is_hidden: bool,
    /// Whether the LLM can invoke this tool directly.
    #[serde(default = "default_true")]
    pub is_user_invocable: bool,
    /// 工具曝光级别
    #[serde(default)]
    pub exposure: ToolExposure,
}

fn default_true() -> bool {
    true
}

/// Tool usage example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    pub description: String,
    pub input: serde_json::Value,
}

/// Where a tool comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolProvenance {
    /// Built-in tool (immutable name).
    Core,
    /// Plugin-contributed tool.
    Plugin { id: String },
    /// MCP server tool.
    Mcp { server: String, host_owned: bool },
    /// Context-injected tool.
    Context { source: ContextSource },
    /// Dynamically registered.
    Dynamic,
}

/// Context source for context-injected tools.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextSource {
    Skill,
    Subagent,
    User,
}

/// Tool category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolCategory {
    Filesystem,
    Search,
    Shell,
    Edit,
    Memory,
    Agent,
    Skill,
    Network,
    Utility,
    Custom,
}

/// Execution mode — how the orchestrator schedules this tool.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub enum ExecutionMode {
    /// May run concurrently with other Parallel tools.
    #[default]
    Parallel,
    /// Must run alone, after preceding tools complete.
    Sequential,
}

/// 工具曝光级别 — 控制工具何时对 LLM 可见。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize,
)]
pub enum ToolExposure {
    /// 始终可见，完整 schema 发送给 LLM
    #[default]
    Direct,
    /// 首次调用时才加载完整定义；发送给 LLM 的只有 name + description
    Deferred,
    /// 不对 LLM 可见，只能通过 Skill 或程序调用
    Hidden,
}

/// Cancellation behavior.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub enum CancelBehavior {
    /// Check cancellation at yield points (default).
    #[default]
    Cooperative,
    /// Kill the process on cancellation.
    KillOnCancel,
    /// Ignore cancellation (tool must complete).
    Ignore,
}
