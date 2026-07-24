//! Fallback strategies for L3 recovery
//!
//! Provides alternative approaches when primary operations fail.

/// Fallback strategy for operations that can be replaced with simpler alternatives
pub struct FallbackStrategy {
    /// Description of what the fallback does
    pub description: String,
    /// The fallback action message to present to the agent
    pub action: String,
}

impl FallbackStrategy {
    /// Creates a new fallback strategy
    pub fn new(
        description: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            description: description.into(),
            action: action.into(),
        }
    }
}

/// Provides fallback strategies for various operations
pub struct FallbackProvider;

impl FallbackProvider {
    /// Returns a fallback strategy for the given operation, if available.
    ///
    /// # Arguments
    /// * `operation` - The name of the operation that failed
    pub fn get_fallback(operation: &str) -> Option<FallbackStrategy> {
        match operation {
            "web_fetch" => Some(FallbackStrategy::new(
                "Using cached content or skipping web fetch",
                "Using cached content or skipping".to_string(),
            )),
            "subagent" => Some(FallbackStrategy::new(
                "Answering directly without spawning a subagent",
                "Answering directly without subagent".to_string(),
            )),
            "mcp_tool" => Some(FallbackStrategy::new(
                "Using built-in tool instead of MCP tool",
                "Using built-in tool instead".to_string(),
            )),
            "file_read" => Some(FallbackStrategy::new(
                "Using previously read content from context or skipping",
                "Using previously read content or skipping".to_string(),
            )),
            "bash" => Some(FallbackStrategy::new(
                "Describing the command instead of executing it",
                "Describing the command instead of executing".to_string(),
            )),
            _ => None,
        }
    }

    /// Returns whether a fallback is available for the given operation
    pub fn has_fallback(operation: &str) -> bool {
        Self::get_fallback(operation).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_fetch_fallback() {
        let fallback = FallbackProvider::get_fallback("web_fetch");
        assert!(fallback.is_some());
        let fallback = fallback.unwrap();
        assert!(fallback.description.contains("cached content"));
    }

    #[test]
    fn test_subagent_fallback() {
        let fallback = FallbackProvider::get_fallback("subagent");
        assert!(fallback.is_some());
        assert!(fallback.unwrap().action.contains("directly"));
    }

    #[test]
    fn test_mcp_tool_fallback() {
        let fallback = FallbackProvider::get_fallback("mcp_tool");
        assert!(fallback.is_some());
        assert!(fallback.unwrap().action.contains("built-in"));
    }

    #[test]
    fn test_unknown_operation_no_fallback() {
        let fallback = FallbackProvider::get_fallback("unknown_operation");
        assert!(fallback.is_none());
    }

    #[test]
    fn test_has_fallback() {
        assert!(FallbackProvider::has_fallback("web_fetch"));
        assert!(FallbackProvider::has_fallback("bash"));
        assert!(!FallbackProvider::has_fallback("nonexistent"));
    }
}
