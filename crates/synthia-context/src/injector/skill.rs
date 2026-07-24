use super::ContextInjector;

/// Skill injector for progressive disclosure of skill information.
///
/// Provides three levels of skill injection:
/// - Level 0: Skill names and descriptions (always loaded at session start)
/// - Level 1: Full skill content (loaded on demand when skill is invoked)
/// - Level 2: Specific code snippets (loaded when explicitly requested)
pub struct SkillInjector {
    level0_prompt: String,
    level1_prompts: Vec<(String, String)>,
    level2_prompts: Vec<(String, String, Vec<String>)>,
}

impl SkillInjector {
    pub fn new() -> Self {
        Self {
            level0_prompt: String::new(),
            level1_prompts: Vec::new(),
            level2_prompts: Vec::new(),
        }
    }

    /// Set Level 0 prompt with skill names and descriptions.
    pub fn with_level0(mut self, prompt: String) -> Self {
        self.level0_prompt = prompt;
        self
    }

    /// Add a Level 1 skill prompt (full content for a specific skill).
    pub fn add_level1(mut self, skill_name: String, content: String) -> Self {
        self.level1_prompts.push((skill_name, content));
        self
    }

    /// Add a Level 2 snippet request for a skill.
    pub fn add_level2(
        mut self,
        skill_name: String,
        content: String,
        snippets: Vec<String>,
    ) -> Self {
        self.level2_prompts.push((skill_name, content, snippets));
        self
    }

    /// Build the complete skill injection prompt for all active levels.
    pub fn build_injection(&self) -> String {
        let mut result = String::new();

        if !self.level0_prompt.is_empty() {
            result.push_str(&self.level0_prompt);
            result.push_str("\n\n");
        }

        for (_, content) in &self.level1_prompts {
            result.push_str(content);
            result.push_str("\n\n");
        }

        for (_, content, _) in &self.level2_prompts {
            result.push_str(content);
            result.push_str("\n\n");
        }

        result.trim().to_string()
    }

    /// Get total estimated tokens for the injected content.
    pub fn estimate_tokens(&self) -> usize {
        self.build_injection().chars().count().div_ceil(4)
    }

    /// Check if any skills are injected at any level.
    pub fn is_empty(&self) -> bool {
        self.level0_prompt.is_empty()
            && self.level1_prompts.is_empty()
            && self.level2_prompts.is_empty()
    }

    /// Get count of Level 1 skills loaded.
    pub fn level1_count(&self) -> usize {
        self.level1_prompts.len()
    }

    /// Get count of Level 2 snippet requests.
    pub fn level2_count(&self) -> usize {
        self.level2_prompts.len()
    }
}

impl Default for SkillInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextInjector for SkillInjector {
    fn name(&self) -> &str {
        "skill_injector"
    }

    fn inject_system_prompt(&self) -> Option<String> {
        let content = self.build_injection();
        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    }

    fn inject_memories(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}
