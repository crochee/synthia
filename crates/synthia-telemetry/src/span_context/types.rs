use std::collections::HashMap;

/// Attributes that can be attached to any span.
pub type SpanAttributes = HashMap<String, serde_json::Value>;

/// Kinds of step spans that can be created within an invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    LlmCall,
    ToolExecution,
    ContextAssembly,
    GuardianCheck,
    Compaction,
}

impl std::fmt::Display for StepKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepKind::LlmCall => write!(f, "llm_call"),
            StepKind::ToolExecution => write!(f, "tool_execution"),
            StepKind::ContextAssembly => write!(f, "context_assembly"),
            StepKind::GuardianCheck => write!(f, "guardian_check"),
            StepKind::Compaction => write!(f, "compaction"),
        }
    }
}
