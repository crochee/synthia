//! The two [`PromptBuilder`] presets.
//!
//! - [`default_with_sections`]: every section the Solo agent needs.
//! - [`build_for_name`]: same set, but the `TeamModeSection` is
//!   parameterised by [`AgentName`] so Lead / Member roles get the
//!   right `role` label.

use super::core::PromptBuilder;
use crate::prompt::{
    AgentName,
    DynamicMcpInstructionsSection,
    EnvironmentSection,
    IdentitySection,
    LanguageSection,
    MemorySection,
    OutputStyleSection,
    ProactiveSection,
    SkillSection,
    SystemSection,
    TaskExecutionSection,
    TeamModeSection,
    TeamPromptInfo,
    TokenBudgetSection,
    ToolUsageSection,
    sections::agents_md::AgentsMdSection,
};

impl PromptBuilder {
    /// Build a PromptBuilder with all 13 default sections in the
    /// canonical order. The `TeamModeSection` is set for `AgentName::Solo`.
    pub fn default_with_sections() -> Self {
        Self::new()
            .add_section(Box::new(IdentitySection))
            .add_section(Box::new(SystemSection))
            .add_section(Box::new(TaskExecutionSection))
            .add_section(Box::new(ToolUsageSection))
            .add_section(Box::new(EnvironmentSection::new()))
            .add_section(Box::new(AgentsMdSection::new()))
            .add_section(Box::new(MemorySection::new()))
            .add_section(Box::new(SkillSection::new()))
            .add_section(Box::new(DynamicMcpInstructionsSection::new(vec![])))
            .add_section(Box::new(OutputStyleSection::default()))
            .add_section(Box::new(LanguageSection::default()))
            .add_section(Box::new(ProactiveSection::new()))
            .add_section(Box::new(TokenBudgetSection::new()))
            .add_section(Box::new(TeamModeSection::new(
                AgentName::Solo,
                TeamPromptInfo {
                    role: String::new(),
                    team_id: String::new(),
                    member_id: None,
                },
            )))
    }

    /// Build a PromptBuilder for a specific agent name. The section
    /// set is identical to [`default_with_sections`], but the
    /// `TeamModeSection` is parameterised by the agent's role
    /// (Solo / Lead / Member) so multi-agent teams get a useful label.
    pub fn build_for_name(name: &AgentName) -> Self {
        let team_info = TeamPromptInfo {
            role: match name {
                AgentName::Solo => "Solo".to_string(),
                AgentName::Lead => "Lead".to_string(),
                AgentName::Custom(_) => "Member".to_string(),
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
            .add_section(Box::new(AgentsMdSection::new()))
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
}
