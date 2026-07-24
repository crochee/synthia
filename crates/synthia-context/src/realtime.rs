//! Realtime context module
//!
//! Provides startup context building with token budgets.

use anyhow::Error;

/// Configuration for realtime context building
#[derive(Debug, Clone)]
pub struct RealtimeContextConfig {
    /// Token budget for recent work section (default: 2200)
    pub recent_work_token_budget: usize,
    /// Token budget for workspace section (default: 1600)
    pub workspace_token_budget: usize,
    /// Token budget for notes section (default: 300)
    pub notes_token_budget: usize,
}

impl Default for RealtimeContextConfig {
    fn default() -> Self {
        Self {
            recent_work_token_budget: 2200,
            workspace_token_budget: 1600,
            notes_token_budget: 300,
        }
    }
}

/// Builds realtime startup context with explicit token budgets
#[derive(Clone)]
pub struct RealtimeContext {
    config: RealtimeContextConfig,
}

impl RealtimeContext {
    pub fn new(config: RealtimeContextConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(RealtimeContextConfig::default())
    }

    /// Build startup context string respecting token budgets
    pub async fn build_startup_context(&self) -> Result<String, Error> {
        let mut context = String::new();

        // Recent work section
        context.push_str(&format!(
            "# Recent Work (max {} tokens)\n\n",
            self.config.recent_work_token_budget
        ));

        // Workspace section
        context.push_str(&format!(
            "# Workspace (max {} tokens)\n\n",
            self.config.workspace_token_budget
        ));

        // Notes section
        context.push_str(&format!(
            "# Notes (max {} tokens)\n\n",
            self.config.notes_token_budget
        ));

        Ok(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_startup_context() {
        let ctx = RealtimeContext::with_default_config();
        let result = ctx.build_startup_context().await.unwrap();
        assert!(result.contains("Recent Work"));
        assert!(result.contains("Workspace"));
        assert!(result.contains("Notes"));
    }

    #[tokio::test]
    async fn test_build_startup_context_with_custom_config() {
        let config = RealtimeContextConfig {
            recent_work_token_budget: 5000,
            workspace_token_budget: 3000,
            notes_token_budget: 1000,
        };
        let ctx = RealtimeContext::new(config);
        let result = ctx.build_startup_context().await.unwrap();

        assert!(result.contains("max 5000 tokens"));
        assert!(result.contains("max 3000 tokens"));
        assert!(result.contains("max 1000 tokens"));
    }

    #[tokio::test]
    async fn test_build_startup_context_sections_present() {
        let ctx = RealtimeContext::with_default_config();
        let result = ctx.build_startup_context().await.unwrap();

        // Verify section headers are present
        assert!(result.contains("# Recent Work"));
        assert!(result.contains("# Workspace"));
        assert!(result.contains("# Notes"));

        // Verify section ordering
        let recent_work_idx = result.find("# Recent Work").unwrap();
        let workspace_idx = result.find("# Workspace").unwrap();
        let notes_idx = result.find("# Notes").unwrap();

        assert!(recent_work_idx < workspace_idx);
        assert!(workspace_idx < notes_idx);
    }

    #[test]
    fn test_realtime_context_config_default() {
        let config = RealtimeContextConfig::default();
        assert_eq!(config.recent_work_token_budget, 2200);
        assert_eq!(config.workspace_token_budget, 1600);
        assert_eq!(config.notes_token_budget, 300);
    }

    #[test]
    fn test_realtime_context_config_custom() {
        let config = RealtimeContextConfig {
            recent_work_token_budget: 1000,
            workspace_token_budget: 2000,
            notes_token_budget: 500,
        };
        assert_eq!(config.recent_work_token_budget, 1000);
        assert_eq!(config.workspace_token_budget, 2000);
        assert_eq!(config.notes_token_budget, 500);
    }
}
