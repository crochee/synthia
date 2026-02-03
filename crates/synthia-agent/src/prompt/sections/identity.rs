use super::{PromptSection, inject_workspace_files};
use crate::prompt::{PromptContext, Result, SectionCaching};

const CYBER_RISK_INSTRUCTION: &str = "Assist with authorized security testing, defensive security, CTF challenges, and educational contexts. Refuse requests for destructive techniques, DoS attacks, mass targeting, supply chain compromise, or detection evasion for malicious purposes.";

const INTRO_TEMPLATES: &[(&str, &str)] = &[
    (
        "output_style",
        "You are {agent_name}.\n{agent_description}\n\nUse tools to complete tasks according to your \"Output Style\" below.\n\n{CYBER_RISK_INSTRUCTION}\n\nNever generate or guess URLs unless for programming help.",
    ),
    (
        "subagent",
        "You are {agent_name}, a subagent.\n{agent_description}\n\nFocus on assigned task. Stay within scope.",
    ),
    (
        "default",
        "You are {agent_name}.\n{agent_description}\n\n{CYBER_RISK_INSTRUCTION}\n\nNever generate or guess URLs unless for programming help.",
    ),
];

const WORKSPACE_FILES: &[&str] =
    &["AGENTS.md", "IDENTITY.md", "USER.md", "MEMORY.md"];

#[derive(Debug, Clone, Copy, Default)]
pub struct IdentitySection;

impl IdentitySection {
    pub fn new() -> Self {
        Self
    }
}

impl PromptSection for IdentitySection {
    fn name(&self) -> &str {
        "identity"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::Cached
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let template = if ctx.is_subagent {
            INTRO_TEMPLATES[1].1
        } else if ctx.output_style.is_some_and(|s| !s.prompt.is_empty()) {
            INTRO_TEMPLATES[0].1
        } else {
            INTRO_TEMPLATES[2].1
        };

        let mut prompt = template
            .replace("{agent_name}", ctx.agent_name)
            .replace("{agent_description}", ctx.agent_description)
            .replace("{CYBER_RISK_INSTRUCTION}", CYBER_RISK_INSTRUCTION);

        if has_workspace_files(ctx.workspace_dir, WORKSPACE_FILES) {
            prompt.push('\n');
            inject_workspace_files(
                &mut prompt,
                ctx.workspace_dir,
                WORKSPACE_FILES,
            );
        }

        Ok(prompt)
    }
}

pub fn has_workspace_files(
    workspace_dir: &std::path::Path,
    files: &[&str],
) -> bool {
    files.iter().any(|f| workspace_dir.join(f).exists())
}
