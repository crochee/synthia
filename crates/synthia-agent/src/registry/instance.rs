/// Runtime status of an agent instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Running,
    Completed,
    Errored,
    Cancelled,
}

/// Result produced by a completed agent execution.
#[derive(Debug)]
pub struct AgentResult {
    pub output: String,
    pub status: AgentStatus,
    pub token_usage: AgentTokenUsage,
}

/// Token usage statistics for an agent execution.
#[derive(Debug)]
pub struct AgentTokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}
