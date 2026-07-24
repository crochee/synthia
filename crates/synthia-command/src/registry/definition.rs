use serde::{Deserialize, Serialize};
use synthia_core::registry::RegistryItem;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub name: String,
    pub description: String,
}

impl RegistryItem for CommandDefinition {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}
