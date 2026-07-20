//! Built-in ContextFragment implementations.
//!
//! Provides standard fragment types used by the Synthia context assembly
//! pipeline: system prompt, token budget, skills, permissions, plugins,
//! environment, rollout budget, and a generic custom fragment.

use async_trait::async_trait;

use super::fragment::{ContextFragment, FragmentContext, FragmentError};

// ---------------------------------------------------------------------------
// FragmentPriorities
// ---------------------------------------------------------------------------

/// Canonical priority values for built-in fragments.
///
/// Lower value = higher priority (rendered first).
pub struct FragmentPriorities;

impl FragmentPriorities {
    /// Workspace / environment info.
    pub const ENVIRONMENT: u32 = 50;
    /// Permission rules.
    pub const PERMISSIONS: u32 = 30;
    /// Active plugin information.
    pub const PLUGINS: u32 = 40;
    /// Rollout / file-change budget.
    pub const ROLLOUT_BUDGET: u32 = 60;
    /// Skill summaries / instructions.
    pub const SKILLS: u32 = 20;
    /// System prompt — highest priority.
    pub const SYSTEM_PROMPT: u32 = 0;
    /// Token budget status.
    pub const TOKEN_BUDGET: u32 = 10;
}

// ---------------------------------------------------------------------------
// SystemPromptFragment
// ---------------------------------------------------------------------------

/// Renders the system prompt text.
pub struct SystemPromptFragment {
    content: String,
    active: bool,
}

impl SystemPromptFragment {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            active: true,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

#[async_trait]
impl ContextFragment for SystemPromptFragment {
    fn name(&self) -> &str {
        "system_prompt"
    }

