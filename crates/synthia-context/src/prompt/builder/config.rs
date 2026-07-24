//! [`EffectivePromptConfig`] — high-level prompt overrides applied
//! on top of the resolved [`super::PromptBuilder`] output.
//!
//! Each `with_*` builder method returns `Self` to allow chaining.
//! The order in which the layers are applied during
//! `build_effective_prompt` is:
//!
//! 1. `override_prompt` (short-circuit if set)
//! 2. `coordinator_prompt` + `append_prompt` (if coordinator mode)
//! 3. `prompt` (static base) + resolved dynamic content
//! 4. `agent_prompt` / `custom_prompt` / `append_prompt` (appended)

#[derive(Debug, Clone, Default)]
pub struct EffectivePromptConfig {
    pub override_prompt: Option<String>,
    pub coordinator_prompt: Option<String>,
    pub agent_prompt: Option<String>,
    pub custom_prompt: Option<String>,
    pub append_prompt: Option<String>,
    pub use_coordinator_mode: bool,
    /// Static base prompt. When set, this replaces the resolved prompt sections
    /// as the static portion, with dynamic content appended after.
    pub prompt: Option<String>,
}

impl EffectivePromptConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_override(mut self, prompt: String) -> Self {
        self.override_prompt = Some(prompt);
        self
    }

    pub fn with_coordinator(mut self, prompt: String) -> Self {
        self.coordinator_prompt = Some(prompt);
        self
    }

    pub fn with_agent(mut self, prompt: String) -> Self {
        self.agent_prompt = Some(prompt);
        self
    }

    pub fn with_custom(mut self, prompt: String) -> Self {
        self.custom_prompt = Some(prompt);
        self
    }

    pub fn with_append(mut self, prompt: String) -> Self {
        self.append_prompt = Some(prompt);
        self
    }

    pub fn with_coordinator_mode(mut self, enabled: bool) -> Self {
        self.use_coordinator_mode = enabled;
        self
    }

    pub fn with_prompt(mut self, prompt: String) -> Self {
        self.prompt = Some(prompt);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_prompt_config_builder_methods() {
        let config = EffectivePromptConfig::new()
            .with_override("override".to_string())
            .with_coordinator("coordinator".to_string())
            .with_agent("agent".to_string())
            .with_custom("custom".to_string())
            .with_append("append".to_string())
            .with_coordinator_mode(true);

        assert_eq!(config.override_prompt, Some("override".to_string()));
        assert_eq!(config.coordinator_prompt, Some("coordinator".to_string()));
        assert_eq!(config.agent_prompt, Some("agent".to_string()));
        assert_eq!(config.custom_prompt, Some("custom".to_string()));
        assert_eq!(config.append_prompt, Some("append".to_string()));
        assert!(config.use_coordinator_mode);
    }

    #[test]
    fn test_effective_prompt_config_default() {
        let config = EffectivePromptConfig::new();
        assert!(config.override_prompt.is_none());
        assert!(config.coordinator_prompt.is_none());
        assert!(config.agent_prompt.is_none());
        assert!(config.custom_prompt.is_none());
        assert!(config.append_prompt.is_none());
        assert!(!config.use_coordinator_mode);
    }

    #[test]
    fn test_effective_prompt_config_with_coordinator_mode_false() {
        let config = EffectivePromptConfig::new().with_coordinator_mode(false);
        assert!(!config.use_coordinator_mode);
    }

    #[test]
    fn test_effective_prompt_config_with_prompt() {
        let config = EffectivePromptConfig::new().with_prompt("P".to_string());
        assert_eq!(config.prompt, Some("P".to_string()));
    }
}
