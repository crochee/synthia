//! Team mode prompt section

use anyhow::Result;

use crate::prompt::{AgentName, PromptContext, SectionCaching};

/// Team role for prompt context.
#[derive(Debug, Clone)]
pub struct TeamPromptInfo {
    pub role: String,
    pub team_id: String,
    pub member_id: Option<String>,
}

/// Team mode section for role-specific prompts.
#[derive(Debug, Clone)]
pub struct TeamModeSection {
    agent_name: AgentName,
    team_info: TeamPromptInfo,
}

impl TeamModeSection {
    /// Creates a new team mode section.
    pub fn new(agent_name: AgentName, team_info: TeamPromptInfo) -> Self {
        Self {
            agent_name,
            team_info,
        }
    }
}

impl super::PromptSection for TeamModeSection {
    fn name(&self) -> &str {
        "team_mode"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::SessionCached
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        let mut content = String::new();

        match &self.agent_name {
            AgentName::Solo => {
                // No team-specific content for solo mode
            }
            AgentName::Lead => {
                content.push_str("# Team Mode\n\n");
                content.push_str("Role: Team Lead\n");
                content.push_str(&format!(
                    "Team ID: {}\n",
                    self.team_info.team_id
                ));

                content.push_str("\n## Team Responsibilities\n\n");
                content
                    .push_str("- Coordinate team members and delegate tasks\n");
                content.push_str("- Monitor team progress and assign work\n");
                content.push_str("- Broadcast messages to team when needed\n");
                content.push_str("- Approve or reject teammate plans\n");
            }
            AgentName::Custom(member_id) => {
                content.push_str("# Team Mode\n\n");
                content.push_str("Role: Team Member\n");
                content.push_str(&format!(
                    "Team ID: {}\n",
                    self.team_info.team_id
                ));
                content.push_str(&format!("Member ID: {member_id}\n"));

                content.push_str("\n## Team Responsibilities\n\n");
                content.push_str("- Execute assigned tasks efficiently\n");
                content.push_str(
                    "- Send updates to team lead via `send_to_lead`\n",
                );
                content.push_str(
                    "- Read your inbox regularly for new assignments\n",
                );
            }
        }

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::LazyLock};

    use super::*;
    use crate::prompt::PromptSection;

    static TEST_AGENT_NAME: LazyLock<AgentName> =
        LazyLock::new(|| AgentName::Custom("TestAgent".to_string()));

    fn make_ctx() -> PromptContext<'static> {
        PromptContext {
            agent_name: &TEST_AGENT_NAME,
            agent_description: "A test agent",
            workspace_dir: Path::new("/tmp/test"),
            skill_instructions: String::new(),
            is_subagent: false,
            session_id: Some("test-session"),
            mcp_servers: &[],
            additional_dirs: &[],
            output_style: None,
            language_preference: None,
            is_proactive_mode: false,
            model_name: Some("claude-sonnet"),
            knowledge_cutoff: Some("2026-03-01"),
            team_info: None,
        }
    }

    #[test]
    fn test_team_mode_section_name() {
        let section = TeamModeSection::new(
            AgentName::Solo,
            TeamPromptInfo {
                role: "Lead".to_string(),
                team_id: "team-1".to_string(),
                member_id: None,
            },
        );
        assert_eq!(section.name(), "team_mode");
    }

    #[test]
    fn test_team_mode_section_caching() {
        let section = TeamModeSection::new(
            AgentName::Solo,
            TeamPromptInfo {
                role: "Lead".to_string(),
                team_id: "team-1".to_string(),
                member_id: None,
            },
        );
        assert_eq!(section.caching(), SectionCaching::SessionCached);
    }

    #[test]
    fn test_team_mode_section_solo_empty() {
        let section = TeamModeSection::new(
            AgentName::Solo,
            TeamPromptInfo {
                role: "Solo".to_string(),
                team_id: String::new(),
                member_id: None,
            },
        );
        let ctx = make_ctx();
        let content = section.build(&ctx).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_team_mode_section_lead() {
        let section = TeamModeSection::new(
            AgentName::Lead,
            TeamPromptInfo {
                role: "Lead".to_string(),
                team_id: "team-1".to_string(),
                member_id: Some("lead-1".to_string()),
            },
        );
        let ctx = make_ctx();
        let content = section.build(&ctx).unwrap();
        assert!(content.contains("Team Mode"));
        assert!(content.contains("Team Lead"));
        assert!(content.contains("team-1"));
        assert!(content.contains("Coordinate team members"));
    }

    #[test]
    fn test_team_mode_section_member() {
        let section = TeamModeSection::new(
            AgentName::Custom("member-1".to_string()),
            TeamPromptInfo {
                role: "Member".to_string(),
                team_id: "team-1".to_string(),
                member_id: Some("member-1".to_string()),
            },
        );
        let ctx = make_ctx();
        let content = section.build(&ctx).unwrap();
        assert!(content.contains("Team Mode"));
        assert!(content.contains("Team Member"));
        assert!(content.contains("Execute assigned tasks"));
        assert!(content.contains("member-1"));
    }

    #[test]
    fn test_team_prompt_info_debug() {
        let info = TeamPromptInfo {
            role: "Lead".to_string(),
            team_id: "team-1".to_string(),
            member_id: Some("lead-1".to_string()),
        };
        let debug = format!("{info:?}");
        assert!(debug.contains("Lead"));
        assert!(debug.contains("team-1"));
    }
}