    fn priority(&self) -> u32 {
        FragmentPriorities::SYSTEM_PROMPT
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        _ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        Ok(self.content.clone())
    }
}

// ---------------------------------------------------------------------------
// TokenBudgetFragment
// ---------------------------------------------------------------------------

/// Renders token budget status derived from [`FragmentContext`].
pub struct TokenBudgetFragment {
    active: bool,
}

impl TokenBudgetFragment {
    pub fn new() -> Self {
        Self { active: true }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

impl Default for TokenBudgetFragment {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextFragment for TokenBudgetFragment {
    fn name(&self) -> &str {
        "token_budget"
    }

    fn priority(&self) -> u32 {
        FragmentPriorities::TOKEN_BUDGET
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        let budget_part = match ctx.token_budget {
            Some(b) => b.to_string(),
            None => "unlimited".to_string(),
        };
        Ok(format!("Token budget: {}/{}", ctx.tokens_used, budget_part))
    }
}

// ---------------------------------------------------------------------------
// SkillsFragment
// ---------------------------------------------------------------------------

/// Renders skill summaries / instructions.
pub struct SkillsFragment {
    content: String,
    active: bool,
}

impl SkillsFragment {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            active: true,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

#[async_trait]
impl ContextFragment for SkillsFragment {
    fn name(&self) -> &str {
        "skills"
    }

    fn priority(&self) -> u32 {
        FragmentPriorities::SKILLS
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        _ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        Ok(self.content.clone())
    }
}

// ---------------------------------------------------------------------------
// PermissionsFragment
// ---------------------------------------------------------------------------

/// Renders permission rules.
pub struct PermissionsFragment {
    content: String,
    active: bool,
}

impl PermissionsFragment {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            active: true,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

#[async_trait]
impl ContextFragment for PermissionsFragment {
    fn name(&self) -> &str {
        "permissions"
    }

    fn priority(&self) -> u32 {
        FragmentPriorities::PERMISSIONS
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        _ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        Ok(self.content.clone())
    }
}

// ---------------------------------------------------------------------------
// PluginsFragment
// ---------------------------------------------------------------------------

/// Renders active plugin information.
pub struct PluginsFragment {
    content: String,
    active: bool,
}

impl PluginsFragment {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            active: true,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

#[async_trait]
impl ContextFragment for PluginsFragment {
    fn name(&self) -> &str {
        "plugins"
    }

    fn priority(&self) -> u32 {
        FragmentPriorities::PLUGINS
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        _ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        Ok(self.content.clone())
    }
}

// ---------------------------------------------------------------------------
// EnvironmentFragment
// ---------------------------------------------------------------------------

/// Renders workspace / environment info.
pub struct EnvironmentFragment {
    content: String,
    active: bool,
}

impl EnvironmentFragment {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            active: true,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

#[async_trait]
impl ContextFragment for EnvironmentFragment {
    fn name(&self) -> &str {
        "environment"
    }

    fn priority(&self) -> u32 {
        FragmentPriorities::ENVIRONMENT
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        _ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        Ok(self.content.clone())
    }
}

// ---------------------------------------------------------------------------
// RolloutBudgetFragment
// ---------------------------------------------------------------------------

/// Renders rollout / file-change budget.
pub struct RolloutBudgetFragment {
    content: String,
    active: bool,
}

impl RolloutBudgetFragment {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            active: true,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

#[async_trait]
impl ContextFragment for RolloutBudgetFragment {
    fn name(&self) -> &str {
        "rollout_budget"
    }

    fn priority(&self) -> u32 {
        FragmentPriorities::ROLLOUT_BUDGET
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        _ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        Ok(self.content.clone())
    }
}

// ---------------------------------------------------------------------------
// CustomFragment
// ---------------------------------------------------------------------------

/// User-defined fragment with arbitrary content, name, and priority.
pub struct CustomFragment {
    name: String,
    priority: u32,
    content: String,
    active: bool,
}

impl CustomFragment {
    pub fn new(
        name: impl Into<String>,
        priority: u32,
        content: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            priority,
            content: content.into(),
            active: true,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

#[async_trait]
impl ContextFragment for CustomFragment {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        _ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        Ok(self.content.clone())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> FragmentContext {
        FragmentContext::new("test-session", "test-user")
    }

    fn ctx_with_budget(
        tokens_used: usize,
        token_budget: Option<usize>,
    ) -> FragmentContext {
        FragmentContext {
            tokens_used,
            token_budget,
            ..ctx()
        }
    }

    // -- 1. SystemPromptFragment renders correctly -------------------------

    #[tokio::test]
    async fn system_prompt_renders_content() {
        let f = SystemPromptFragment::new("You are a helpful assistant.");
        assert_eq!(f.name(), "system_prompt");
        assert_eq!(f.priority(), 0);
        assert!(f.is_active());
        let result = f.render(&ctx()).await.unwrap();
        assert_eq!(result, "You are a helpful assistant.");
    }

    // -- 2. Inactive SystemPromptFragment ----------------------------------

    #[tokio::test]
    async fn system_prompt_inactive() {
        let f = SystemPromptFragment::new("hello").with_active(false);
        assert!(!f.is_active());
    }

    // -- 3. TokenBudgetFragment with budget --------------------------------

    #[tokio::test]
    async fn token_budget_with_limit() {
        let f = TokenBudgetFragment::new();
        assert_eq!(f.name(), "token_budget");
        assert_eq!(f.priority(), 10);
        let ctx = ctx_with_budget(500, Some(2000));
        let result = f.render(&ctx).await.unwrap();
        assert_eq!(result, "Token budget: 500/2000");
    }

    // -- 4. TokenBudgetFragment without budget (unlimited) -----------------

    #[tokio::test]
    async fn token_budget_unlimited() {
        let f = TokenBudgetFragment::new();
        let ctx = ctx_with_budget(300, None);
        let result = f.render(&ctx).await.unwrap();
        assert_eq!(result, "Token budget: 300/unlimited");
    }

    // -- 5. TokenBudgetFragment inactive -----------------------------------

    #[tokio::test]
    async fn token_budget_inactive() {
        let f = TokenBudgetFragment::new().with_active(false);
        assert!(!f.is_active());
    }

    // -- 6. SkillsFragment renders correctly -------------------------------

    #[tokio::test]
    async fn skills_fragment_renders() {
        let f = SkillsFragment::new("Available skills: debug, test");
        assert_eq!(f.name(), "skills");
        assert_eq!(f.priority(), 20);
        assert!(f.is_active());
        let result = f.render(&ctx()).await.unwrap();
        assert_eq!(result, "Available skills: debug, test");
    }

    // -- 7. PermissionsFragment renders correctly --------------------------

    #[tokio::test]
    async fn permissions_fragment_renders() {
        let f = PermissionsFragment::new("No restricted files.");
        assert_eq!(f.name(), "permissions");
        assert_eq!(f.priority(), 30);
        let result = f.render(&ctx()).await.unwrap();
        assert_eq!(result, "No restricted files.");
    }

    // -- 8. PluginsFragment renders correctly ------------------------------

    #[tokio::test]
    async fn plugins_fragment_renders() {
        let f = PluginsFragment::new("Plugin A, Plugin B");
        assert_eq!(f.name(), "plugins");
        assert_eq!(f.priority(), 40);
        let result = f.render(&ctx()).await.unwrap();
        assert_eq!(result, "Plugin A, Plugin B");
    }

    // -- 9. EnvironmentFragment renders correctly --------------------------

    #[tokio::test]
    async fn environment_fragment_renders() {
        let f = EnvironmentFragment::new("Workspace: /home/user/project");
        assert_eq!(f.name(), "environment");
        assert_eq!(f.priority(), 50);
        let result = f.render(&ctx()).await.unwrap();
        assert_eq!(result, "Workspace: /home/user/project");
    }

    // -- 10. RolloutBudgetFragment renders correctly ------------------------

    #[tokio::test]
    async fn rollout_budget_fragment_renders() {
        let f = RolloutBudgetFragment::new("File changes: 3/10");
        assert_eq!(f.name(), "rollout_budget");
        assert_eq!(f.priority(), 60);
        let result = f.render(&ctx()).await.unwrap();
        assert_eq!(result, "File changes: 3/10");
    }

    // -- 11. CustomFragment with custom name and priority ------------------

    #[tokio::test]
    async fn custom_fragment_custom_name_and_priority() {
        let f = CustomFragment::new("my_section", 99, "custom content here");
        assert_eq!(f.name(), "my_section");
        assert_eq!(f.priority(), 99);
        assert!(f.is_active());
        let result = f.render(&ctx()).await.unwrap();
        assert_eq!(result, "custom content here");
    }

    // -- 12. CustomFragment inactive ---------------------------------------

    #[tokio::test]
    async fn custom_fragment_inactive() {
        let f = CustomFragment::new("x", 1, "y").with_active(false);
        assert!(!f.is_active());
    }

    // -- 13. FragmentPriorities constants match expected values ------------

    #[test]
    fn fragment_priorities_constants() {
        assert_eq!(FragmentPriorities::SYSTEM_PROMPT, 0);
        assert_eq!(FragmentPriorities::TOKEN_BUDGET, 10);
        assert_eq!(FragmentPriorities::SKILLS, 20);
        assert_eq!(FragmentPriorities::PERMISSIONS, 30);
        assert_eq!(FragmentPriorities::PLUGINS, 40);
        assert_eq!(FragmentPriorities::ENVIRONMENT, 50);
        assert_eq!(FragmentPriorities::ROLLOUT_BUDGET, 60);
    }

    // -- 14. Inactive content fragments still return their content ---------

    #[tokio::test]
    async fn inactive_fragment_still_renders_content() {
        // is_active() is false but render() still works — active state is
        // only checked by the registry, not by render itself.
        let f = EnvironmentFragment::new("env-info").with_active(false);
        assert!(!f.is_active());
        let result = f.render(&ctx()).await.unwrap();
        assert_eq!(result, "env-info");
    }

    // -- 15. Default TokenBudgetFragment -----------------------------------

    #[test]
    fn token_budget_default() {
        let f = TokenBudgetFragment::default();
        assert!(f.is_active());
        assert_eq!(f.name(), "token_budget");
    }
}
