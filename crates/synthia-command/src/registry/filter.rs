use super::definition::CommandDefinition;

#[derive(Clone, Debug, Default)]
pub struct CommandFilter {
    pub enabled_only: bool,
}

impl CommandFilter {
    pub fn matches_command(&self, _cmd: &CommandDefinition) -> bool {
        true
    }
}
