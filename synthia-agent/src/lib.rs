pub mod clients;
pub mod core;
pub mod tools;
pub mod prompts;
pub mod memory;
pub mod mcp;

pub use clients::{
    LLMClient, LLMError, Message, MessageRole, OpenAIClient, StreamChunk, ToolDefinition,
    create_llm_client,
};
pub use core::{ReactAgent, Step};
pub use tools::{default_tools, ToolManager, ToolTrait};
pub use prompts::build_code_agent_prompt;
pub use memory::{ContextCompressor, ConversationHistory, ToolResult};
pub use mcp::{MCPConfig, MCPError, MCPManager};
