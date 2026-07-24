use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptLayer {
    Fixed,
    SkillLevel0,
    Memory,
    Session,
}

impl PromptLayer {
    pub fn priority(&self) -> u8 {
        match self {
            PromptLayer::Fixed => 100,
            PromptLayer::SkillLevel0 => 90,
            PromptLayer::Memory => 80,
            PromptLayer::Session => 70,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PromptLayer::Fixed => "Fixed",
            PromptLayer::SkillLevel0 => "SkillLevel0",
            PromptLayer::Memory => "Memory",
            PromptLayer::Session => "Session",
        }
    }
}

pub struct PromptBuilder {
    layers: Vec<(PromptLayer, String)>,
    variables: HashMap<String, String>,
    token_budget: usize,
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            variables: HashMap::new(),
            token_budget: 0,
        }
    }

    pub fn with_token_budget(mut self, budget: usize) -> Self {
        self.token_budget = budget;
        self
    }

    pub fn add_layer(&mut self, layer: PromptLayer, content: String) {
        self.layers.push((layer, content));
    }

    pub fn add_fixed(&mut self, content: impl Into<String>) {
        self.add_layer(PromptLayer::Fixed, content.into());
    }

    pub fn add_skill_level0(&mut self, content: impl Into<String>) {
        self.add_layer(PromptLayer::SkillLevel0, content.into());
    }

    pub fn add_memory(&mut self, content: impl Into<String>) {
        self.add_layer(PromptLayer::Memory, content.into());
    }

    pub fn add_session(&mut self, content: impl Into<String>) {
        self.add_layer(PromptLayer::Session, content.into());
    }

    pub fn set_variable(&mut self, key: &str, value: &str) {
        self.variables.insert(key.to_string(), value.to_string());
    }

    pub fn build(&self) -> String {
        let mut prompt = String::new();
        for (_, content) in &self.layers {
            if !prompt.is_empty() {
                prompt.push_str("\n\n");
            }
            prompt.push_str(content);
        }
        let mut result = prompt;
        for (key, value) in &self.variables {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }

    pub fn estimate_tokens(&self) -> usize {
        let content: String = self
            .layers
            .iter()
            .map(|(_, c)| c.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        content.chars().count().div_ceil(4)
    }

    pub fn fits_budget(&self, extra_tokens: usize) -> bool {
        self.estimate_tokens() + extra_tokens <= self.token_budget
    }

    pub fn layers(&self) -> &[(PromptLayer, String)] {
        &self.layers
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_layer_priority() {
        assert!(
            PromptLayer::Fixed.priority() > PromptLayer::SkillLevel0.priority()
        );
        assert!(
            PromptLayer::SkillLevel0.priority()
                > PromptLayer::Memory.priority()
        );
        assert!(
            PromptLayer::Memory.priority() > PromptLayer::Session.priority()
        );
    }

    #[test]
    fn test_prompt_layer_as_str() {
        assert_eq!(PromptLayer::Fixed.as_str(), "Fixed");
        assert_eq!(PromptLayer::SkillLevel0.as_str(), "SkillLevel0");
        assert_eq!(PromptLayer::Memory.as_str(), "Memory");
        assert_eq!(PromptLayer::Session.as_str(), "Session");
    }

    #[test]
    fn test_build_prompt_with_layers() {
        let mut builder = PromptBuilder::new();
        builder.add_layer(PromptLayer::Fixed, "Layer 1".to_string());
        builder.add_layer(PromptLayer::SkillLevel0, "Layer 2".to_string());
        let prompt = builder.build();
        assert!(prompt.contains("Layer 1"));
        assert!(prompt.contains("Layer 2"));
    }

    #[test]
    fn test_build_prompt_with_named_layers() {
        let mut builder = PromptBuilder::new();
        builder.add_fixed("System prompt");
        builder.add_skill_level0("Skill level 0 info");
        builder.add_memory("User memory");
        builder.add_session("Session context");
        let prompt = builder.build();
        assert!(prompt.contains("System prompt"));
        assert!(prompt.contains("Skill level 0 info"));
        assert!(prompt.contains("User memory"));
        assert!(prompt.contains("Session context"));
    }

    #[test]
    fn test_build_prompt_with_variables() {
        let mut builder = PromptBuilder::new();
        builder.add_layer(PromptLayer::Fixed, "Hello {name}".to_string());
        builder.set_variable("name", "World");
        let prompt = builder.build();
        assert_eq!(prompt, "Hello World");
    }

    #[test]
    fn test_estimate_tokens() {
        let mut builder = PromptBuilder::new();
        builder.add_fixed("Test content for token estimation");
        let tokens = builder.estimate_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_fits_budget() {
        let builder = PromptBuilder::new()
            .with_token_budget(100)
            .with_token_budget(50);
        assert!(builder.fits_budget(0));
        assert!(builder.fits_budget(10));
        assert!(!builder.fits_budget(60));
    }

    #[test]
    fn test_layers_access() {
        let mut builder = PromptBuilder::new();
        builder.add_fixed("fixed");
        builder.add_skill_level0("skill");
        let layers = builder.layers();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].0, PromptLayer::Fixed);
        assert_eq!(layers[1].0, PromptLayer::SkillLevel0);
    }

    #[test]
    fn test_empty_builder() {
        let builder = PromptBuilder::new();
        assert!(builder.build().is_empty());
        assert_eq!(builder.estimate_tokens(), 0);
    }

    #[test]
    fn test_with_token_budget() {
        let builder = PromptBuilder::new().with_token_budget(1000);
        assert!(builder.fits_budget(0));
    }
}
