use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use super::{
    DynamicMcpInstructionsSection,
    EnvironmentSection,
    IdentitySection,
    LanguageSection,
    MemorySection,
    OutputStyleSection,
    ProactiveSection,
    PromptSection,
    SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
    SectionCaching,
    SkillSection,
    SystemSection,
    TaskExecutionSection,
    TeamModeSection,
    TeamPromptInfo,
    TokenBudgetSection,
    ToolUsageSection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SystemPromptPriority {
    Override,
    Coordinator,
    Agent,
    Custom,
    #[default]
    Default,
}

#[derive(Clone, Default)]
pub struct PromptState {
    global_cache: std::collections::HashMap<String, String>,
    session_cache: std::collections::HashMap<String, String>,
}

impl PromptState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear_session(&mut self) {
        self.session_cache.clear();
    }

    pub fn clear_all(&mut self) {
        self.global_cache.clear();
        self.session_cache.clear();
    }

    pub fn invalidate(&mut self, name: &str) {
        self.session_cache.remove(name);
    }

    pub fn get(&self, name: &str, caching: SectionCaching) -> Option<String> {
        match caching {
            SectionCaching::Cached => self.global_cache.get(name).cloned(),
            SectionCaching::SessionCached | SectionCaching::Volatile => {
                self.session_cache.get(name).cloned()
            }
            SectionCaching::Uncached => None,
        }
    }

    pub fn insert(
        &mut self,
        name: String,
        value: String,
        caching: SectionCaching,
    ) {
        match caching {
            SectionCaching::Cached => {
                self.global_cache.insert(name, value);
            }
            SectionCaching::SessionCached | SectionCaching::Volatile => {
                self.session_cache.insert(name, value);
            }
            SectionCaching::Uncached => {}
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            global_entries: self.global_cache.len() as u64,
            session_entries: self.session_cache.len() as u64,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub global_entries: u64,
    pub session_entries: u64,
}

#[derive(Debug, Clone)]
pub struct ResolvedPrompt {
    pub static_content: String,
    pub dynamic_content: String,
    pub sections_used: Vec<String>,
    pub prefix_hash: String,
    pub static_hash: String,
}

impl ResolvedPrompt {
    pub fn full_prompt(&self) -> String {
        if self.dynamic_content.is_empty() {
            self.static_content.clone()
        } else {
            format!(
                "{}\n\n{}\n\n{}",
                self.static_content,
                SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
                self.dynamic_content
            )
        }
    }

    pub fn get_static_prefix(&self) -> &str {
        &self.static_content
    }

    pub fn get_dynamic_tail(&self) -> &str {
        &self.dynamic_content
    }
}

pub struct PromptBuilder {
    sections: Vec<Box<dyn PromptSection>>,
}

impl std::fmt::Debug for PromptBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptBuilder")
            .field("sections_count", &self.sections.len())
            .finish_non_exhaustive()
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    pub fn default_with_sections() -> Self {
        Self::new()
            .add_section(Box::new(IdentitySection))
            .add_section(Box::new(SystemSection))
            .add_section(Box::new(TaskExecutionSection))
            .add_section(Box::new(ToolUsageSection))
            .add_section(Box::new(EnvironmentSection::new()))
            .add_section(Box::new(MemorySection::new()))
            .add_section(Box::new(SkillSection::new()))
            .add_section(Box::new(DynamicMcpInstructionsSection::new(vec![])))
            .add_section(Box::new(OutputStyleSection::default()))
            .add_section(Box::new(LanguageSection::default()))
            .add_section(Box::new(ProactiveSection::new()))
            .add_section(Box::new(TokenBudgetSection::new()))
            .add_section(Box::new(TeamModeSection::new(
                crate::config::AgentName::Solo,
                TeamPromptInfo {
                    role: String::new(),
                    team_id: String::new(),
                    member_id: None,
                },
            )))
    }

    /// Builds prompt sections for a specific agent name.
    pub fn build_for_name(name: &crate::config::AgentName) -> Self {
        let team_info = TeamPromptInfo {
            role: match name {
                crate::config::AgentName::Solo => "Solo".to_string(),
                crate::config::AgentName::Lead => "Lead".to_string(),
                crate::config::AgentName::Custom(_) => "Member".to_string(),
            },
            team_id: String::new(),
            member_id: None,
        };

        Self::new()
            .add_section(Box::new(IdentitySection))
            .add_section(Box::new(SystemSection))
            .add_section(Box::new(TaskExecutionSection))
            .add_section(Box::new(ToolUsageSection))
            .add_section(Box::new(EnvironmentSection::new()))
            .add_section(Box::new(MemorySection::new()))
            .add_section(Box::new(SkillSection::new()))
            .add_section(Box::new(DynamicMcpInstructionsSection::new(vec![])))
            .add_section(Box::new(OutputStyleSection::default()))
            .add_section(Box::new(LanguageSection::default()))
            .add_section(Box::new(ProactiveSection::new()))
            .add_section(Box::new(TokenBudgetSection::new()))
            .add_section(Box::new(TeamModeSection::new(
                name.clone(),
                team_info,
            )))
    }

    pub fn add_section(mut self, section: Box<dyn PromptSection>) -> Self {
        self.sections.push(section);
        self
    }

    pub fn resolve(
        &self,
        ctx: &super::PromptContext<'_>,
        state: &mut PromptState,
    ) -> super::Result<ResolvedPrompt> {
        let mut static_content = String::new();
        let mut dynamic_content = String::new();
        let mut sections_used = Vec::new();
        let mut static_hasher = DefaultHasher::new();
        let mut full_hasher = DefaultHasher::new();

        for section in &self.sections {
            let caching = section.caching();

            let part = if caching == SectionCaching::Uncached {
                section.build(ctx)?
            } else if let Some(cached) = state.get(section.name(), caching) {
                cached
            } else {
                let value = section.build(ctx)?;
                state.insert(
                    section.name().to_string(),
                    value.clone(),
                    caching,
                );
                value
            };

            if part.trim().is_empty() {
                continue;
            }

            sections_used.push(section.name().to_string());

            let trimmed = part.trim_end();
            trimmed.hash(&mut full_hasher);

            if caching == SectionCaching::Cached {
                trimmed.hash(&mut static_hasher);
            }

            if caching == SectionCaching::Uncached
                || caching == SectionCaching::SessionCached
                || caching == SectionCaching::Volatile
            {
                if !dynamic_content.is_empty() {
                    dynamic_content.push_str("\n\n");
                }
                dynamic_content.push_str(trimmed);
            } else {
                if !static_content.is_empty() {
                    static_content.push_str("\n\n");
                }
                static_content.push_str(trimmed);
            }
        }

        let prefix_hash = format!("{:x}", full_hasher.finish());
        let static_hash = format!("{:x}", static_hasher.finish());

        Ok(ResolvedPrompt {
            static_content,
            dynamic_content,
            sections_used,
            prefix_hash,
            static_hash,
        })
    }

    pub fn section_names(&self) -> Vec<&str> {
        self.sections
            .iter()
            .map(super::PromptSection::name)
            .collect()
    }

    pub fn validate_prefix_stability(
        &self,
        ctx: &super::PromptContext<'_>,
        state: &PromptState,
        previous_static_hash: Option<&str>,
    ) -> super::Result<bool> {
        let Some(previous) = previous_static_hash else {
            return Ok(true);
        };

        let mut static_hasher = DefaultHasher::new();

        for section in &self.sections {
            let caching = section.caching();
            if caching != SectionCaching::Cached {
                continue;
            }

            let part = if let Some(cached) = state.get(section.name(), caching)
            {
                cached
            } else {
                section.build(ctx)?
            };

            if part.trim().is_empty() {
                continue;
            }

            let trimmed = part.trim_end();
            trimmed.hash(&mut static_hasher);
        }

        let current_static_hash = format!("{:x}", static_hasher.finish());
        Ok(current_static_hash == previous)
    }

    pub fn get_static_sections(&self) -> Vec<&str> {
        self.sections
            .iter()
            .filter(|s| s.caching() == SectionCaching::Cached)
            .map(super::PromptSection::name)
            .collect()
    }

    pub fn get_dynamic_sections(&self) -> Vec<&str> {
        self.sections
            .iter()
            .filter(|s| s.caching() != SectionCaching::Cached)
            .map(super::PromptSection::name)
            .collect()
    }

    pub fn build_effective_prompt(
        &self,
        ctx: &super::PromptContext<'_>,
        state: &mut PromptState,
        effective_config: EffectivePromptConfig,
    ) -> super::Result<String> {
        if let Some(override_prompt) = effective_config.override_prompt {
            return Ok(override_prompt);
        }

        if effective_config.use_coordinator_mode
            && let Some(ref coordinator_prompt) =
                effective_config.coordinator_prompt
        {
            let mut result = coordinator_prompt.clone();
            if let Some(ref append) = effective_config.append_prompt {
                result.push_str("\n\n");
                result.push_str(append);
            }
            return Ok(result);
        }

        let resolved = self.resolve(ctx, state)?;

        let mut final_prompt = if let Some(ref prompt) = effective_config.prompt
        {
            if resolved.dynamic_content.is_empty() {
                prompt.clone()
            } else {
                format!(
                    "{}\n\n{}\n\n{}",
                    prompt,
                    SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
                    resolved.dynamic_content
                )
            }
        } else {
            resolved.full_prompt()
        };

        if let Some(ref agent_prompt) = effective_config.agent_prompt {
            final_prompt.push_str("\n\n");
            final_prompt.push_str(agent_prompt);
        }

        if let Some(ref custom_prompt) = effective_config.custom_prompt {
            final_prompt.push_str("\n\n");
            final_prompt.push_str(custom_prompt);
        }

        if let Some(ref append) = effective_config.append_prompt {
            final_prompt.push_str("\n\n");
            final_prompt.push_str(append);
        }

        Ok(final_prompt)
    }
}

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
    use std::sync::LazyLock;

    use super::*;
    use crate::{
        config::AgentName,
        prompt::{PromptContext, PromptSection},
    };

    static TEST_AGENT_NAME: LazyLock<AgentName> =
        LazyLock::new(|| AgentName::Custom("TestAgent".to_string()));

    fn make_test_context() -> PromptContext<'static> {
        PromptContext {
            agent_name: &TEST_AGENT_NAME,
            agent_description: "A test agent",
            workspace_dir: std::path::Path::new("/tmp/test"),
            skill_instructions: String::new(),
            is_subagent: false,
            session_id: Some("test-session-123"),
            mcp_servers: &[],
            additional_dirs: &[],
            output_style: None,
            language_preference: None,
            is_proactive_mode: false,
            model_name: Some("claude-sonnet"),
            knowledge_cutoff: Some("2024-01"),
            team_info: None,
        }
    }

    struct MockSection {
        name: String,
        caching: SectionCaching,
        content: String,
    }

    impl MockSection {
        fn new(name: &str, caching: SectionCaching, content: &str) -> Self {
            Self {
                name: name.to_string(),
                caching,
                content: content.to_string(),
            }
        }
    }

    impl PromptSection for MockSection {
        fn name(&self) -> &str {
            &self.name
        }

        fn caching(&self) -> SectionCaching {
            self.caching
        }

        fn build(&self, _ctx: &PromptContext<'_>) -> crate::Result<String> {
            Ok(self.content.clone())
        }
    }

    #[test]
    fn test_prompt_builder_new() {
        let builder = PromptBuilder::new();
        assert_eq!(builder.sections.len(), 0);
        assert!(builder.section_names().is_empty());
    }

    #[test]
    fn test_prompt_builder_add_section() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "test1",
                SectionCaching::Cached,
                "content1",
            )))
            .add_section(Box::new(MockSection::new(
                "test2",
                SectionCaching::SessionCached,
                "content2",
            )));

        assert_eq!(builder.sections.len(), 2);
        assert_eq!(builder.section_names(), vec!["test1", "test2"]);
    }

    #[test]
    fn test_prompt_builder_default_with_sections() {
        let builder = PromptBuilder::default_with_sections();
        let names = builder.section_names();

        assert!(names.contains(&"identity"));
        assert!(names.contains(&"system"));
        assert!(names.contains(&"task_execution"));
        assert!(names.contains(&"tool_usage"));
        assert!(names.contains(&"environment"));
        assert!(names.contains(&"memory"));
        assert!(names.contains(&"skills"));
        assert!(names.contains(&"mcp_instructions"));
        assert!(names.contains(&"output_style"));
        assert!(names.contains(&"language"));
        assert!(names.contains(&"proactive"));
        assert!(names.contains(&"token_budget"));
    }

    #[test]
    fn test_prompt_builder_resolve_empty() {
        let builder = PromptBuilder::new();
        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();
        assert!(result.static_content.is_empty());
        assert!(result.dynamic_content.is_empty());
        assert!(result.sections_used.is_empty());
    }

    #[test]
    fn test_prompt_builder_resolve_cached_section() {
        let builder =
            PromptBuilder::new().add_section(Box::new(MockSection::new(
                "cached",
                SectionCaching::Cached,
                "cached content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        // First resolve - builds and caches
        let result1 = builder.resolve(&ctx, &mut state).unwrap();
        assert!(result1.static_content.contains("cached content"));

        // Second resolve - should use cache
        let result2 = builder.resolve(&ctx, &mut state).unwrap();
        assert_eq!(result1.static_hash, result2.static_hash);
    }

    #[test]
    fn test_prompt_builder_resolve_volatile_section() {
        let builder =
            PromptBuilder::new().add_section(Box::new(MockSection::new(
                "volatile",
                SectionCaching::Volatile,
                "volatile content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();
        // Volatile sections go to dynamic content
        assert!(result.dynamic_content.contains("volatile content"));
        assert!(result.static_content.is_empty());
    }

    #[test]
    fn test_prompt_builder_resolve_session_cached_section() {
        let builder =
            PromptBuilder::new().add_section(Box::new(MockSection::new(
                "session",
                SectionCaching::SessionCached,
                "session content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();
        // SessionCached goes to dynamic content
        assert!(result.dynamic_content.contains("session content"));
    }

    #[test]
    fn test_prompt_builder_resolve_uncached_section() {
        let builder =
            PromptBuilder::new().add_section(Box::new(MockSection::new(
                "uncached",
                SectionCaching::Uncached,
                "uncached content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();
        // Uncached goes to dynamic content
        assert!(result.dynamic_content.contains("uncached content"));
    }

    #[test]
    fn test_prompt_builder_resolve_empty_section_skipped() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "empty",
                SectionCaching::Cached,
                "   ",
            )))
            .add_section(Box::new(MockSection::new(
                "nonempty",
                SectionCaching::Cached,
                "actual content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();
        assert!(!result.sections_used.contains(&"empty".to_string()));
        assert!(result.sections_used.contains(&"nonempty".to_string()));
    }

    #[test]
    fn test_prompt_builder_resolve_multiple_cached_sections() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "section1",
                SectionCaching::Cached,
                "content1",
            )))
            .add_section(Box::new(MockSection::new(
                "section2",
                SectionCaching::Cached,
                "content2",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();
        assert!(result.static_content.contains("content1"));
        assert!(result.static_content.contains("content2"));
        assert!(result.static_content.contains("\n\n"));
    }

    #[test]
    fn test_prompt_builder_resolve_mixed_sections() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "static",
                SectionCaching::Cached,
                "static content",
            )))
            .add_section(Box::new(MockSection::new(
                "dynamic",
                SectionCaching::Volatile,
                "dynamic content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();
        assert!(result.static_content.contains("static content"));
        assert!(result.dynamic_content.contains("dynamic content"));
    }

    #[test]
    fn test_prompt_builder_validate_prefix_stability_no_previous() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "content"),
        ));

        let ctx = make_test_context();
        let state = PromptState::new();

        let result = builder
            .validate_prefix_stability(&ctx, &state, None)
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_prompt_builder_validate_prefix_stability_match() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "content"),
        ));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        // First resolve to compute hash
        let resolved = builder.resolve(&ctx, &mut state).unwrap();
        let static_hash = resolved.static_hash.as_str();

        // Validate against the same hash
        let result = builder
            .validate_prefix_stability(&ctx, &state, Some(static_hash))
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_prompt_builder_validate_prefix_stability_mismatch() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "content"),
        ));

        let ctx = make_test_context();
        let state = PromptState::new();

        // Validate against a different hash
        let result = builder
            .validate_prefix_stability(&ctx, &state, Some("different_hash"))
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_prompt_builder_validate_prefix_stability_ignores_non_cached() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "cached",
                SectionCaching::Cached,
                "cached content",
            )))
            .add_section(Box::new(MockSection::new(
                "volatile",
                SectionCaching::Volatile,
                "volatile content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        // Only cached sections contribute to static hash
        let resolved = builder.resolve(&ctx, &mut state).unwrap();
        let static_hash = resolved.static_hash.as_str();

        let result = builder
            .validate_prefix_stability(&ctx, &state, Some(static_hash))
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_prompt_builder_get_static_sections() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "cached",
                SectionCaching::Cached,
                "",
            )))
            .add_section(Box::new(MockSection::new(
                "volatile",
                SectionCaching::Volatile,
                "",
            )))
            .add_section(Box::new(MockSection::new(
                "session",
                SectionCaching::SessionCached,
                "",
            )))
            .add_section(Box::new(MockSection::new(
                "uncached",
                SectionCaching::Uncached,
                "",
            )));

        let static_sections = builder.get_static_sections();
        assert_eq!(static_sections, vec!["cached"]);
    }

    #[test]
    fn test_prompt_builder_get_dynamic_sections() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "cached",
                SectionCaching::Cached,
                "",
            )))
            .add_section(Box::new(MockSection::new(
                "volatile",
                SectionCaching::Volatile,
                "",
            )))
            .add_section(Box::new(MockSection::new(
                "session",
                SectionCaching::SessionCached,
                "",
            )))
            .add_section(Box::new(MockSection::new(
                "uncached",
                SectionCaching::Uncached,
                "",
            )));

        let dynamic_sections = builder.get_dynamic_sections();
        assert_eq!(dynamic_sections, vec!["volatile", "session", "uncached"]);
    }

    #[test]
    fn test_prompt_builder_build_effective_prompt_override() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "base content"),
        ));

        let ctx = make_test_context();
        let mut state = PromptState::new();
        let config = EffectivePromptConfig::new()
            .with_override("override prompt".to_string());

        let result = builder
            .build_effective_prompt(&ctx, &mut state, config)
            .unwrap();
        assert_eq!(result, "override prompt");
    }

    #[test]
    fn test_prompt_builder_build_effective_prompt_coordinator_mode() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "base content"),
        ));

        let ctx = make_test_context();
        let mut state = PromptState::new();
        let config = EffectivePromptConfig::new()
            .with_coordinator_mode(true)
            .with_coordinator("coordinator prompt".to_string())
            .with_append("appended".to_string());

        let result = builder
            .build_effective_prompt(&ctx, &mut state, config)
            .unwrap();
        assert!(result.contains("coordinator prompt"));
        assert!(result.contains("appended"));
    }

    #[test]
    fn test_prompt_builder_build_effective_prompt_normal_flow() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "test",
                SectionCaching::Cached,
                "static section",
            )))
            .add_section(Box::new(MockSection::new(
                "dynamic",
                SectionCaching::Volatile,
                "dynamic section",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();
        let config = EffectivePromptConfig::new();

        let result = builder
            .build_effective_prompt(&ctx, &mut state, config)
            .unwrap();
        assert!(result.contains("static section"));
        assert!(result.contains("dynamic section"));
    }

    #[test]
    fn test_prompt_builder_build_effective_prompt_with_agent_and_custom() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "base"),
        ));

        let ctx = make_test_context();
        let mut state = PromptState::new();
        let config = EffectivePromptConfig::new()
            .with_agent("agent prompt".to_string())
            .with_custom("custom prompt".to_string());

        let result = builder
            .build_effective_prompt(&ctx, &mut state, config)
            .unwrap();
        assert!(result.contains("base"));
        assert!(result.contains("agent prompt"));
        assert!(result.contains("custom prompt"));
    }

    #[test]
    fn test_prompt_builder_build_effective_prompt_append_only() {
        let builder = PromptBuilder::new();

        let ctx = make_test_context();
        let mut state = PromptState::new();
        let config =
            EffectivePromptConfig::new().with_append("append only".to_string());

        let result = builder
            .build_effective_prompt(&ctx, &mut state, config)
            .unwrap();
        assert_eq!(result.trim(), "append only");
    }

    #[test]
    fn test_resolved_prompt_get_static_prefix() {
        let resolved = ResolvedPrompt {
            static_content: "static content".to_string(),
            dynamic_content: String::new(),
            sections_used: vec![],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };
        assert_eq!(resolved.get_static_prefix(), "static content");
    }

    #[test]
    fn test_resolved_prompt_get_dynamic_tail() {
        let resolved = ResolvedPrompt {
            static_content: "static".to_string(),
            dynamic_content: "dynamic content".to_string(),
            sections_used: vec![],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };
        assert_eq!(resolved.get_dynamic_tail(), "dynamic content");
    }

    #[test]
    fn test_resolved_prompt_get_dynamic_tail_empty() {
        let resolved = ResolvedPrompt {
            static_content: "static".to_string(),
            dynamic_content: String::new(),
            sections_used: vec![],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };
        assert_eq!(resolved.get_dynamic_tail(), "");
    }

    #[test]
    fn test_resolved_prompt_full_prompt_with_boundary() {
        let resolved = ResolvedPrompt {
            static_content: "static".to_string(),
            dynamic_content: "dynamic".to_string(),
            sections_used: vec!["test".to_string()],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };

        let full = resolved.full_prompt();
        assert!(full.contains("static"));
        assert!(full.contains(SYSTEM_PROMPT_DYNAMIC_BOUNDARY));
        assert!(full.contains("dynamic"));
    }

    #[test]
    fn test_resolved_prompt_full_prompt_no_dynamic() {
        let resolved = ResolvedPrompt {
            static_content: "static only".to_string(),
            dynamic_content: String::new(),
            sections_used: vec![],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };

        assert_eq!(resolved.full_prompt(), "static only");
    }

    #[test]
    fn test_prompt_state_cache_volatile() {
        let mut state = PromptState::new();
        state.insert(
            "key".to_string(),
            "value".to_string(),
            SectionCaching::Volatile,
        );
        assert_eq!(
            state.get("key", SectionCaching::Volatile),
            Some("value".to_string())
        );
        assert_eq!(state.get("key", SectionCaching::Cached), None);
    }

    #[test]
    fn test_prompt_state_cache_uncached() {
        let mut state = PromptState::new();
        state.insert(
            "key".to_string(),
            "value".to_string(),
            SectionCaching::Uncached,
        );
        // Uncached should not store anything
        assert_eq!(state.get("key", SectionCaching::Uncached), None);
        assert_eq!(state.get("key", SectionCaching::SessionCached), None);
    }

    #[test]
    fn test_prompt_state_insert_replaces() {
        let mut state = PromptState::new();
        state.insert(
            "key".to_string(),
            "value1".to_string(),
            SectionCaching::Cached,
        );
        state.insert(
            "key".to_string(),
            "value2".to_string(),
            SectionCaching::Cached,
        );
        assert_eq!(
            state.get("key", SectionCaching::Cached),
            Some("value2".to_string())
        );
    }

    #[test]
    fn test_prompt_state_stats() {
        let mut state = PromptState::new();
        assert_eq!(state.stats().global_entries, 0);
        assert_eq!(state.stats().session_entries, 0);

        state.insert(
            "k1".to_string(),
            "v1".to_string(),
            SectionCaching::Cached,
        );
        state.insert(
            "k2".to_string(),
            "v2".to_string(),
            SectionCaching::SessionCached,
        );
        state.insert(
            "k3".to_string(),
            "v3".to_string(),
            SectionCaching::Volatile,
        );

        assert_eq!(state.stats().global_entries, 1);
        assert_eq!(state.stats().session_entries, 2);
    }

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
    fn test_prompt_builder_debug_trait() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "content"),
        ));

        let debug = format!("{builder:?}");
        assert!(debug.contains("PromptBuilder"));
        assert!(debug.contains("sections_count"));
    }

    #[test]
    fn test_system_prompt_priority_debug() {
        let priorities = vec![
            SystemPromptPriority::Override,
            SystemPromptPriority::Coordinator,
            SystemPromptPriority::Agent,
            SystemPromptPriority::Custom,
            SystemPromptPriority::Default,
        ];

        for p in &priorities {
            let debug = format!("{p:?}");
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn test_system_prompt_priority_default() {
        let priority = SystemPromptPriority::default();
        assert_eq!(priority, SystemPromptPriority::Default);
    }

    #[test]
    fn test_system_prompt_priority_partial_eq() {
        assert_eq!(
            SystemPromptPriority::Default,
            SystemPromptPriority::Default
        );
        assert_ne!(
            SystemPromptPriority::Default,
            SystemPromptPriority::Override
        );
    }

    #[test]
    fn test_section_caching_is_static() {
        assert!(SectionCaching::Cached.is_static());
        assert!(!SectionCaching::SessionCached.is_static());
        assert!(!SectionCaching::Volatile.is_static());
        assert!(!SectionCaching::Uncached.is_static());
    }

    #[test]
    fn test_section_caching_is_dynamic() {
        assert!(!SectionCaching::Cached.is_dynamic());
        assert!(SectionCaching::SessionCached.is_dynamic());
        assert!(SectionCaching::Volatile.is_dynamic());
        assert!(SectionCaching::Uncached.is_dynamic());
    }

    #[test]
    fn test_cache_stats_default() {
        let stats = CacheStats::default();
        assert_eq!(stats.global_entries, 0);
        assert_eq!(stats.session_entries, 0);
    }

    #[test]
    fn test_prompt_state_new_and_clear() {
        let mut state = PromptState::new();
        state.insert(
            "k1".to_string(),
            "v1".to_string(),
            SectionCaching::Cached,
        );
        state.insert(
            "k2".to_string(),
            "v2".to_string(),
            SectionCaching::SessionCached,
        );

        state.clear_all();

        assert_eq!(state.stats().global_entries, 0);
        assert_eq!(state.stats().session_entries, 0);
    }

    #[test]
    fn test_prompt_state_invalidate_nonexistent() {
        let mut state = PromptState::new();
        state.insert(
            "k1".to_string(),
            "v1".to_string(),
            SectionCaching::SessionCached,
        );

        // Invalidating non-existent key should not panic
        state.invalidate("nonexistent");
        assert_eq!(
            state.get("k1", SectionCaching::SessionCached),
            Some("v1".to_string())
        );
    }

    #[test]
    fn test_prompt_builder_resolve_uses_cache() {
        let builder =
            PromptBuilder::new().add_section(Box::new(MockSection::new(
                "cached",
                SectionCaching::Cached,
                "original content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        // First resolve
        let result1 = builder.resolve(&ctx, &mut state).unwrap();
        assert!(result1.static_content.contains("original content"));

        // Manually insert different content into cache
        state.insert(
            "cached".to_string(),
            "cached content".to_string(),
            SectionCaching::Cached,
        );

        // Second resolve should use cache
        let result2 = builder.resolve(&ctx, &mut state).unwrap();
        assert!(result2.static_content.contains("cached content"));
    }

    #[test]
    fn test_prompt_builder_resolve_session_cache_persists_across_resolve() {
        let builder =
            PromptBuilder::new().add_section(Box::new(MockSection::new(
                "session",
                SectionCaching::SessionCached,
                "session content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        // First resolve
        let result1 = builder.resolve(&ctx, &mut state).unwrap();
        assert!(result1.dynamic_content.contains("session content"));

        // Second resolve should use cache
        let result2 = builder.resolve(&ctx, &mut state).unwrap();
        assert!(result2.dynamic_content.contains("session content"));
    }

    #[test]
    fn test_prompt_builder_resolve_trim_end() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "content   \n  "),
        ));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();
        // Should be trimmed at end
        assert!(result.static_content.ends_with("content"));
    }

    #[test]
    fn test_resolved_prompt_hashes() {
        let builder = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "content"),
        ));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();

        // Hashes should be non-empty hex strings
        assert!(!result.prefix_hash.is_empty());
        assert!(!result.static_hash.is_empty());
        // Hash format should be valid hex
        assert!(result.prefix_hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(result.static_hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_resolved_prompt_sections_used() {
        let builder = PromptBuilder::new()
            .add_section(Box::new(MockSection::new(
                "section1",
                SectionCaching::Cached,
                "content1",
            )))
            .add_section(Box::new(MockSection::new(
                "empty",
                SectionCaching::Cached,
                "   ",
            )))
            .add_section(Box::new(MockSection::new(
                "section2",
                SectionCaching::Volatile,
                "content2",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();

        let result = builder.resolve(&ctx, &mut state).unwrap();

        assert!(result.sections_used.contains(&"section1".to_string()));
        assert!(!result.sections_used.contains(&"empty".to_string()));
        assert!(result.sections_used.contains(&"section2".to_string()));
    }

    #[test]
    fn test_prompt_builder_validate_prefix_stability_empty_sections() {
        let builder = PromptBuilder::new();
        let ctx = make_test_context();
        let state = PromptState::new();

        // No sections with Some(hash) - should return false since empty prefix hash != provided hash
        let result = builder
            .validate_prefix_stability(&ctx, &state, Some("any_hash"))
            .unwrap();
        assert!(!result);

        // No sections with None - should return true (first call, no previous hash)
        let result = builder
            .validate_prefix_stability(&ctx, &state, None)
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_effective_prompt_config_chaining() {
        // Test that builder methods can be chained
        let config = EffectivePromptConfig::new()
            .with_override("1".to_string())
            .with_coordinator("2".to_string())
            .with_agent("3".to_string())
            .with_custom("4".to_string())
            .with_append("5".to_string())
            .with_coordinator_mode(true);

        assert!(config.override_prompt.is_some());
        assert!(config.coordinator_prompt.is_some());
        assert!(config.agent_prompt.is_some());
        assert!(config.custom_prompt.is_some());
        assert!(config.append_prompt.is_some());
        assert!(config.use_coordinator_mode);
    }

    #[test]
    fn test_prompt_builder_resolve_hash_changes_with_content() {
        let ctx = make_test_context();
        let mut state = PromptState::new();

        let builder1 = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "content1"),
        ));
        let result1 = builder1.resolve(&ctx, &mut state).unwrap();

        state.clear_all();

        let builder2 = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "content2"),
        ));
        let result2 = builder2.resolve(&ctx, &mut state).unwrap();

        assert_ne!(result1.static_hash, result2.static_hash);
    }

    #[test]
    fn test_prompt_builder_resolve_hash_same_for_same_content() {
        let ctx = make_test_context();
        let mut state1 = PromptState::new();
        let mut state2 = PromptState::new();

        let builder1 = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "same content"),
        ));
        let result1 = builder1.resolve(&ctx, &mut state1).unwrap();

        let builder2 = PromptBuilder::new().add_section(Box::new(
            MockSection::new("test", SectionCaching::Cached, "same content"),
        ));
        let result2 = builder2.resolve(&ctx, &mut state2).unwrap();

        assert_eq!(result1.static_hash, result2.static_hash);
    }

    #[test]
    fn test_effective_prompt_config_with_prompt() {
        let config = EffectivePromptConfig::new()
            .with_prompt("static base prompt".to_string());
        assert_eq!(config.prompt, Some("static base prompt".to_string()));
    }

    #[test]
    fn test_build_effective_prompt_with_prompt_and_dynamic() {
        let builder =
            PromptBuilder::new().add_section(Box::new(MockSection::new(
                "dynamic",
                SectionCaching::Volatile,
                "dynamic content",
            )));

        let ctx = make_test_context();
        let mut state = PromptState::new();
        let config =
            EffectivePromptConfig::new().with_prompt("static base".to_string());

        let result = builder
            .build_effective_prompt(&ctx, &mut state, config)
            .unwrap();

        assert!(result.contains("static base"));
        assert!(result.contains(SYSTEM_PROMPT_DYNAMIC_BOUNDARY));
        assert!(result.contains("dynamic content"));
    }

    #[test]
    fn test_build_effective_prompt_with_prompt_no_dynamic() {
        let builder = PromptBuilder::new();
        // No sections at all → resolved dynamic content will be empty

        let ctx = make_test_context();
        let mut state = PromptState::new();
        let config =
            EffectivePromptConfig::new().with_prompt("static only".to_string());

        let result = builder
            .build_effective_prompt(&ctx, &mut state, config)
            .unwrap();

        // No dynamic content, so prompt is used directly without boundary
        assert_eq!(result, "static only");
    }
}
